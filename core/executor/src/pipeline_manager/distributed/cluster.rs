/**
 * 分散クラスターモジュール
 * 
 * 分散パイプラインクラスターとノード管理を担当するモジュール
 */

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use super::node::{NodeId, NodeInfo, NodeStatus};

/// クラスター健全性状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterHealthStatus {
    /// 正常
    Healthy,
    /// 警告
    Warning,
    /// 危険
    Critical,
    /// 分断
    Partitioned,
    /// 回復中
    Recovering,
}

/// クラスター内役割
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterRole {
    /// マスターノード
    Master,
    /// ワーカーノード
    Worker,
    /// バックアップマスター
    BackupMaster,
    /// モニタリングノード
    Monitor,
    /// ゲートウェイノード
    Gateway,
}

/// クラスター設定
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// クラスタID
    pub cluster_id: String,
    /// クラスタシークレット
    pub cluster_secret: String,
    /// メンバーシップ確認インターバル
    pub membership_interval: Duration,
    /// ノード失敗検出タイムアウト
    pub failure_detection_timeout: Duration,
    /// マスター選出タイムアウト
    pub master_election_timeout: Duration,
    /// 最小クォーラムサイズ
    pub min_quorum_size: usize,
    /// 自動マスター選出を有効化
    pub enable_auto_election: bool,
    /// ノード間通信暗号化を有効化
    pub enable_encryption: bool,
    /// クラスタメタデータ
    pub metadata: HashMap<String, String>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            cluster_id: Uuid::new_v4().to_string(),
            cluster_secret: Uuid::new_v4().to_string(),
            membership_interval: Duration::from_secs(10),
            failure_detection_timeout: Duration::from_secs(30),
            master_election_timeout: Duration::from_secs(15),
            min_quorum_size: 1,
            enable_auto_election: true,
            enable_encryption: true,
            metadata: HashMap::new(),
        }
    }
}

/// クラスターノード情報
#[derive(Debug, Clone)]
pub struct ClusterNodeInfo {
    /// 基本ノード情報
    pub base_info: NodeInfo,
    /// クラスタ内の役割
    pub role: ClusterRole,
    /// 参加時間
    pub join_time: DateTime<Utc>,
    /// マスター選出の優先度
    pub election_priority: u8,
    /// メタデータ
    pub metadata: HashMap<String, String>,
    /// セキュリティ関連情報
    pub security_info: Option<NodeSecurityInfo>,
    /// ノード間の通信統計
    pub peer_stats: HashMap<NodeId, PeerCommunicationStats>,
}

/// ノードセキュリティ情報
#[derive(Debug, Clone)]
pub struct NodeSecurityInfo {
    /// 公開鍵
    pub public_key: String,
    /// 証明書のフィンガープリント
    pub cert_fingerprint: String,
    /// アクセストークン
    pub access_token: String,
    /// 最終認証時間
    pub last_authenticated: DateTime<Utc>,
}

/// ピア通信統計
#[derive(Debug, Clone)]
pub struct PeerCommunicationStats {
    /// 平均応答時間
    pub avg_response_time: Duration,
    /// 成功率
    pub success_rate: f64,
    /// 最終通信時間
    pub last_communication: Instant,
    /// 送信バイト数
    pub bytes_sent: u64,
    /// 受信バイト数
    pub bytes_received: u64,
}

/// クラスタートポロジー
#[derive(Debug)]
pub struct ClusterTopology {
    /// ノード間の接続グラフ
    pub connections: HashMap<NodeId, HashSet<NodeId>>,
    /// ネットワークの分断状態
    pub partitions: Vec<HashSet<NodeId>>,
    /// トポロジー更新時間
    pub last_updated: Instant,
    /// 健全性状態
    pub health_status: ClusterHealthStatus,
}

/// クラスターマネージャー
pub struct ClusterManager {
    /// クラスタ設定
    config: ClusterConfig,
    /// ノードレジストリ
    node_registry: Arc<RwLock<HashMap<NodeId, ClusterNodeInfo>>>,
    /// 現在のマスターノード
    current_master: Arc<RwLock<Option<NodeId>>>,
    /// バックアップマスターノード
    backup_masters: Arc<RwLock<Vec<NodeId>>>,
    /// クラスタトポロジ
    topology: Arc<RwLock<ClusterTopology>>,
    /// ハートビートレジストリ
    heartbeats: Arc<RwLock<HashMap<NodeId, Instant>>>,
    /// ローカルノードID
    local_node_id: NodeId,
}

impl ClusterManager {
    /// 新しいクラスターマネージャーを作成
    pub fn new(config: ClusterConfig, local_node: NodeInfo) -> Self {
        let local_node_id = local_node.id.clone();
        
        // ローカルノードのクラスター情報を作成
        let local_cluster_node = ClusterNodeInfo {
            base_info: local_node,
            role: ClusterRole::Worker, // デフォルトはワーカー
            join_time: Utc::now(),
            election_priority: 50, // デフォルトの優先度
            metadata: HashMap::new(),
            security_info: None,
            peer_stats: HashMap::new(),
        };
        
        // ノードレジストリを初期化
        let mut node_registry = HashMap::new();
        node_registry.insert(local_node_id.clone(), local_cluster_node);
        
        // トポロジーを初期化
        let topology = ClusterTopology {
            connections: HashMap::new(),
            partitions: Vec::new(),
            last_updated: Instant::now(),
            health_status: ClusterHealthStatus::Healthy,
        };
        
        Self {
            config,
            node_registry: Arc::new(RwLock::new(node_registry)),
            current_master: Arc::new(RwLock::new(None)),
            backup_masters: Arc::new(RwLock::new(Vec::new())),
            topology: Arc::new(RwLock::new(topology)),
            heartbeats: Arc::new(RwLock::new(HashMap::new())),
            local_node_id,
        }
    }
    
    /// クラスターを開始
    pub async fn start(&self) -> Result<()> {
        info!("クラスター {} を開始しています", self.config.cluster_id);
        
        // マスター選出を実行
        if self.config.enable_auto_election {
            self.start_master_election().await?;
        }
        
        // 定期的なメンバーシップ確認を開始
        self.start_membership_check()?;
        
        Ok(())
    }
    
    /// クラスターにノードを追加
    pub async fn join_node(&self, node: NodeInfo) -> Result<()> {
        debug!("ノード {} をクラスターに追加しています", node.id);
        
        let node_id = node.id.clone();
        
        // ノードをチェック
        if node_id == self.local_node_id {
            return Err(anyhow!("ローカルノードは追加できません"));
        }
        
        // クラスターノード情報を作成
        let cluster_node = ClusterNodeInfo {
            base_info: node,
            role: ClusterRole::Worker,
            join_time: Utc::now(),
            election_priority: 50,
            metadata: HashMap::new(),
            security_info: None,
            peer_stats: HashMap::new(),
        };
        
        // レジストリに追加
        {
            let mut registry = self.node_registry.write().await;
            if registry.contains_key(&node_id) {
                return Err(anyhow!("ノード {} は既にクラスターに存在します", node_id));
            }
            
            registry.insert(node_id.clone(), cluster_node);
        }
        
        // トポロジーに追加
        {
            let mut topology = self.topology.write().await;
            topology.connections.insert(node_id.clone(), HashSet::new());
            topology.last_updated = Instant::now();
        }
        
        // ハートビートを初期化
        {
            let mut heartbeats = self.heartbeats.write().await;
            heartbeats.insert(node_id.clone(), Instant::now());
        }
        
        info!("ノード {} がクラスターに参加しました", node_id);
        Ok(())
    }
    
    /// クラスターからノードを削除
    pub async fn remove_node(&self, node_id: &NodeId) -> Result<()> {
        debug!("ノード {} をクラスターから削除しています", node_id);
        
        // ローカルノードは削除できない
        if node_id == &self.local_node_id {
            return Err(anyhow!("ローカルノードはクラスターから削除できません"));
        }
        
        // ノードの存在を確認
        {
            let registry = self.node_registry.read().await;
            if !registry.contains_key(node_id) {
                return Err(anyhow!("ノード {} はクラスターに存在しません", node_id));
            }
        }
        
        // マスターノードの場合は処理を中止
        {
            let master = self.current_master.read().await;
            if let Some(master_id) = &*master {
                if master_id == node_id {
                    return Err(anyhow!("マスターノードは削除できません"));
                }
            }
        }
        
        // レジストリから削除
        {
            let mut registry = self.node_registry.write().await;
            registry.remove(node_id);
        }
        
        // トポロジーから削除
        {
            let mut topology = self.topology.write().await;
            topology.connections.remove(node_id);
            
            // 他のノードの接続リストからも削除
            for connections in topology.connections.values_mut() {
                connections.remove(node_id);
            }
            
            topology.last_updated = Instant::now();
        }
        
        // ハートビートから削除
        {
            let mut heartbeats = self.heartbeats.write().await;
            heartbeats.remove(node_id);
        }
        
        // バックアップマスターから削除
        {
            let mut backup_masters = self.backup_masters.write().await;
            backup_masters.retain(|id| id != node_id);
        }
        
        info!("ノード {} がクラスターから削除されました", node_id);
        Ok(())
    }
    
    /// マスターノードを設定
    pub async fn set_master_node(&self, node_id: NodeId) -> Result<()> {
        debug!("ノード {} をマスターとして設定します", node_id);
        
        // ノードの存在を確認
        {
            let registry = self.node_registry.read().await;
            if !registry.contains_key(&node_id) {
                return Err(anyhow!("ノード {} はクラスターに存在しません", node_id));
            }
        }
        
        // 旧マスターをバックアップに降格
        {
            let old_master = {
                let mut master = self.current_master.write().await;
                let old = master.take();
                *master = Some(node_id.clone());
                old
            };
            
            if let Some(old_id) = old_master {
                if old_id != node_id {
                    // 古いマスターをバックアップに降格
                    let mut registry = self.node_registry.write().await;
                    if let Some(node) = registry.get_mut(&old_id) {
                        node.role = ClusterRole::BackupMaster;
                    }
                    
                    // バックアップリストに追加
                    let mut backup_masters = self.backup_masters.write().await;
                    if !backup_masters.contains(&old_id) {
                        backup_masters.push(old_id);
                    }
                }
            }
        }
        
        // 新マスターの役割を更新
        {
            let mut registry = self.node_registry.write().await;
            if let Some(node) = registry.get_mut(&node_id) {
                node.role = ClusterRole::Master;
            }
        }
        
        info!("ノード {} がマスターに昇格しました", node_id);
        Ok(())
    }
    
    /// マスターノードIDを取得
    pub async fn get_master_node(&self) -> Option<NodeId> {
        let master = self.current_master.read().await;
        master.clone()
    }
    
    /// ノード情報を取得
    pub async fn get_node_info(&self, node_id: &NodeId) -> Option<ClusterNodeInfo> {
        let registry = self.node_registry.read().await;
        registry.get(node_id).cloned()
    }
    
    /// マスター選出を開始
    async fn start_master_election(&self) -> Result<()> {
        debug!("マスター選出を開始します");
        
        let registry = self.node_registry.read().await;
        
        // 選出条件に基づいてノードをソート
        let mut nodes: Vec<(&NodeId, &ClusterNodeInfo)> = registry.iter().collect();
        nodes.sort_by(|(_, a), (_, b)| b.election_priority.cmp(&a.election_priority));
        
        if let Some((node_id, _)) = nodes.first() {
            info!("ノード {} がマスターとして選出されました", node_id);
            self.set_master_node((*node_id).clone()).await?;
        } else {
            warn!("マスター選出失敗: 有効なノードがありません");
        }
        
        Ok(())
    }
    
    /// 定期的なメンバーシップ確認を開始
    fn start_membership_check(&self) -> Result<()> {
        info!("メンバーシップ確認を開始します");
        
        let node_registry = self.node_registry.clone();
        let heartbeats = self.heartbeats.clone();
        let failure_timeout = self.config.failure_detection_timeout;
        let membership_interval = self.config.membership_interval;
        
        // 定期的なメンバーシップ確認タスクを起動
        tokio::spawn(async move {
            loop {
                // タイムアウトしたノードを検出
                let timed_out_nodes = {
                    let now = Instant::now();
                    let beats = heartbeats.read().await;
                    let registry = node_registry.read().await;
                    
                    beats.iter()
                        .filter(|(node_id, last_time)| {
                            // ローカルノードを除外
                            **node_id != self.local_node_id && 
                            // タイムアウト確認
                            now.duration_since(**last_time) > failure_timeout
                        })
                        .map(|(node_id, _)| node_id.clone())
                        .collect::<Vec<_>>()
                };
                
                // タイムアウトしたノードを処理
                for node_id in timed_out_nodes {
                    // ノードのステータスを更新
                    let node_registry_clone = node_registry.clone();
                    let mut registry = node_registry_clone.write().await;
                    if let Some(node) = registry.get_mut(&node_id) {
                        node.base_info.update_status(NodeStatus::Offline);
                        warn!("ノード {} はタイムアウトしました", node_id);
                    }
                }
                
                // 次の確認まで待機
                tokio::time::sleep(membership_interval).await;
            }
        });
        
        Ok(())
    }
    
    /// ハートビートを送信/更新
    pub async fn update_heartbeat(&self, node_id: &NodeId) -> Result<()> {
        // ノードの存在を確認
        {
            let registry = self.node_registry.read().await;
            if !registry.contains_key(node_id) {
                return Err(anyhow!("ノード {} はクラスターに存在しません", node_id));
            }
        }
        
        // ハートビートを更新
        {
            let mut heartbeats = self.heartbeats.write().await;
            heartbeats.insert(node_id.clone(), Instant::now());
        }
        
        // ノードがオフラインだった場合、オンラインに戻す
        {
            let mut registry = self.node_registry.write().await;
            if let Some(node) = registry.get_mut(node_id) {
                if node.base_info.status == NodeStatus::Offline {
                    node.base_info.update_status(NodeStatus::Online);
                    info!("ノード {} がオンラインに戻りました", node_id);
                }
            }
        }
        
        Ok(())
    }
    
    /// クラスター健全性状態を取得
    pub async fn check_cluster_health(&self) -> ClusterHealthStatus {
        // 基本的な健全性確認
        let registry = self.node_registry.read().await;
        let total_nodes = registry.len();
        let online_nodes = registry.values()
            .filter(|node| node.base_info.is_available())
            .count();
        
        // マスターの存在確認
        let has_master = {
            let master = self.current_master.read().await;
            master.is_some()
        };
        
        // クォーラムの確認
        let has_quorum = online_nodes >= self.config.min_quorum_size;
        
        // 決定ロジック
        let status = if !has_master {
            ClusterHealthStatus::Critical
        } else if !has_quorum {
            ClusterHealthStatus::Critical
        } else if online_nodes < total_nodes / 2 {
            ClusterHealthStatus::Warning
        } else {
            // トポロジーを確認
            let topology = self.topology.read().await;
            if !topology.partitions.is_empty() {
                ClusterHealthStatus::Partitioned
            } else {
                ClusterHealthStatus::Healthy
            }
        };
        
        // トポロジーの健全性状態を更新
        {
            let mut topology = self.topology.write().await;
            topology.health_status = status;
        }
        
        status
    }
    
    /// ノード間の接続を更新
    pub async fn update_connection(&self, from: &NodeId, to: &NodeId, connected: bool) -> Result<()> {
        // ノードの存在を確認
        {
            let registry = self.node_registry.read().await;
            if !registry.contains_key(from) || !registry.contains_key(to) {
                return Err(anyhow!("ノードが存在しません"));
            }
        }
        
        // 接続を更新
        {
            let mut topology = self.topology.write().await;
            let connections = topology.connections.entry(from.clone()).or_insert_with(HashSet::new);
            
            if connected {
                connections.insert(to.clone());
            } else {
                connections.remove(to);
            }
            
            topology.last_updated = Instant::now();
        }
        
        Ok(())
    }
    
    /// すべてのノード情報を取得
    pub async fn get_all_nodes(&self) -> Vec<ClusterNodeInfo> {
        let registry = self.node_registry.read().await;
        registry.values().cloned().collect()
    }
    
    /// オンラインノード情報を取得
    pub async fn get_online_nodes(&self) -> Vec<ClusterNodeInfo> {
        let registry = self.node_registry.read().await;
        registry.values()
            .filter(|node| node.base_info.is_available())
            .cloned()
            .collect()
    }
    
    /// バックアップマスターノードを取得
    pub async fn get_backup_masters(&self) -> Vec<NodeId> {
        let backup_masters = self.backup_masters.read().await;
        backup_masters.clone()
    }
    
    /// ローカルノードIDを取得
    pub fn get_local_node_id(&self) -> &NodeId {
        &self.local_node_id
    }
    
    /// ローカルノードがマスターかどうか
    pub async fn is_local_node_master(&self) -> bool {
        let master = self.current_master.read().await;
        match &*master {
            Some(id) => id == &self.local_node_id,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cluster_config_default() {
        let config = ClusterConfig::default();
        assert_eq!(config.min_quorum_size, 1);
        assert_eq!(config.enable_auto_election, true);
        assert_eq!(config.enable_encryption, true);
    }
    
    #[tokio::test]
    async fn test_cluster_manager_basic() {
        // ローカルノード作成
        let local_node = NodeInfo::new(
            "local-node".to_string(),
            "localhost".to_string(),
            8080
        );
        let local_id = local_node.id.clone();
        
        // クラスターマネージャー作成
        let manager = ClusterManager::new(ClusterConfig::default(), local_node);
        
        // マスター選出
        manager.start_master_election().await.unwrap();
        
        // マスターノード確認
        let master = manager.get_master_node().await.unwrap();
        assert_eq!(master, local_id);
        
        // ノード追加
        let node2 = NodeInfo::new(
            "node2".to_string(),
            "localhost".to_string(),
            8081
        );
        let node2_id = node2.id.clone();
        
        manager.join_node(node2).await.unwrap();
        
        // 全ノード取得
        let all_nodes = manager.get_all_nodes().await;
        assert_eq!(all_nodes.len(), 2);
        
        // ノード削除
        manager.remove_node(&node2_id).await.unwrap();
        
        // 全ノード再確認
        let all_nodes = manager.get_all_nodes().await;
        assert_eq!(all_nodes.len(), 1);
    }
} 