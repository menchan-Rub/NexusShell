use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::time;
use super::node::{Node, NodeState};

/// デフォルトのハートビートタイムアウト (秒)
const DEFAULT_HEARTBEAT_TIMEOUT_SEC: u64 = 30;

/// ノードのハートビートを監視する
pub struct HeartbeatMonitor {
    /// ノード状態の共有参照
    nodes: Arc<RwLock<HashMap<String, Node>>>,
    /// タイムアウト期間
    timeout: Duration,
    /// 最後のチェック時刻
    last_check: Instant,
}

impl HeartbeatMonitor {
    /// 新しいハートビートモニターを作成
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            timeout: Duration::from_secs(DEFAULT_HEARTBEAT_TIMEOUT_SEC),
            last_check: Instant::now(),
        }
    }
    
    /// ノード参照を設定
    pub fn set_nodes(&mut self, nodes: Arc<RwLock<HashMap<String, Node>>>) {
        self.nodes = nodes;
    }
    
    /// タイムアウト期間を設定
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
    
    /// ハートビートチェックを実行
    pub fn check_heartbeats(&mut self) -> Vec<String> {
        let mut failed_nodes = Vec::new();
        let now = Instant::now();
        
        // 前回のチェックから一定時間経過していない場合はスキップ
        if now.duration_since(self.last_check) < Duration::from_secs(5) {
            return failed_nodes;
        }
        
        self.last_check = now;
        
        // ノードの状態を読み取る
        let nodes_guard = match self.nodes.read() {
            Ok(guard) => guard,
            Err(_) => return failed_nodes, // ロック取得失敗
        };
        
        // タイムアウトしたノードを特定
        for (node_id, node) in nodes_guard.iter() {
            if node.state == NodeState::Available || node.state == NodeState::Busy {
                if now.duration_since(node.last_heartbeat) > self.timeout {
                    failed_nodes.push(node_id.clone());
                }
            }
        }
        
        // ロックを解放
        drop(nodes_guard);
        
        // 失敗したノードの状態を更新
        if !failed_nodes.is_empty() {
            if let Ok(mut nodes) = self.nodes.write() {
                for node_id in &failed_nodes {
                    if let Some(node) = nodes.get_mut(node_id) {
                        node.set_state(NodeState::Offline);
                    }
                }
            }
        }
        
        failed_nodes
    }
    
    /// 非同期ハートビート監視タスクを開始
    pub async fn start_monitoring(
        mut self,
        mut failure_callback: impl FnMut(Vec<String>) + Send + 'static,
    ) {
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                let failed_nodes = self.check_heartbeats();
                
                if !failed_nodes.is_empty() {
                    failure_callback(failed_nodes);
                }
            }
        });
    }
    
    /// ノードからのハートビートを処理
    pub fn process_heartbeat(&self, node_id: &str) -> bool {
        let mut result = false;
        
        if let Ok(mut nodes) = self.nodes.write() {
            if let Some(node) = nodes.get_mut(node_id) {
                node.update_heartbeat();
                
                // オフラインノードが復帰した場合は状態を更新
                if node.state == NodeState::Offline {
                    node.set_state(NodeState::Available);
                }
                
                result = true;
            }
        }
        
        result
    }
} 