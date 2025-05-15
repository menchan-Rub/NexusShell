use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, info, warn, error, trace};
use uuid::Uuid;

use super::PipelineData;

/// パイプの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeStatus {
    /// 初期化済み
    Initialized,
    /// 接続済み
    Connected,
    /// 実行中
    Running,
    /// 完了
    Completed,
    /// 失敗
    Failed,
    /// 閉じられた
    Closed,
}

impl fmt::Display for PipeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialized => write!(f, "初期化済み"),
            Self::Connected => write!(f, "接続済み"),
            Self::Running => write!(f, "実行中"),
            Self::Completed => write!(f, "完了"),
            Self::Failed => write!(f, "失敗"),
            Self::Closed => write!(f, "閉じられた"),
        }
    }
}

/// パイプ設定
#[derive(Debug, Clone)]
pub struct PipeConfig {
    /// バッファサイズ
    pub buffer_size: usize,
    /// スロットル制限（毎秒データ数）
    pub throttle_limit: Option<usize>,
    /// フィルタ関数
    pub filter: Option<Arc<dyn Fn(&PipelineData) -> bool + Send + Sync>>,
    /// 変換関数
    pub transform: Option<Arc<dyn Fn(PipelineData) -> Result<PipelineData> + Send + Sync>>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

impl Default for PipeConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024,
            throttle_limit: None,
            filter: None,
            transform: None,
            metadata: HashMap::new(),
        }
    }
}

/// パイプインターフェイス
#[async_trait]
pub trait Pipe: Send + Sync {
    /// パイプの名前を取得
    fn name(&self) -> &str;
    
    /// パイプのIDを取得
    fn id(&self) -> &str;
    
    /// パイプの状態を取得
    fn status(&self) -> PipeStatus;
    
    /// パイプのメタデータを取得
    fn metadata(&self) -> &HashMap<String, String>;
    
    /// データを書き込む
    async fn write(&self, data: PipelineData) -> Result<()>;
    
    /// データを読み込む
    async fn read(&self) -> Result<Option<PipelineData>>;
    
    /// パイプを閉じる
    async fn close(&self) -> Result<()>;
    
    /// パイプをクローン
    fn clone_pipe(&self) -> Box<dyn Pipe>;
}

/// 標準パイプ実装
pub struct StandardPipe {
    /// パイプID
    id: String,
    /// パイプ名
    name: String,
    /// パイプ状態
    status: tokio::sync::RwLock<PipeStatus>,
    /// 送信チャネル
    tx: mpsc::Sender<PipelineData>,
    /// 受信チャネル
    rx: tokio::sync::Mutex<mpsc::Receiver<PipelineData>>,
    /// 設定
    config: PipeConfig,
    /// 処理済みデータ数
    processed_count: tokio::sync::Mutex<u64>,
    /// 最終処理時刻
    last_processed: tokio::sync::Mutex<std::time::Instant>,
}

impl StandardPipe {
    /// 新しいパイプを作成
    pub fn new(name: &str) -> Self {
        Self::with_config(name, PipeConfig::default())
    }
    
    /// 設定を指定してパイプを作成
    pub fn with_config(name: &str, config: PipeConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.buffer_size);
        
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            status: tokio::sync::RwLock::new(PipeStatus::Initialized),
            tx,
            rx: tokio::sync::Mutex::new(rx),
            config,
            processed_count: tokio::sync::Mutex::new(0),
            last_processed: tokio::sync::Mutex::new(std::time::Instant::now()),
        }
    }
    
    /// スロットル処理
    async fn apply_throttle(&self) -> Result<()> {
        if let Some(limit) = self.config.throttle_limit {
            let mut last_processed = self.last_processed.lock().await;
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(*last_processed);
            
            // 1秒当たりの制限を適用
            let desired_interval = std::time::Duration::from_secs_f64(1.0 / limit as f64);
            
            if elapsed < desired_interval {
                let sleep_duration = desired_interval.checked_sub(elapsed)
                    .unwrap_or_else(|| std::time::Duration::from_millis(0));
                
                tokio::time::sleep(sleep_duration).await;
            }
            
            *last_processed = std::time::Instant::now();
        }
        
        Ok(())
    }
}

#[async_trait]
impl Pipe for StandardPipe {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn id(&self) -> &str {
        &self.id
    }
    
    fn status(&self) -> PipeStatus {
        *self.status.blocking_read()
    }
    
    fn metadata(&self) -> &HashMap<String, String> {
        &self.config.metadata
    }
    
    async fn write(&self, data: PipelineData) -> Result<()> {
        // スロットル制限を適用
        self.apply_throttle().await?;
        
        // フィルター適用
        if let Some(filter) = &self.config.filter {
            if !filter(&data) {
                return Ok(());
            }
        }
        
        // 変換関数を適用
        let processed_data = if let Some(transform) = &self.config.transform {
            transform(data)?
        } else {
            data
        };
        
        // データを送信
        self.tx.send(processed_data).await.map_err(|e| anyhow!("パイプへのデータ送信に失敗: {}", e))?;
        
        // 処理カウントを増加
        let mut count = self.processed_count.lock().await;
        *count += 1;
        
        Ok(())
    }
    
    async fn read(&self) -> Result<Option<PipelineData>> {
        let mut rx = self.rx.lock().await;
        
        match rx.recv().await {
            Some(data) => Ok(Some(data)),
            None => Ok(None),
        }
    }
    
    async fn close(&self) -> Result<()> {
        let mut status = self.status.write().await;
        *status = PipeStatus::Closed;
        
        // チャネルは自動的に閉じられる（tx がドロップされると）
        
        Ok(())
    }
    
    fn clone_pipe(&self) -> Box<dyn Pipe> {
        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        
        Box::new(StandardPipe {
            id: self.id.clone(),
            name: self.name.clone(),
            status: tokio::sync::RwLock::new(self.status()),
            tx,
            rx: tokio::sync::Mutex::new(rx),
            config: self.config.clone(),
            processed_count: tokio::sync::Mutex::new(0),
            last_processed: tokio::sync::Mutex::new(std::time::Instant::now()),
        })
    }
}

impl Clone for StandardPipe {
    fn clone(&self) -> Self {
        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            status: tokio::sync::RwLock::new(self.status()),
            tx,
            rx: tokio::sync::Mutex::new(rx),
            config: self.config.clone(),
            processed_count: tokio::sync::Mutex::new(0),
            last_processed: tokio::sync::Mutex::new(std::time::Instant::now()),
        }
    }
}

/// 共有パイプ実装
/// 複数のリーダーが同じデータを受信できるパイプ
pub struct SharedPipe {
    /// パイプID
    id: String,
    /// パイプ名
    name: String,
    /// パイプ状態
    status: tokio::sync::RwLock<PipeStatus>,
    /// 送信チャネル
    tx: mpsc::Sender<PipelineData>,
    /// ブロードキャスト送信チャネル
    broadcast_tx: tokio::sync::broadcast::Sender<PipelineData>,
    /// 設定
    config: PipeConfig,
    /// 処理済みデータ数
    processed_count: tokio::sync::Mutex<u64>,
    /// 最終処理時刻
    last_processed: tokio::sync::Mutex<std::time::Instant>,
}

impl SharedPipe {
    /// 新しい共有パイプを作成
    pub fn new(name: &str) -> Self {
        Self::with_config(name, PipeConfig::default())
    }
    
    /// 設定を指定して共有パイプを作成
    pub fn with_config(name: &str, config: PipeConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.buffer_size);
        let (broadcast_tx, _) = tokio::sync::broadcast::channel(config.buffer_size);
        
        let broadcast_tx_clone = broadcast_tx.clone();
        
        // 送信データをブロードキャストに転送するタスク
        tokio::spawn(async move {
            let mut rx = rx;
            
            while let Some(data) = rx.recv().await {
                if let Err(e) = broadcast_tx_clone.send(data) {
                    error!("ブロードキャストへのデータ送信に失敗: {}", e);
                    break;
                }
            }
        });
        
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            status: tokio::sync::RwLock::new(PipeStatus::Initialized),
            tx,
            broadcast_tx,
            config,
            processed_count: tokio::sync::Mutex::new(0),
            last_processed: tokio::sync::Mutex::new(std::time::Instant::now()),
        }
    }
    
    /// 購読者を作成
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<PipelineData> {
        self.broadcast_tx.subscribe()
    }
    
    /// スロットル処理
    async fn apply_throttle(&self) -> Result<()> {
        if let Some(limit) = self.config.throttle_limit {
            let mut last_processed = self.last_processed.lock().await;
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(*last_processed);
            
            // 1秒当たりの制限を適用
            let desired_interval = std::time::Duration::from_secs_f64(1.0 / limit as f64);
            
            if elapsed < desired_interval {
                let sleep_duration = desired_interval.checked_sub(elapsed)
                    .unwrap_or_else(|| std::time::Duration::from_millis(0));
                
                tokio::time::sleep(sleep_duration).await;
            }
            
            *last_processed = std::time::Instant::now();
        }
        
        Ok(())
    }
}

#[async_trait]
impl Pipe for SharedPipe {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn id(&self) -> &str {
        &self.id
    }
    
    fn status(&self) -> PipeStatus {
        *self.status.blocking_read()
    }
    
    fn metadata(&self) -> &HashMap<String, String> {
        &self.config.metadata
    }
    
    async fn write(&self, data: PipelineData) -> Result<()> {
        // スロットル制限を適用
        self.apply_throttle().await?;
        
        // フィルター適用
        if let Some(filter) = &self.config.filter {
            if !filter(&data) {
                return Ok(());
            }
        }
        
        // 変換関数を適用
        let processed_data = if let Some(transform) = &self.config.transform {
            transform(data)?
        } else {
            data
        };
        
        // データを送信
        self.tx.send(processed_data).await.map_err(|e| anyhow!("パイプへのデータ送信に失敗: {}", e))?;
        
        // 処理カウントを増加
        let mut count = self.processed_count.lock().await;
        *count += 1;
        
        Ok(())
    }
    
    async fn read(&self) -> Result<Option<PipelineData>> {
        let mut rx = self.broadcast_tx.subscribe();
        
        match rx.recv().await {
            Ok(data) => Ok(Some(data)),
            Err(tokio::sync::broadcast::error::RecvError::Closed) => Ok(None),
            Err(e) => Err(anyhow!("パイプからのデータ受信に失敗: {}", e)),
        }
    }
    
    async fn close(&self) -> Result<()> {
        let mut status = self.status.write().await;
        *status = PipeStatus::Closed;
        
        // チャネルは自動的に閉じられる（tx がドロップされると）
        
        Ok(())
    }
    
    fn clone_pipe(&self) -> Box<dyn Pipe> {
        Box::new(self.clone())
    }
}

impl Clone for SharedPipe {
    fn clone(&self) -> Self {
        // 同じブロードキャストチャネルを共有
        let (tx, _) = mpsc::channel(self.config.buffer_size);
        
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            status: tokio::sync::RwLock::new(self.status()),
            tx,
            broadcast_tx: self.broadcast_tx.clone(),
            config: self.config.clone(),
            processed_count: tokio::sync::Mutex::new(0),
            last_processed: tokio::sync::Mutex::new(std::time::Instant::now()),
        }
    }
} 