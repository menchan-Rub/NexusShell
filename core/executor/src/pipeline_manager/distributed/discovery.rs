/**
 * 分散サービス検出モジュール
 * 
 * 分散パイプライン実行ノードの検出と管理を担当するモジュール
 */

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn, error};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::time::sleep;
use mdns_sd::{ServiceDaemon, ServiceInfo, ServiceEvent};
use uuid::Uuid;
use hostname;

use super::node::{NodeId, NodeInfo, NodeStatus};

/// サービス検出方法
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    /// 静的ノードリスト
    StaticList,
    /// マルチキャストDNS
    MulticastDns,
    /// 中央レジストリ
    CentralRegistry,
    /// Kubernetes
    Kubernetes,
    /// コンスル
    Consul,
    /// ZooKeeper
    ZooKeeper,
    /// カスタム
    Custom(String),
}

/// ディスカバリーサービスの特性
#[async_trait]
pub trait DiscoveryService: Send + Sync {
    /// ノードを登録
    async fn register_node(&self, node: &NodeInfo) -> Result<()>;
    
    /// ノードの登録を解除
    async fn unregister_node(&self, node_id: &NodeId) -> Result<()>;
    
    /// ノードを検出
    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>>;
    
    /// ノードの状態を更新
    async fn update_node_status(&self, node_id: &NodeId, status: NodeStatus) -> Result<()>;
}

/// マルチキャストDNSディスカバリー
pub struct MulticastDnsDiscovery {
    /// クラスタ名
    cluster_name: String,
    /// サービスポート
    port: u16,
    /// 内部状態
    state: Arc<Mutex<MulticastDnsState>>,
}

struct MulticastDnsState {
    /// 登録済みノード
    registered_nodes: HashMap<NodeId, NodeInfo>,
    /// サービス名
    service_name: String,
}

impl MulticastDnsDiscovery {
    /// 新しいマルチキャストDNSディスカバリーを作成
    pub fn new(cluster_name: String, port: u16) -> Self {
        let service_name = format!("{}.local.", cluster_name);
        
        let state = MulticastDnsState {
            registered_nodes: HashMap::new(),
            service_name: service_name.clone(),
        };
        
        Self {
            cluster_name,
            port,
            state: Arc::new(Mutex::new(state)),
        }
    }
    
    /// サービス検出を開始
    pub async fn start(&self) -> Result<()> {
        info!("マルチキャストDNSディスカバリーを開始: {}", self.cluster_name);
        
        // mDNSサービスデーモンを作成
        let mdns = ServiceDaemon::new()?;
        
        // このノードのサービス情報を作成
        let host_name = hostname::get()?
            .to_string_lossy()
            .to_string();
        
        let service_type = format!("_{}._tcp.local.", self.cluster_name);
        let instance_name = format!("{}_{}", host_name, Uuid::new_v4().to_simple());
        
        let mut service_info = ServiceInfo::new(
            &service_type,
            &instance_name,
            &host_name,
            None, // ローカルIPアドレスは自動検出
            self.port,
            None, // TXTレコードなし
        )?;
        
        // メタデータを追加
        let mut txt_properties = HashMap::new();
        txt_properties.insert("cluster".to_string(), self.cluster_name.clone());
        service_info.set_properties(txt_properties)?;
        
        // サービスを登録
        mdns.register(service_info)?;
        
        // サービスブラウジングを開始
        let browse_type = format!("_{}._tcp.local.", self.cluster_name);
        let receiver = mdns.browse(&browse_type)?;
        
        // バックグラウンドでサービスイベントを監視
        let state = self.state.clone();
        tokio::spawn(async move {
            debug!("mDNSブラウジングを開始: {}", browse_type);
            
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceFound(service_type, fullname) => {
                        debug!("サービスを発見: {} ({})", fullname, service_type);
                    }
                    ServiceEvent::ServiceResolved(info) => {
                        debug!("サービスを解決: {}", info.get_fullname());
                        
                        // ノード情報を作成
                        let properties = info.get_properties();
                        if let Some(cluster) = properties.get("cluster") {
                            if cluster == &self.cluster_name {
                                let address = format!("{}:{}", info.get_addresses()[0], info.get_port());
                                let node_id = NodeId::from_string(info.get_fullname().to_string());
                                let node_info = NodeInfo::new(
                                    node_id.clone(),
                                    info.get_hostname().to_string(),
                                    address,
                                    NodeStatus::Available,
                                );
                                
                                // ノードを登録
                                let mut state_guard = state.lock().await;
                                state_guard.registered_nodes.insert(node_id, node_info);
                            }
                        }
                    }
                    ServiceEvent::ServiceRemoved(service_type, fullname) => {
                        debug!("サービスが削除されました: {} ({})", fullname, service_type);
                        
                        // ノードを削除
                        let node_id = NodeId::from_string(fullname);
                        let mut state_guard = state.lock().await;
                        state_guard.registered_nodes.remove(&node_id);
                    }
                    _ => {}
                }
            }
        });
        
        Ok(())
    }
    
    /// サービス検出を停止
    pub async fn stop(&self) -> Result<()> {
        info!("マルチキャストDNSディスカバリーを停止: {}", self.cluster_name);
        
        // mDNSサービスデーモンを作成
        let mdns = ServiceDaemon::new()?;
        
        // このノードのサービス情報を作成
        let host_name = hostname::get()?
            .to_string_lossy()
            .to_string();
        
        let service_type = format!("_{}._tcp.local.", self.cluster_name);
        let instance_name = format!("{}_{}", host_name, Uuid::new_v4().to_simple());
        
        // サービスを登録解除
        mdns.unregister(&service_type, &instance_name)?;
        
        Ok(())
    }
}

#[async_trait]
impl DiscoveryService for MulticastDnsDiscovery {
    async fn register_node(&self, node: &NodeInfo) -> Result<()> {
        let mut state = self.state.lock().await;
        
        // すでに登録されているか確認
        if state.registered_nodes.contains_key(&node.id) {
            debug!("ノード {} は既に登録されています", node.id);
            state.registered_nodes.insert(node.id.clone(), node.clone());
            return Ok(());
        }
        
        // ノードを登録
        info!("ノード {} を登録しています", node.id);
        state.registered_nodes.insert(node.id.clone(), node.clone());
        
        // mDNSサービスデーモンを作成
        let mdns = ServiceDaemon::new()?;
        
        // ノードのサービス情報を作成
        let service_type = format!("_{}._tcp.local.", self.cluster_name);
        
        let mut service_info = ServiceInfo::new(
            &service_type,
            &node.id.to_string(),
            &node.name,
            node.address.split(':').next(),
            node.address.split(':').nth(1).unwrap_or("0").parse()?,
            None, // TXTレコードなし
        )?;
        
        // メタデータを追加
        let mut txt_properties = HashMap::new();
        txt_properties.insert("cluster".to_string(), self.cluster_name.clone());
        txt_properties.insert("node_id".to_string(), node.id.to_string());
        txt_properties.insert("status".to_string(), format!("{:?}", node.status));
        service_info.set_properties(txt_properties)?;
        
        // サービスを登録
        mdns.register(service_info)?;
        
        Ok(())
    }
    
    async fn unregister_node(&self, node_id: &NodeId) -> Result<()> {
        let mut state = self.state.lock().await;
        
        // 登録されているか確認
        if !state.registered_nodes.contains_key(node_id) {
            return Err(anyhow!("ノード {} は登録されていません", node_id));
        }
        
        // ノードの登録を解除
        info!("ノード {} の登録を解除しています", node_id);
        let node = state.registered_nodes.remove(node_id).unwrap();
        
        // mDNSサービスデーモンを作成
        let mdns = ServiceDaemon::new()?;
        
        // サービスを登録解除
        let service_type = format!("_{}._tcp.local.", self.cluster_name);
        mdns.unregister(&service_type, &node_id.to_string())?;
        
        Ok(())
    }
    
    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        let state = self.state.lock().await;
        let nodes: Vec<NodeInfo> = state.registered_nodes.values().cloned().collect();
        
        debug!("発見されたノード数: {}", nodes.len());
        Ok(nodes)
    }
    
    async fn update_node_status(&self, node_id: &NodeId, status: NodeStatus) -> Result<()> {
        let mut state = self.state.lock().await;
        
        // 登録されているか確認
        if let Some(node) = state.registered_nodes.get_mut(node_id) {
            debug!("ノード {} の状態を更新: {:?}", node_id, status);
            node.update_status(status);
            Ok(())
        } else {
            Err(anyhow!("ノード {} は登録されていません", node_id))
        }
    }
}

/// 静的ノードリストディスカバリー
pub struct StaticListDiscovery {
    /// 内部状態
    state: Arc<RwLock<HashMap<NodeId, NodeInfo>>>,
}

impl StaticListDiscovery {
    /// 新しい静的ノードリストディスカバリーを作成
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 初期ノードを追加
    pub async fn add_initial_nodes(&self, nodes: Vec<NodeInfo>) -> Result<()> {
        let mut state = self.state.write().await;
        
        for node in nodes {
            info!("初期ノード {} を追加: {}", node.name, node.address());
            state.insert(node.id.clone(), node);
        }
        
        Ok(())
    }
}

#[async_trait]
impl DiscoveryService for StaticListDiscovery {
    async fn register_node(&self, node: &NodeInfo) -> Result<()> {
        let mut state = self.state.write().await;
        state.insert(node.id.clone(), node.clone());
        debug!("ノード {} を静的リストに登録しました", node.id);
        Ok(())
    }
    
    async fn unregister_node(&self, node_id: &NodeId) -> Result<()> {
        let mut state = self.state.write().await;
        
        if state.remove(node_id).is_some() {
            debug!("ノード {} を静的リストから削除しました", node_id);
            Ok(())
        } else {
            Err(anyhow!("ノード {} は登録されていません", node_id))
        }
    }
    
    async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        let state = self.state.read().await;
        Ok(state.values().cloned().collect())
    }
    
    async fn update_node_status(&self, node_id: &NodeId, status: NodeStatus) -> Result<()> {
        let mut state = self.state.write().await;
        
        if let Some(node) = state.get_mut(node_id) {
            node.update_status(status);
            debug!("ノード {} の状態を更新しました: {:?}", node_id, status);
            Ok(())
        } else {
            Err(anyhow!("ノード {} は登録されていません", node_id))
        }
    }
}

/// ディスカバリーマネージャー
pub struct DiscoveryManager {
    /// 現在のディスカバリーサービス
    service: Arc<dyn DiscoveryService>,
    /// 最終検出時刻
    last_discovery: RwLock<Instant>,
    /// 検出間隔
    discovery_interval: Duration,
    /// 自動検出を有効にするか
    auto_discovery: bool,
}

impl DiscoveryManager {
    /// 新しいディスカバリーマネージャーを作成
    pub fn new(service: Arc<dyn DiscoveryService>, discovery_interval: Duration) -> Self {
        Self {
            service,
            last_discovery: RwLock::new(Instant::now()),
            discovery_interval,
            auto_discovery: true,
        }
    }
    
    /// 自動検出の有効/無効を設定
    pub fn set_auto_discovery(&mut self, enabled: bool) {
        self.auto_discovery = enabled;
    }
    
    /// ノードを検出
    pub async fn discover_nodes(&self) -> Result<Vec<NodeInfo>> {
        // 最終検出時刻を更新
        {
            let mut last_discovery = self.last_discovery.write().await;
            *last_discovery = Instant::now();
        }
        
        // ノードを検出
        self.service.discover_nodes().await
    }
    
    /// 自動検出ループを開始
    pub async fn start_auto_discovery(&self) -> Result<()> {
        if !self.auto_discovery {
            return Ok(());
        }
        
        info!("自動ノード検出を開始します (間隔: {:?})", self.discovery_interval);
        
        let service = self.service.clone();
        let discovery_interval = self.discovery_interval;
        
        tokio::spawn(async move {
            loop {
                match service.discover_nodes().await {
                    Ok(nodes) => {
                        debug!("自動ノード検出: {} ノードを発見しました", nodes.len());
                    },
                    Err(e) => {
                        error!("自動ノード検出中にエラーが発生しました: {}", e);
                    }
                }
                
                // 次の検出まで待機
                tokio::time::sleep(discovery_interval).await;
            }
        });
        
        Ok(())
    }
    
    /// ノードを登録
    pub async fn register_node(&self, node: &NodeInfo) -> Result<()> {
        self.service.register_node(node).await
    }
    
    /// ノードの登録を解除
    pub async fn unregister_node(&self, node_id: &NodeId) -> Result<()> {
        self.service.unregister_node(node_id).await
    }
    
    /// ノードの状態を更新
    pub async fn update_node_status(&self, node_id: &NodeId, status: NodeStatus) -> Result<()> {
        self.service.update_node_status(node_id, status).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_static_list_discovery() {
        let discovery = StaticListDiscovery::new();
        
        // テストノード作成
        let node1 = NodeInfo::new(
            "test-node-1".to_string(),
            "localhost".to_string(),
            8080
        );
        
        let node2 = NodeInfo::new(
            "test-node-2".to_string(),
            "localhost".to_string(),
            8081
        );
        
        // ノード登録
        discovery.register_node(&node1).await.unwrap();
        discovery.register_node(&node2).await.unwrap();
        
        // ノード検出
        let nodes = discovery.discover_nodes().await.unwrap();
        assert_eq!(nodes.len(), 2);
        
        // ノード状態更新
        let node1_id = node1.id.clone();
        discovery.update_node_status(&node1_id, NodeStatus::Busy).await.unwrap();
        
        // ノード検出 (状態更新確認)
        let nodes = discovery.discover_nodes().await.unwrap();
        let updated_node = nodes.iter().find(|n| n.id == node1_id).unwrap();
        assert_eq!(updated_node.status, NodeStatus::Busy);
        
        // ノード登録解除
        discovery.unregister_node(&node1_id).await.unwrap();
        
        // ノード検出 (削除確認)
        let nodes = discovery.discover_nodes().await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(nodes.iter().all(|n| n.id != node1_id));
    }
}
