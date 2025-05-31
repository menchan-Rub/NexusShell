/**
 * 分散リソース管理モジュール
 * 
 * 分散パイプライン実行におけるリソース管理と負荷分散を担当するモジュール
 */

use std::collections::{HashMap, HashSet, BinaryHeap};
use std::cmp::{Ordering, Reverse};
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use super::node::{NodeId, NodeInfo, NodeStatus, NodeCapabilities, NodeLoad};
use super::task::{DistributedTask, TaskStatus};

/// リソース種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    /// CPU
    Cpu,
    /// メモリ
    Memory,
    /// ディスク容量
    DiskSpace,
    /// ネットワーク帯域
    NetworkBandwidth,
    /// GPU
    Gpu,
}

/// リソース量（64ビット浮動小数点数）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceQuantity(f64);

impl ResourceQuantity {
    /// 新しいリソース量を作成
    pub fn new(value: f64) -> Self {
        Self(value.max(0.0))
    }
    
    /// リソース量の値を取得
    pub fn value(&self) -> f64 {
        self.0
    }
    
    /// リソース量を加算
    pub fn add(&self, other: &Self) -> Self {
        Self(self.0 + other.0)
    }
    
    /// リソース量を減算
    pub fn subtract(&self, other: &Self) -> Self {
        Self((self.0 - other.0).max(0.0))
    }
    
    /// リソース量を乗算
    pub fn multiply(&self, factor: f64) -> Self {
        Self(self.0 * factor.max(0.0))
    }
    
    /// リソース量を割り算
    pub fn divide(&self, divisor: f64) -> Self {
        if divisor <= 0.0 {
            Self(0.0)
        } else {
            Self(self.0 / divisor)
        }
    }
    
    /// リソース利用率を計算
    pub fn usage_ratio(&self, capacity: &Self) -> f64 {
        if capacity.0 <= 0.0 {
            0.0
        } else {
            (self.0 / capacity.0).min(1.0)
        }
    }
}

/// リソース要件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// 必要なリソース
    pub resources: HashMap<ResourceType, ResourceQuantity>,
    /// タイムアウト（秒）
    pub timeout_sec: Option<u64>,
    /// 優先度（0-100）
    pub priority: u8,
    /// 特定のノードに割り当てる
    pub node_affinity: Option<HashSet<NodeId>>,
    /// 特定のノードを避ける
    pub node_anti_affinity: Option<HashSet<NodeId>>,
}

impl ResourceRequirements {
    /// 新しいリソース要件を作成
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            timeout_sec: None,
            priority: 50,
            node_affinity: None,
            node_anti_affinity: None,
        }
    }
    
    /// リソースを追加
    pub fn add_resource(&mut self, resource_type: ResourceType, quantity: ResourceQuantity) -> &mut Self {
        self.resources.insert(resource_type, quantity);
        self
    }
    
    /// タイムアウトを設定
    pub fn with_timeout(&mut self, timeout_sec: u64) -> &mut Self {
        self.timeout_sec = Some(timeout_sec);
        self
    }
    
    /// 優先度を設定
    pub fn with_priority(&mut self, priority: u8) -> &mut Self {
        self.priority = priority.min(100);
        self
    }
    
    /// ノードアフィニティを設定
    pub fn with_node_affinity(&mut self, nodes: HashSet<NodeId>) -> &mut Self {
        self.node_affinity = Some(nodes);
        self
    }
    
    /// ノードアンチアフィニティを設定
    pub fn with_node_anti_affinity(&mut self, nodes: HashSet<NodeId>) -> &mut Self {
        self.node_anti_affinity = Some(nodes);
        self
    }
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self::new()
    }
}

/// ノードリソース状態
#[derive(Debug, Clone)]
pub struct NodeResources {
    /// ノードID
    pub node_id: NodeId,
    /// 総リソース容量
    pub capacity: HashMap<ResourceType, ResourceQuantity>,
    /// 現在の使用量
    pub usage: HashMap<ResourceType, ResourceQuantity>,
    /// 割り当て済みタスク
    pub assigned_tasks: HashSet<String>,
    /// ノード負荷
    pub load: NodeLoad,
    /// 最終更新時間
    pub last_updated: Instant,
}

impl NodeResources {
    /// 新しいノードリソース状態を作成
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            capacity: HashMap::new(),
            usage: HashMap::new(),
            assigned_tasks: HashSet::new(),
            load: NodeLoad::default(),
            last_updated: Instant::now(),
        }
    }
    
    /// ノード情報から作成
    pub fn from_node_info(node: &NodeInfo) -> Self {
        let mut resources = Self::new(node.id.clone());
        
        // ノードの能力からリソース容量を設定
        resources.capacity.insert(
            ResourceType::Cpu,
            ResourceQuantity::new(node.capabilities.available_cores as f64),
        );
        
        resources.capacity.insert(
            ResourceType::Memory,
            ResourceQuantity::new(node.capabilities.available_memory as f64),
        );
        
        resources.capacity.insert(
            ResourceType::DiskSpace,
            ResourceQuantity::new(node.capabilities.disk_space as f64),
        );
        
        resources.capacity.insert(
            ResourceType::NetworkBandwidth,
            ResourceQuantity::new(node.capabilities.network_bandwidth as f64),
        );
        
        resources
    }
    
    /// リソース使用量を更新
    pub fn update_usage(&mut self, resource_type: ResourceType, usage: ResourceQuantity) {
        self.usage.insert(resource_type, usage);
        self.last_updated = Instant::now();
    }
    
    /// タスクを割り当て
    pub fn assign_task(&mut self, task_id: &str, requirements: &ResourceRequirements) -> Result<()> {
        // リソースが十分かチェック
        if !self.has_enough_resources(requirements) {
            return Err(anyhow!("ノード {} にリソースが不足しています", self.node_id));
        }
        
        // タスクを追加
        self.assigned_tasks.insert(task_id.to_string());
        
        // リソース使用量を更新
        for (resource_type, quantity) in &requirements.resources {
            let current = self.usage.get(resource_type)
                .cloned()
                .unwrap_or(ResourceQuantity::new(0.0));
                
            self.usage.insert(*resource_type, current.add(quantity));
        }
        
        Ok(())
    }
    
    /// タスクを解放
    pub fn release_task(&mut self, task_id: &str, requirements: &ResourceRequirements) -> Result<()> {
        // タスクが割り当てられているか確認
        if !self.assigned_tasks.remove(task_id) {
            return Err(anyhow!("タスク {} はノード {} に割り当てられていません", task_id, self.node_id));
        }
        
        // リソース使用量を更新
        for (resource_type, quantity) in &requirements.resources {
            let current = self.usage.get(resource_type)
                .cloned()
                .unwrap_or(ResourceQuantity::new(0.0));
                
            self.usage.insert(*resource_type, current.subtract(quantity));
        }
        
        Ok(())
    }
    
    /// 空きリソースを取得
    pub fn get_available_resources(&self) -> HashMap<ResourceType, ResourceQuantity> {
        let mut available = HashMap::new();
        
        for (resource_type, capacity) in &self.capacity {
            let usage = self.usage.get(resource_type)
                .cloned()
                .unwrap_or(ResourceQuantity::new(0.0));
                
            available.insert(*resource_type, capacity.subtract(&usage));
        }
        
        available
    }
    
    /// リソースが十分かチェック
    pub fn has_enough_resources(&self, requirements: &ResourceRequirements) -> bool {
        let available = self.get_available_resources();
        
        for (resource_type, required) in &requirements.resources {
            if let Some(available_quantity) = available.get(resource_type) {
                if available_quantity.value() < required.value() {
                    return false;
                }
            } else {
                return false;
            }
        }
        
        true
    }
    
    /// 使用率を計算
    pub fn calculate_usage_ratio(&self, resource_type: ResourceType) -> f64 {
        let capacity = self.capacity.get(&resource_type)
            .cloned()
            .unwrap_or(ResourceQuantity::new(0.0));
            
        let usage = self.usage.get(&resource_type)
            .cloned()
            .unwrap_or(ResourceQuantity::new(0.0));
            
        usage.usage_ratio(&capacity)
    }
    
    /// 総合負荷スコアを計算
    pub fn calculate_load_score(&self) -> f64 {
        let cpu_weight = 0.4;
        let memory_weight = 0.3;
        let disk_weight = 0.2;
        let network_weight = 0.1;
        
        let cpu_ratio = self.calculate_usage_ratio(ResourceType::Cpu);
        let memory_ratio = self.calculate_usage_ratio(ResourceType::Memory);
        let disk_ratio = self.calculate_usage_ratio(ResourceType::DiskSpace);
        let network_ratio = self.calculate_usage_ratio(ResourceType::NetworkBandwidth);
        
        (cpu_ratio * cpu_weight) +
        (memory_ratio * memory_weight) +
        (disk_ratio * disk_weight) +
        (network_ratio * network_weight)
    }
}

/// 負荷分散アルゴリズム
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancingAlgorithm {
    /// ラウンドロビン
    RoundRobin,
    /// 最小負荷
    LeastLoaded,
    /// リソース適合度
    ResourceFit,
    /// 重み付きランダム
    WeightedRandom,
    /// ローカリティ優先
    LocalityAware,
}

/// リソース割り当て
#[derive(Debug, Clone)]
pub struct ResourceAllocation {
    /// タスクID
    pub task_id: String,
    /// 割り当てられたノード
    pub node_id: NodeId,
    /// 割り当てタイムスタンプ
    pub timestamp: Instant,
    /// 割り当てリソース
    pub allocated_resources: HashMap<ResourceType, ResourceQuantity>,
    /// タイムアウト（秒）
    pub timeout_sec: Option<u64>,
}

/// リソース強制解放理由
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreemptionReason {
    /// 優先度の高いタスク
    HigherPriorityTask,
    /// リソース不足
    ResourceShortage,
    /// ノード障害
    NodeFailure,
    /// システム要求
    SystemRequested,
}

/// リソース管理設定
#[derive(Debug, Clone)]
pub struct ResourceManagerConfig {
    /// 負荷分散アルゴリズム
    pub load_balancing_algorithm: LoadBalancingAlgorithm,
    /// リソース割り当てタイムアウト（秒）
    pub allocation_timeout_sec: u64,
    /// 状態更新間隔（秒）
    pub state_update_interval_sec: u64,
    /// 強制解放を許可
    pub allow_preemption: bool,
    /// リソース超過割り当て係数
    pub overcommit_factor: f64,
    /// リソース予約割合
    pub reservation_percentage: f64,
}

impl Default for ResourceManagerConfig {
    fn default() -> Self {
        Self {
            load_balancing_algorithm: LoadBalancingAlgorithm::LeastLoaded,
            allocation_timeout_sec: 30,
            state_update_interval_sec: 10,
            allow_preemption: false,
            overcommit_factor: 1.0,
            reservation_percentage: 0.1,
        }
    }
}

/// リソースマネージャー
pub struct ResourceManager {
    /// リソース管理設定
    config: ResourceManagerConfig,
    /// ノードリソース状態
    node_resources: Arc<RwLock<HashMap<NodeId, NodeResources>>>,
    /// リソース割り当て
    allocations: Arc<RwLock<HashMap<String, ResourceAllocation>>>,
    /// ラウンドロビンインデックス
    round_robin_index: Arc<Mutex<usize>>,
}

impl ResourceManager {
    /// 新しいリソースマネージャーを作成
    pub fn new(config: ResourceManagerConfig) -> Self {
        Self {
            config,
            node_resources: Arc::new(RwLock::new(HashMap::new())),
            allocations: Arc::new(RwLock::new(HashMap::new())),
            round_robin_index: Arc::new(Mutex::new(0)),
        }
    }
    
    /// ノードを登録
    pub async fn register_node(&self, node: &NodeInfo) -> Result<()> {
        let node_id = node.id.clone();
        let resources = NodeResources::from_node_info(node);
        
        let mut nodes = self.node_resources.write().await;
        nodes.insert(node_id, resources);
        
        debug!("ノード {} を登録しました", node.id);
        Ok(())
    }
    
    /// ノードを登録解除
    pub async fn unregister_node(&self, node_id: &NodeId) -> Result<()> {
        let mut nodes = self.node_resources.write().await;
        
        if nodes.remove(node_id).is_some() {
            debug!("ノード {} の登録を解除しました", node_id);
            
            // 割り当て済みのタスクを解放
            self.release_node_allocations(node_id).await?;
            
            Ok(())
        } else {
            Err(anyhow!("ノード {} は登録されていません", node_id))
        }
    }
    
    /// ノードのリソース状態を更新
    pub async fn update_node_resources(&self, node_id: &NodeId, load: NodeLoad) -> Result<()> {
        let mut nodes = self.node_resources.write().await;
        
        if let Some(resources) = nodes.get_mut(node_id) {
            // ノード負荷を更新
            resources.load = load;
            
            // リソース使用量を更新
            resources.update_usage(
                ResourceType::Cpu,
                ResourceQuantity::new((load.cpu_usage as f64) / 100.0 * resources.capacity
                    .get(&ResourceType::Cpu)
                    .unwrap_or(&ResourceQuantity::new(1.0))
                    .value()),
            );
            
            resources.update_usage(
                ResourceType::Memory,
                ResourceQuantity::new((load.memory_usage as f64) / 100.0 * resources.capacity
                    .get(&ResourceType::Memory)
                    .unwrap_or(&ResourceQuantity::new(1.0))
                    .value()),
            );
            
            resources.update_usage(
                ResourceType::DiskSpace,
                ResourceQuantity::new((load.disk_usage as f64) / 100.0 * resources.capacity
                    .get(&ResourceType::DiskSpace)
                    .unwrap_or(&ResourceQuantity::new(1.0))
                    .value()),
            );
            
            resources.update_usage(
                ResourceType::NetworkBandwidth,
                ResourceQuantity::new((load.network_usage as f64) / 100.0 * resources.capacity
                    .get(&ResourceType::NetworkBandwidth)
                    .unwrap_or(&ResourceQuantity::new(1.0))
                    .value()),
            );
            
            debug!("ノード {} のリソース状態を更新しました", node_id);
            Ok(())
        } else {
            Err(anyhow!("ノード {} は登録されていません", node_id))
        }
    }
    
    /// タスクのリソースを割り当て
    pub async fn allocate_resources(
        &self,
        task_id: &str,
        requirements: &ResourceRequirements,
    ) -> Result<NodeId> {
        // 割り当て済みかチェック
        {
            let allocations = self.allocations.read().await;
            if allocations.contains_key(task_id) {
                return Err(anyhow!("タスク {} は既に割り当てられています", task_id));
            }
        }
        
        // 負荷分散アルゴリズムに基づいてノードを選択
        let selected_node = match self.config.load_balancing_algorithm {
            LoadBalancingAlgorithm::RoundRobin => {
                self.select_node_round_robin(requirements).await?
            },
            LoadBalancingAlgorithm::LeastLoaded => {
                self.select_node_least_loaded(requirements).await?
            },
            LoadBalancingAlgorithm::ResourceFit => {
                self.select_node_resource_fit(requirements).await?
            },
            LoadBalancingAlgorithm::WeightedRandom => {
                self.select_node_weighted_random(requirements).await?
            },
            LoadBalancingAlgorithm::LocalityAware => {
                self.select_node_locality_aware(requirements).await?
            },
        };
        
        // リソースを割り当て
        {
            let mut nodes = self.node_resources.write().await;
            
            if let Some(resources) = nodes.get_mut(&selected_node) {
                resources.assign_task(task_id, requirements)?;
            } else {
                return Err(anyhow!("ノード {} は登録されていません", selected_node));
            }
        }
        
        // 割り当て情報を保存
        {
            let mut allocations = self.allocations.write().await;
            
            let mut allocated_resources = HashMap::new();
            for (resource_type, quantity) in &requirements.resources {
                allocated_resources.insert(*resource_type, *quantity);
            }
            
            allocations.insert(task_id.to_string(), ResourceAllocation {
                task_id: task_id.to_string(),
                node_id: selected_node.clone(),
                timestamp: Instant::now(),
                allocated_resources,
                timeout_sec: requirements.timeout_sec,
            });
        }
        
        info!("タスク {} のリソースをノード {} に割り当てました", task_id, selected_node);
        Ok(selected_node)
    }
    
    /// タスクのリソースを解放
    pub async fn release_resources(&self, task_id: &str) -> Result<()> {
        // 割り当て情報を取得
        let allocation = {
            let allocations = self.allocations.read().await;
            allocations.get(task_id).cloned()
        };
        
        if let Some(allocation) = allocation {
            // ノードからリソースを解放
            {
                let mut nodes = self.node_resources.write().await;
                
                if let Some(resources) = nodes.get_mut(&allocation.node_id) {
                    // リソース要件を再構築
                    let mut requirements = ResourceRequirements::new();
                    for (resource_type, quantity) in &allocation.allocated_resources {
                        requirements.add_resource(*resource_type, *quantity);
                    }
                    
                    resources.release_task(task_id, &requirements)?;
                }
            }
            
            // 割り当て情報を削除
            {
                let mut allocations = self.allocations.write().await;
                allocations.remove(task_id);
            }
            
            info!("タスク {} のリソースを解放しました", task_id);
            Ok(())
        } else {
            Err(anyhow!("タスク {} の割り当てが見つかりません", task_id))
        }
    }
    
    /// ノードの割り当てをすべて解放
    async fn release_node_allocations(&self, node_id: &NodeId) -> Result<()> {
        // ノードに割り当てられたタスクを取得
        let task_ids = {
            let allocations = self.allocations.read().await;
            allocations.values()
                .filter(|allocation| allocation.node_id == *node_id)
                .map(|allocation| allocation.task_id.clone())
                .collect::<Vec<_>>()
        };
        
        // 各タスクのリソースを解放
        for task_id in task_ids {
            if let Err(e) = self.release_resources(&task_id).await {
                warn!("タスク {} のリソース解放中にエラー: {}", task_id, e);
            }
        }
        
        Ok(())
    }
    
    /// ラウンドロビンでノードを選択
    async fn select_node_round_robin(&self, requirements: &ResourceRequirements) -> Result<NodeId> {
        let nodes = self.node_resources.read().await;
        
        if nodes.is_empty() {
            return Err(anyhow!("利用可能なノードがありません"));
        }
        
        // ノードのリスト
        let node_ids: Vec<NodeId> = nodes.keys().cloned().collect();
        
        // ラウンドロビンインデックスを更新
        let mut index = self.round_robin_index.lock().await;
        *index = (*index + 1) % node_ids.len();
        
        // 候補ノードを順に確認
        for i in 0..node_ids.len() {
            let check_index = (*index + i) % node_ids.len();
            let node_id = &node_ids[check_index];
            
            if let Some(resources) = nodes.get(node_id) {
                // ノードアフィニティをチェック
                if let Some(ref affinity) = requirements.node_affinity {
                    if !affinity.contains(node_id) {
                        continue;
                    }
                }
                
                // ノードアンチアフィニティをチェック
                if let Some(ref anti_affinity) = requirements.node_anti_affinity {
                    if anti_affinity.contains(node_id) {
                        continue;
                    }
                }
                
                // リソースが十分かチェック
                if resources.has_enough_resources(requirements) {
                    return Ok(node_id.clone());
                }
            }
        }
        
        Err(anyhow!("要件を満たすノードが見つかりません"))
    }
    
    /// 最小負荷でノードを選択
    async fn select_node_least_loaded(&self, requirements: &ResourceRequirements) -> Result<NodeId> {
        let nodes = self.node_resources.read().await;
        
        if nodes.is_empty() {
            return Err(anyhow!("利用可能なノードがありません"));
        }
        
        // 候補ノードとそのスコアのリスト
        let mut candidates = Vec::new();
        
        for (node_id, resources) in nodes.iter() {
            // ノードアフィニティをチェック
            if let Some(ref affinity) = requirements.node_affinity {
                if !affinity.contains(node_id) {
                    continue;
                }
            }
            
            // ノードアンチアフィニティをチェック
            if let Some(ref anti_affinity) = requirements.node_anti_affinity {
                if anti_affinity.contains(node_id) {
                    continue;
                }
            }
            
            // リソースが十分かチェック
            if resources.has_enough_resources(requirements) {
                // 負荷スコアを計算
                let load_score = resources.calculate_load_score();
                candidates.push((node_id.clone(), load_score));
            }
        }
        
        if candidates.is_empty() {
            return Err(anyhow!("要件を満たすノードが見つかりません"));
        }
        
        // 負荷の最も低いノードを選択
        candidates.sort_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        Ok(candidates[0].0.clone())
    }
    
    /// リソースの適合度でノードを選択
    async fn select_node_resource_fit(&self, requirements: &ResourceRequirements) -> Result<NodeId> {
        let nodes = self.node_resources.read().await;
        
        if nodes.is_empty() {
            return Err(anyhow!("利用可能なノードがありません"));
        }
        
        // 候補ノードとそのスコアのリスト
        let mut candidates = Vec::new();
        
        for (node_id, resources) in nodes.iter() {
            // ノードアフィニティをチェック
            if let Some(ref affinity) = requirements.node_affinity {
                if !affinity.contains(node_id) {
                    continue;
                }
            }
            
            // ノードアンチアフィニティをチェック
            if let Some(ref anti_affinity) = requirements.node_anti_affinity {
                if anti_affinity.contains(node_id) {
                    continue;
                }
            }
            
            // リソースが十分かチェック
            if resources.has_enough_resources(requirements) {
                // 適合度スコアを計算
                let mut fit_score = 0.0;
                let available = resources.get_available_resources();
                
                for (resource_type, required) in &requirements.resources {
                    if let Some(avail) = available.get(resource_type) {
                        // リソースの余裕度をスコアとして加算
                        // 余裕が少ないほどスコアが低い（より適合度が高い）
                        let margin = avail.value() - required.value();
                        fit_score += margin;
                    }
                }
                
                candidates.push((node_id.clone(), fit_score));
            }
        }
        
        if candidates.is_empty() {
            return Err(anyhow!("要件を満たすノードが見つかりません"));
        }
        
        // 最も適合度の高い（余剰リソースが少ない）ノードを選択
        candidates.sort_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        Ok(candidates[0].0.clone())
    }
    
    /// 重み付きランダムでノードを選択
    async fn select_node_weighted_random(&self, requirements: &ResourceRequirements) -> Result<NodeId> {
        // 簡易実装のため、最小負荷アルゴリズムを使用
        self.select_node_least_loaded(requirements).await
    }
    
    /// ローカリティ優先でノードを選択
    async fn select_node_locality_aware(&self, requirements: &ResourceRequirements) -> Result<NodeId> {
        // 簡易実装のため、リソース適合度アルゴリズムを使用
        self.select_node_resource_fit(requirements).await
    }
    
    /// ノードのリソース状態を取得
    pub async fn get_node_resources(&self, node_id: &NodeId) -> Option<NodeResources> {
        let nodes = self.node_resources.read().await;
        nodes.get(node_id).cloned()
    }
    
    /// すべてのノードリソース状態を取得
    pub async fn get_all_node_resources(&self) -> Vec<NodeResources> {
        let nodes = self.node_resources.read().await;
        nodes.values().cloned().collect()
    }
    
    /// タスクの割り当て情報を取得
    pub async fn get_task_allocation(&self, task_id: &str) -> Option<ResourceAllocation> {
        let allocations = self.allocations.read().await;
        allocations.get(task_id).cloned()
    }
    
    /// ノードに割り当てられたタスクを取得
    pub async fn get_node_tasks(&self, node_id: &NodeId) -> Vec<String> {
        let nodes = self.node_resources.read().await;
        
        if let Some(resources) = nodes.get(node_id) {
            resources.assigned_tasks.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }
    
    /// クラスター全体のリソース使用状況を取得
    pub async fn get_cluster_resource_usage(&self) -> HashMap<ResourceType, (ResourceQuantity, ResourceQuantity)> {
        let nodes = self.node_resources.read().await;
        
        let mut usage = HashMap::new();
        
        // 各リソースタイプの合計容量と使用量を計算
        for resources in nodes.values() {
            for (resource_type, capacity) in &resources.capacity {
                let total_capacity = usage
                    .entry(*resource_type)
                    .or_insert((ResourceQuantity::new(0.0), ResourceQuantity::new(0.0)))
                    .0;
                
                *total_capacity = total_capacity.add(capacity);
            }
            
            for (resource_type, used) in &resources.usage {
                let total_usage = &mut usage
                    .entry(*resource_type)
                    .or_insert((ResourceQuantity::new(0.0), ResourceQuantity::new(0.0)))
                    .1;
                
                *total_usage = total_usage.add(used);
            }
        }
        
        usage
    }
    
    /// 期限切れの割り当てをクリーンアップ
    pub async fn cleanup_expired_allocations(&self) -> Result<usize> {
        let now = Instant::now();
        let expired_task_ids = {
            let allocations = self.allocations.read().await;
            
            allocations.iter()
                .filter_map(|(task_id, allocation)| {
                    if let Some(timeout_sec) = allocation.timeout_sec {
                        let elapsed = now.duration_since(allocation.timestamp);
                        if elapsed > Duration::from_secs(timeout_sec) {
                            Some(task_id.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        
        let count = expired_task_ids.len();
        
        for task_id in expired_task_ids {
            if let Err(e) = self.release_resources(&task_id).await {
                warn!("期限切れ割り当て {} の解放中にエラー: {}", task_id, e);
            }
        }
        
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resource_quantity() {
        let r1 = ResourceQuantity::new(10.0);
        let r2 = ResourceQuantity::new(5.0);
        
        assert_eq!(r1.value(), 10.0);
        assert_eq!(r1.add(&r2).value(), 15.0);
        assert_eq!(r1.subtract(&r2).value(), 5.0);
        assert_eq!(r1.multiply(2.0).value(), 20.0);
        assert_eq!(r1.divide(2.0).value(), 5.0);
        
        // 負の値にならないことを確認
        assert_eq!(r2.subtract(&r1).value(), 0.0);
    }
    
    #[tokio::test]
    async fn test_resource_manager() {
        // リソースマネージャーを作成
        let config = ResourceManagerConfig::default();
        let resource_manager = ResourceManager::new(config);
        
        // テスト用ノード情報
        let node_id = NodeId::from_string("test-node".to_string());
        let mut node_info = NodeInfo::new(
            "test-node".to_string(),
            "localhost".to_string(),
            8080,
        );
        
        // ノード能力を設定
        node_info.capabilities.available_cores = 4;
        node_info.capabilities.available_memory = 8 * 1024 * 1024 * 1024; // 8GB
        node_info.capabilities.disk_space = 100 * 1024 * 1024 * 1024; // 100GB
        node_info.capabilities.network_bandwidth = 1000; // 1000Mbps
        
        // ノードを登録
        resource_manager.register_node(&node_info).await.unwrap();
        
        // ノードリソース状態を取得
        let resources = resource_manager.get_node_resources(&node_id).await.unwrap();
        
        // リソース容量を確認
        assert_eq!(
            resources.capacity.get(&ResourceType::Cpu).unwrap().value(),
            4.0
        );
        
        // リソース要件を作成
        let mut requirements = ResourceRequirements::new();
        requirements.add_resource(ResourceType::Cpu, ResourceQuantity::new(2.0));
        requirements.add_resource(ResourceType::Memory, ResourceQuantity::new(4.0 * 1024 * 1024 * 1024));
        
        // リソースを割り当て
        let task_id = "test-task";
        let assigned_node = resource_manager.allocate_resources(task_id, &requirements).await.unwrap();
        
        // 割り当てられたノードを確認
        assert_eq!(assigned_node, node_id);
        
        // 割り当て情報を取得
        let allocation = resource_manager.get_task_allocation(task_id).await.unwrap();
        assert_eq!(allocation.node_id, node_id);
        
        // ノードに割り当てられたタスクを確認
        let tasks = resource_manager.get_node_tasks(&node_id).await;
        assert!(tasks.contains(&task_id.to_string()));
        
        // リソースを解放
        resource_manager.release_resources(task_id).await.unwrap();
        
        // 割り当てが解放されたことを確認
        assert!(resource_manager.get_task_allocation(task_id).await.is_none());
        
        // ノードの割り当てタスクが空になったことを確認
        let tasks = resource_manager.get_node_tasks(&node_id).await;
        assert!(tasks.is_empty());
    }
} 