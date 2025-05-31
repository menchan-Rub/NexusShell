/**
 * 分散通信モジュール
 * 
 * 分散パイプライン実行におけるノード間通信を担当するモジュール
 */

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use super::node::{NodeId, NodeInfo, NodeStatus};
use super::task::DistributedTask;

/// メッセージID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(String);

impl MessageId {
    /// 新しいメッセージIDを生成
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    /// 文字列からメッセージIDを作成
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// メッセージタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// ハートビート
    Heartbeat,
    /// タスク割り当て
    TaskAssignment,
    /// タスク状態更新
    TaskStatusUpdate,
    /// タスク結果
    TaskResult,
    /// ノード情報
    NodeInfo,
    /// ノード参加要求
    JoinRequest,
    /// ノード参加応答
    JoinResponse,
    /// マスター選出
    MasterElection,
    /// データ転送
    DataTransfer,
    /// クエリ
    Query,
    /// コマンド
    Command,
    /// エラー
    Error,
}

/// 分散メッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedMessage {
    /// メッセージID
    pub id: String,
    /// 送信元ノード
    pub sender: String,
    /// 宛先ノード
    pub recipient: String,
    /// メッセージタイプ
    pub message_type: MessageType,
    /// タイムスタンプ（ミリ秒）
    pub timestamp: u64,
    /// ペイロード
    pub payload: Vec<u8>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl DistributedMessage {
    /// 新しいメッセージを作成
    pub fn new(
        sender: String,
        recipient: String,
        message_type: MessageType,
        payload: Vec<u8>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
            
        Self {
            id: MessageId::new().to_string(),
            sender,
            recipient,
            message_type,
            timestamp: now,
            payload,
            metadata: HashMap::new(),
        }
    }
    
    /// メタデータを追加
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
    
    /// ハートビートメッセージを作成
    pub fn create_heartbeat(sender: String, recipient: String) -> Self {
        Self::new(sender, recipient, MessageType::Heartbeat, Vec::new())
    }
    
    /// タスク割り当てメッセージを作成
    pub fn create_task_assignment(sender: String, recipient: String, task: &DistributedTask) -> Result<Self> {
        let payload = serde_json::to_vec(task)
            .context("タスクのシリアル化に失敗")?;
            
        Ok(Self::new(sender, recipient, MessageType::TaskAssignment, payload))
    }
    
    /// エラーメッセージを作成
    pub fn create_error(sender: String, recipient: String, error_message: &str) -> Self {
        Self::new(
            sender,
            recipient,
            MessageType::Error,
            error_message.as_bytes().to_vec(),
        )
    }
}

/// 通信トランスポート
#[async_trait]
pub trait CommunicationTransport: Send + Sync {
    /// メッセージを送信
    async fn send_message(&self, message: DistributedMessage) -> Result<()>;
    
    /// メッセージを受信
    async fn receive_message(&self) -> Result<DistributedMessage>;
    
    /// 特定のメッセージタイプを受信
    async fn receive_message_of_type(&self, message_type: MessageType) -> Result<DistributedMessage>;
    
    /// 通信を開始
    async fn start(&self) -> Result<()>;
    
    /// 通信を停止
    async fn stop(&self) -> Result<()>;
}

/// TCP通信トランスポート
pub struct TcpTransport {
    /// ローカルノードID
    local_node_id: NodeId,
    /// ローカルノードの接続情報
    bind_address: String,
    /// 受信チャネル
    rx_queue: mpsc::Receiver<DistributedMessage>,
    /// 送信チャネル
    tx_queue: mpsc::Sender<DistributedMessage>,
    /// ノード接続キャッシュ
    connections: Arc<RwLock<HashMap<NodeId, NodeConnection>>>,
    /// 実行状態
    running: Arc<RwLock<bool>>,
}

/// ノード接続情報
struct NodeConnection {
    /// 接続先アドレス
    address: String,
    /// 最終通信時間
    last_activity: Instant,
    /// 送信チャネル
    tx: mpsc::Sender<DistributedMessage>,
}

impl TcpTransport {
    /// 新しいTCP通信トランスポートを作成
    pub fn new(local_node_id: NodeId, bind_address: String) -> Self {
        let (tx_queue, rx_queue) = mpsc::channel(100);
        
        Self {
            local_node_id,
            bind_address,
            rx_queue,
            tx_queue,
            connections: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// TCPサーバーを開始
    async fn start_server(&self) -> Result<()> {
        // ローカルアドレスをバインド
        let addr = self.bind_address.parse()
            .context("アドレスのパースに失敗")?;
            
        let listener = tokio::net::TcpListener::bind(addr).await
            .context("TCPリスナーのバインドに失敗")?;
            
        info!("TCPトランスポートがアドレス {}でリッスン開始", self.bind_address);
        
        let tx_queue = self.tx_queue.clone();
        let running = self.running.clone();
        
        // クライアント接続を処理するタスクを起動
        tokio::spawn(async move {
            while let Ok(true) = {
                let guard = running.read().await;
                Ok::<_, anyhow::Error>(*guard)
            } {
                match listener.accept().await {
                    Ok((socket, addr)) => {
                        debug!("新しいクライアント接続: {}", addr);
                        
                        let tx_queue = tx_queue.clone();
                        
                        // 各クライアント接続を処理するタスクを起動
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_client(socket, tx_queue).await {
                                error!("クライアント処理中にエラーが発生: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("クライアント接続の受け入れに失敗: {}", e);
                    }
                }
            }
            
            debug!("TCPサーバーが停止しました");
        });
        
        Ok(())
    }
    
    /// クライアント接続を処理
    async fn handle_client(socket: tokio::net::TcpStream, tx_queue: mpsc::Sender<DistributedMessage>) -> Result<()> {
        let (reader, _writer) = socket.into_split();
        let mut buf_reader = tokio::io::BufReader::new(reader);
        
        // JSONメッセージを行単位で読み込み
        let mut line = String::new();
        while let Ok(n) = buf_reader.read_line(&mut line).await {
            if n == 0 {
                break; // 接続終了
            }
            
            // 受信したJSONをパース
            match serde_json::from_str::<DistributedMessage>(&line) {
                Ok(message) => {
                    // メッセージキューに送信
                    if let Err(e) = tx_queue.send(message).await {
                        error!("メッセージキューへの送信に失敗: {}", e);
                    }
                }
                Err(e) => {
                    error!("JSONのパースに失敗: {}, 受信データ: {}", e, line);
                }
            }
            
            line.clear();
        }
        
        Ok(())
    }
    
    /// ノードに接続
    async fn connect_to_node(&self, node_id: &NodeId, address: &str) -> Result<()> {
        debug!("ノード {} (アドレス: {}) に接続中", node_id, address);
        
        let socket = tokio::net::TcpStream::connect(address).await
            .context(format!("ノード {} への接続に失敗", node_id))?;
            
        // ノードごとの送信キューを作成
        let (tx, mut rx) = mpsc::channel::<DistributedMessage>(100);
        
        // 接続情報を保存
        {
            let mut connections = self.connections.write().await;
            connections.insert(node_id.clone(), NodeConnection {
                address: address.to_string(),
                last_activity: Instant::now(),
                tx: tx.clone(),
            });
        }
        
        let writer = socket.try_clone().await?;
        
        // 受信タスクを起動
        let tx_queue = self.tx_queue.clone();
        let reader = socket;
        tokio::spawn(async move {
            if let Err(e) = Self::handle_client(reader, tx_queue).await {
                error!("ノード接続の読み取り中にエラー: {}", e);
            }
        });
        
        // 送信タスクを起動
        tokio::spawn(async move {
            let mut writer = tokio::io::BufWriter::new(writer);
            
            while let Some(message) = rx.recv().await {
                // メッセージをJSON形式で送信
                match serde_json::to_string(&message) {
                    Ok(json) => {
                        if let Err(e) = writeln!(writer, "{}", json).await {
                            error!("メッセージ送信中にエラー: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("メッセージのJSONシリアル化に失敗: {}", e);
                    }
                }
                
                if let Err(e) = writer.flush().await {
                    error!("ライターのフラッシュに失敗: {}", e);
                    break;
                }
            }
            
            debug!("ノード送信タスクが終了しました");
        });
        
        Ok(())
    }
}

#[async_trait]
impl CommunicationTransport for TcpTransport {
    async fn send_message(&self, message: DistributedMessage) -> Result<()> {
        let recipient = NodeId::from_string(message.recipient.clone());
        
        // ノードへの接続を取得または確立
        let tx = {
            let connections = self.connections.read().await;
            
            if let Some(connection) = connections.get(&recipient) {
                connection.tx.clone()
            } else {
                // 接続がない場合はエラー
                return Err(anyhow!("ノード {} への接続が確立されていません", recipient));
            }
        };
        
        // メッセージを送信
        tx.send(message).await
            .map_err(|e| anyhow!("メッセージの送信に失敗: {}", e))?;
            
        Ok(())
    }
    
    async fn receive_message(&self) -> Result<DistributedMessage> {
        self.rx_queue.recv().await
            .ok_or_else(|| anyhow!("受信チャネルが閉じられました"))
    }
    
    async fn receive_message_of_type(&self, message_type: MessageType) -> Result<DistributedMessage> {
        let mut rx = self.rx_queue.clone();
        
        while let Some(message) = rx.recv().await {
            if message.message_type == message_type {
                return Ok(message);
            }
        }
        
        Err(anyhow!("受信チャネルが閉じられました"))
    }
    
    async fn start(&self) -> Result<()> {
        // 実行状態を設定
        {
            let mut running = self.running.write().await;
            *running = true;
        }
        
        // TCPサーバーを開始
        self.start_server().await?;
        
        info!("TCP通信トランスポートを開始しました");
        Ok(())
    }
    
    async fn stop(&self) -> Result<()> {
        // 実行状態を更新
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        
        info!("TCP通信トランスポートを停止しました");
        Ok(())
    }
}

/// 通信マネージャー
pub struct CommunicationManager {
    /// ローカルノードID
    local_node_id: NodeId,
    /// トランスポート
    transport: Arc<dyn CommunicationTransport>,
    /// メッセージハンドラー
    message_handlers: Arc<RwLock<HashMap<MessageType, Vec<Arc<dyn MessageHandler>>>>>,
    /// 実行状態
    running: Arc<RwLock<bool>>,
}

/// メッセージハンドラー
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// メッセージを処理
    async fn handle_message(&self, message: &DistributedMessage) -> Result<()>;
    
    /// 処理対象のメッセージタイプを取得
    fn message_types(&self) -> Vec<MessageType>;
}

impl CommunicationManager {
    /// 新しい通信マネージャーを作成
    pub fn new(local_node_id: NodeId, transport: Arc<dyn CommunicationTransport>) -> Self {
        Self {
            local_node_id,
            transport,
            message_handlers: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// メッセージハンドラーを登録
    pub async fn register_handler(&self, handler: Arc<dyn MessageHandler>) -> Result<()> {
        let mut handlers = self.message_handlers.write().await;
        
        for message_type in handler.message_types() {
            let type_handlers = handlers
                .entry(message_type)
                .or_insert_with(Vec::new);
                
            type_handlers.push(handler.clone());
        }
        
        Ok(())
    }
    
    /// 通信を開始
    pub async fn start(&self) -> Result<()> {
        // トランスポートを開始
        self.transport.start().await?;
        
        // 実行状態を設定
        {
            let mut running = self.running.write().await;
            *running = true;
        }
        
        // メッセージ処理ループを開始
        let transport = self.transport.clone();
        let message_handlers = self.message_handlers.clone();
        let running = self.running.clone();
        
        tokio::spawn(async move {
            while let Ok(true) = {
                let guard = running.read().await;
                Ok::<_, anyhow::Error>(*guard)
            } {
                match transport.receive_message().await {
                    Ok(message) => {
                        debug!("メッセージを受信: ID={}, タイプ={:?}, 送信元={}",
                               message.id, message.message_type, message.sender);
                               
                        // メッセージタイプに対応するハンドラーを取得
                        let handlers = {
                            let handlers_map = message_handlers.read().await;
                            handlers_map.get(&message.message_type)
                                .cloned()
                                .unwrap_or_default()
                        };
                        
                        // すべてのハンドラーでメッセージを処理
                        for handler in handlers {
                            if let Err(e) = handler.handle_message(&message).await {
                                error!("メッセージ処理中にエラー: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("メッセージ受信中にエラー: {}", e);
                        
                        // 一時的なエラーの場合は少し待機
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
            
            debug!("メッセージ処理ループが終了しました");
        });
        
        info!("通信マネージャーを開始しました");
        Ok(())
    }
    
    /// 通信を停止
    pub async fn stop(&self) -> Result<()> {
        // 実行状態を更新
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        
        // トランスポートを停止
        self.transport.stop().await?;
        
        info!("通信マネージャーを停止しました");
        Ok(())
    }
    
    /// メッセージを送信
    pub async fn send_message(&self, message: DistributedMessage) -> Result<()> {
        self.transport.send_message(message).await
    }
    
    /// 特定のノードにメッセージを送信
    pub async fn send_to_node(
        &self,
        recipient: &NodeId,
        message_type: MessageType,
        payload: Vec<u8>,
    ) -> Result<MessageId> {
        let message = DistributedMessage {
            id: MessageId::new().to_string(),
            sender: self.local_node_id.to_string(),
            recipient: recipient.to_string(),
            message_type,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            payload,
            metadata: HashMap::new(),
        };
        
        let message_id = MessageId::from_string(message.id.clone());
        
        self.transport.send_message(message).await?;
        
        Ok(message_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_id() {
        let id = MessageId::new();
        assert!(!id.as_str().is_empty());
        
        let id2 = MessageId::from_string("test-id".to_string());
        assert_eq!(id2.as_str(), "test-id");
    }
    
    #[test]
    fn test_distributed_message() {
        let message = DistributedMessage::new(
            "sender".to_string(),
            "recipient".to_string(),
            MessageType::Heartbeat,
            vec![1, 2, 3],
        );
        
        assert_eq!(message.sender, "sender");
        assert_eq!(message.recipient, "recipient");
        assert_eq!(message.message_type, MessageType::Heartbeat);
        assert_eq!(message.payload, vec![1, 2, 3]);
    }
} 