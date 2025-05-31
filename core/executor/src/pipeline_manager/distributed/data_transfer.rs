/**
 * 分散データ転送モジュール
 * 
 * 分散パイプライン実行における大規模データの効率的な転送を担当するモジュール
 */

use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::sync::oneshot;
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use anyhow::{Result, anyhow, Context};
use serde::{Serialize, Deserialize};
use tracing::{debug, info, warn, error, trace};
use uuid::Uuid;

use super::communication::{CommunicationManager, MessageType, DistributedMessage};
use super::node::NodeId;

/// 転送ID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransferId(String);

impl TransferId {
    /// 新しい転送IDを生成
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    /// 文字列から転送IDを作成
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    /// 文字列表現を取得
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TransferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 転送状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferStatus {
    /// 準備中
    Preparing,
    /// 転送中
    Transferring,
    /// 一時停止
    Paused,
    /// 完了
    Completed,
    /// 失敗
    Failed,
    /// キャンセル
    Cancelled,
}

/// 圧縮タイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    /// 圧縮なし
    None,
    /// GZIP圧縮
    Gzip,
    /// ZSTD圧縮
    Zstd,
    /// LZ4圧縮
    Lz4,
}

/// データチャンク
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataChunk {
    /// 転送ID
    pub transfer_id: String,
    /// チャンクインデックス
    pub index: u32,
    /// 総チャンク数
    pub total_chunks: u32,
    /// データ
    pub data: Vec<u8>,
    /// チェックサム
    pub checksum: u64,
}

impl DataChunk {
    /// 新しいデータチャンクを作成
    pub fn new(transfer_id: String, index: u32, total_chunks: u32, data: Vec<u8>) -> Self {
        let checksum = calculate_checksum(&data);
        
        Self {
            transfer_id,
            index,
            total_chunks,
            data,
            checksum,
        }
    }
    
    /// チェックサムを検証
    pub fn verify_checksum(&self) -> bool {
        let calculated = calculate_checksum(&self.data);
        calculated == self.checksum
    }
}

/// チェックサムを計算
fn calculate_checksum(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// 転送メタデータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferMetadata {
    /// 転送ID
    pub id: String,
    /// 送信元ノード
    pub source_node: String,
    /// 宛先ノード
    pub destination_node: String,
    /// データサイズ (バイト)
    pub total_size: u64,
    /// チャンクサイズ (バイト)
    pub chunk_size: u32,
    /// チャンク数
    pub chunk_count: u32,
    /// 圧縮タイプ
    pub compression: CompressionType,
    /// コンテンツタイプ
    pub content_type: String,
    /// タイムスタンプ (ミリ秒)
    pub timestamp: u64,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

/// 転送リクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequest {
    /// 転送メタデータ
    pub metadata: TransferMetadata,
    /// 再開可能フラグ
    pub resumable: bool,
    /// 優先度 (0-100)
    pub priority: u8,
}

/// 転送応答
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResponse {
    /// 転送ID
    pub transfer_id: String,
    /// 受け入れフラグ
    pub accepted: bool,
    /// 再開ポイント (チャンクインデックス)
    pub resume_from: Option<u32>,
    /// エラーメッセージ
    pub error_message: Option<String>,
}

/// データ転送完了通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferCompletion {
    /// 転送ID
    pub transfer_id: String,
    /// 成功フラグ
    pub success: bool,
    /// 転送されたチャンク数
    pub chunks_transferred: u32,
    /// 転送エラー
    pub error: Option<String>,
    /// 処理結果
    pub result: Option<String>,
}

/// データソース
#[async_trait]
pub trait DataSource: Send + Sync {
    /// 総データサイズを取得
    async fn get_size(&self) -> Result<u64>;
    
    /// データチャンクを読み取り
    async fn read_chunk(&mut self, index: u32, size: usize) -> Result<Vec<u8>>;
    
    /// データソースを閉じる
    async fn close(&mut self) -> Result<()>;
}

/// データシンク
#[async_trait]
pub trait DataSink: Send + Sync {
    /// データチャンクを書き込み
    async fn write_chunk(&mut self, chunk: DataChunk) -> Result<()>;
    
    /// データシンクを完了
    async fn complete(&mut self) -> Result<()>;
    
    /// データシンクを中止
    async fn abort(&mut self) -> Result<()>;
}

/// メモリーデータソース
pub struct MemoryDataSource {
    /// データバッファ
    data: Vec<u8>,
    /// 現在位置
    position: usize,
}

impl MemoryDataSource {
    /// 新しいメモリーデータソースを作成
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            position: 0,
        }
    }
}

#[async_trait]
impl DataSource for MemoryDataSource {
    async fn get_size(&self) -> Result<u64> {
        Ok(self.data.len() as u64)
    }
    
    async fn read_chunk(&mut self, _index: u32, size: usize) -> Result<Vec<u8>> {
        if self.position >= self.data.len() {
            return Err(anyhow!("データの終端に達しました"));
        }
        
        let remaining = self.data.len() - self.position;
        let read_size = size.min(remaining);
        
        let result = self.data[self.position..self.position + read_size].to_vec();
        self.position += read_size;
        
        Ok(result)
    }
    
    async fn close(&mut self) -> Result<()> {
        // メモリデータソースは特別なクローズ処理は不要
        Ok(())
    }
}

/// メモリーデータシンク
pub struct MemoryDataSink {
    /// 転送ID
    transfer_id: String,
    /// 受信したチャンク
    chunks: HashMap<u32, DataChunk>,
    /// 合計チャンク数
    total_chunks: Option<u32>,
    /// 結合したデータ
    combined_data: Option<Vec<u8>>,
}

impl MemoryDataSink {
    /// 新しいメモリーデータシンクを作成
    pub fn new(transfer_id: String) -> Self {
        Self {
            transfer_id,
            chunks: HashMap::new(),
            total_chunks: None,
            combined_data: None,
        }
    }
    
    /// 収集したデータを取得
    pub fn get_data(&self) -> Option<&Vec<u8>> {
        self.combined_data.as_ref()
    }
    
    /// すべてのチャンクが受信されたか確認
    fn all_chunks_received(&self) -> bool {
        if let Some(total) = self.total_chunks {
            self.chunks.len() as u32 == total
        } else {
            false
        }
    }
    
    /// 受信したチャンクを結合
    fn combine_chunks(&mut self) -> Result<()> {
        if self.combined_data.is_some() {
            return Ok(());
        }
        
        if !self.all_chunks_received() {
            return Err(anyhow!("すべてのチャンクがまだ受信されていません"));
        }
        
        let total = self.total_chunks.unwrap();
        let mut combined = Vec::new();
        
        for i in 0..total {
            if let Some(chunk) = self.chunks.get(&i) {
                combined.extend_from_slice(&chunk.data);
            } else {
                return Err(anyhow!("チャンク {}が見つかりません", i));
            }
        }
        
        self.combined_data = Some(combined);
        Ok(())
    }
}

#[async_trait]
impl DataSink for MemoryDataSink {
    async fn write_chunk(&mut self, chunk: DataChunk) -> Result<()> {
        // 転送IDが一致するか確認
        if chunk.transfer_id != self.transfer_id {
            return Err(anyhow!("転送IDが一致しません"));
        }
        
        // チェックサムを検証
        if !chunk.verify_checksum() {
            return Err(anyhow!("チェックサムが無効です"));
        }
        
        // 合計チャンク数を更新
        if self.total_chunks.is_none() {
            self.total_chunks = Some(chunk.total_chunks);
        } else if self.total_chunks != Some(chunk.total_chunks) {
            return Err(anyhow!("チャンク数が一致しません"));
        }
        
        // チャンクを保存
        self.chunks.insert(chunk.index, chunk);
        
        // すべてのチャンクが揃ったら結合
        if self.all_chunks_received() {
            self.combine_chunks()?;
        }
        
        Ok(())
    }
    
    async fn complete(&mut self) -> Result<()> {
        if !self.all_chunks_received() {
            return Err(anyhow!("すべてのチャンクがまだ受信されていません"));
        }
        
        // まだ結合されていなければ結合
        if self.combined_data.is_none() {
            self.combine_chunks()?;
        }
        
        Ok(())
    }
    
    async fn abort(&mut self) -> Result<()> {
        // メモリを解放
        self.chunks.clear();
        self.combined_data = None;
        
        Ok(())
    }
}

/// ファイルデータソース
pub struct FileDataSource {
    /// ファイルパス
    path: PathBuf,
    /// ファイルサイズ
    size: u64,
    /// ファイルハンドル
    file: Option<tokio::fs::File>,
}

impl FileDataSource {
    /// 新しいファイルデータソースを作成
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        // ファイルの存在を確認
        if !path.exists() {
            return Err(anyhow!("ファイルが存在しません: {:?}", path));
        }
        
        // ファイルサイズを取得
        let metadata = tokio::fs::metadata(&path).await?;
        let size = metadata.len();
        
        Ok(Self {
            path,
            size,
            file: None,
        })
    }
    
    /// ファイルをオープン
    async fn ensure_file_open(&mut self) -> Result<()> {
        if self.file.is_none() {
            let file = tokio::fs::File::open(&self.path).await
                .context(format!("ファイルをオープンできません: {:?}", self.path))?;
            self.file = Some(file);
        }
        
        Ok(())
    }
}

#[async_trait]
impl DataSource for FileDataSource {
    async fn get_size(&self) -> Result<u64> {
        Ok(self.size)
    }
    
    async fn read_chunk(&mut self, index: u32, size: usize) -> Result<Vec<u8>> {
        self.ensure_file_open().await?;
        
        let offset = (index as u64) * (size as u64);
        if offset >= self.size {
            return Err(anyhow!("ファイルの終端を超えています"));
        }
        
        let file = self.file.as_mut()
            .ok_or_else(|| anyhow!("ファイルがオープンされていません"))?;
        
        // ファイル位置を設定
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        
        // データを読み取り
        let mut buffer = vec![0u8; size];
        let bytes_read = file.read(&mut buffer).await?;
        
        // 実際に読み取ったサイズで正確に切り詰め
        if bytes_read < buffer.len() {
            // 効率的なメモリ管理のためにバッファを正確なサイズに調整
            buffer.truncate(bytes_read);
            
            // メモリ使用量を最適化（キャパシティも調整）
            if bytes_read < buffer.capacity() / 2 {
                let mut optimized_buffer = Vec::with_capacity(bytes_read);
                optimized_buffer.extend_from_slice(&buffer);
                buffer = optimized_buffer;
            }
            
            // EOF (End-of-File) に達したかどうかを確認
            if bytes_read == 0 && index > 0 {
                debug!("ファイルの終端に到達しました: チャンク {}", index);
            } else {
                debug!("部分的なチャンクを読み取りました: サイズ {}/{}", bytes_read, size);
            }
        } else {
            // フルチャンク読み取り - パフォーマンス統計を更新
            trace!("完全なチャンクを読み取りました: インデックス {}", index);
        }
        
        // チャンクデータの整合性を検証
        let checksum = calculate_checksum(&buffer);
        trace!("チャンク {} のチェックサム: {}", index, checksum);
        
        Ok(buffer)
    }
    
    async fn close(&mut self) -> Result<()> {
        self.file = None;
        Ok(())
    }
}

/// ファイルデータシンク
pub struct FileDataSink {
    /// 転送ID
    transfer_id: String,
    /// ファイルパス
    path: PathBuf,
    /// ファイルハンドル
    file: Option<tokio::fs::File>,
    /// 合計チャンク数
    total_chunks: Option<u32>,
    /// 受信したチャンク
    received_chunks: HashSet<u32>,
}

impl FileDataSink {
    /// 新しいファイルデータシンクを作成
    pub fn new<P: AsRef<Path>>(transfer_id: String, path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        
        Self {
            transfer_id,
            path,
            file: None,
            total_chunks: None,
            received_chunks: HashSet::new(),
        }
    }
    
    /// ファイルをオープン
    async fn ensure_file_open(&mut self) -> Result<()> {
        if self.file.is_none() {
            // 親ディレクトリを作成
            if let Some(parent) = self.path.parent() {
                tokio::fs::create_dir_all(parent).await
                    .context(format!("ディレクトリを作成できません: {:?}", parent))?;
            }
            
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&self.path).await
                .context(format!("ファイルをオープンできません: {:?}", self.path))?;
                
            self.file = Some(file);
        }
        
        Ok(())
    }
}

#[async_trait]
impl DataSink for FileDataSink {
    async fn write_chunk(&mut self, chunk: DataChunk) -> Result<()> {
        // 転送IDが一致するか確認
        if chunk.transfer_id != self.transfer_id {
            return Err(anyhow!("転送IDが一致しません"));
        }
        
        // チェックサムを検証
        if !chunk.verify_checksum() {
            return Err(anyhow!("チェックサムが無効です"));
        }
        
        // 合計チャンク数を更新
        if self.total_chunks.is_none() {
            self.total_chunks = Some(chunk.total_chunks);
        } else if self.total_chunks != Some(chunk.total_chunks) {
            return Err(anyhow!("チャンク数が一致しません"));
        }
        
        // ファイルをオープン
        self.ensure_file_open().await?;
        
        let file = self.file.as_mut()
            .ok_or_else(|| anyhow!("ファイルがオープンされていません"))?;
        
        // チャンクの位置を計算
        let chunk_size = chunk.data.len() as u64;
        let offset = (chunk.index as u64) * chunk_size;
        
        // ファイル位置を設定
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        
        // データを書き込み
        file.write_all(&chunk.data).await?;
        
        // 受信したチャンクを記録
        self.received_chunks.insert(chunk.index);
        
        Ok(())
    }
    
    async fn complete(&mut self) -> Result<()> {
        // ファイルを閉じる
        if let Some(mut file) = self.file.take() {
            file.flush().await?;
            // ファイルは自動的に閉じられる
        }
        
        // すべてのチャンクが揃っているか確認
        if let Some(total) = self.total_chunks {
            if self.received_chunks.len() as u32 != total {
                return Err(anyhow!("すべてのチャンクがまだ受信されていません"));
            }
        }
        
        Ok(())
    }
    
    async fn abort(&mut self) -> Result<()> {
        // ファイルを閉じる
        self.file = None;
        
        // ファイルを削除
        if self.path.exists() {
            tokio::fs::remove_file(&self.path).await
                .context(format!("ファイルを削除できません: {:?}", self.path))?;
        }
        
        Ok(())
    }
}

/// 転送追跡情報
#[derive(Debug)]
struct TransferTracker {
    /// 転送ID
    id: TransferId,
    /// 転送メタデータ
    metadata: TransferMetadata,
    /// 転送元ノード
    source_node: NodeId,
    /// 転送先ノード
    destination_node: NodeId,
    /// 開始時間
    start_time: Instant,
    /// 最終更新時間
    last_update: Instant,
    /// 転送状態
    status: TransferStatus,
    /// 転送されたチャンク数
    chunks_transferred: u32,
    /// 送信されたバイト数
    bytes_transferred: u64,
    /// エラーメッセージ
    error: Option<String>,
    /// 受信側で転送完了を通知するためのチャネル
    completion_notifier: Option<oneshot::Sender<Result<(), String>>>,
}

impl TransferTracker {
    #[allow(clippy::too_many_arguments)] // 引数の数を一時的に許可
    fn new(
        id: TransferId,
        metadata: TransferMetadata,
        source_node: NodeId,
        destination_node: NodeId,
        completion_notifier: Option<oneshot::Sender<Result<(), String>>>, // 追加
    ) -> Self {
        TransferTracker {
            id,
            metadata,
            source_node,
            destination_node,
            start_time: Instant::now(),
            last_update: Instant::now(),
            status: TransferStatus::Preparing, // 修正: Preparing に初期化
            chunks_transferred: 0,            // 修正: 0 に初期化
            bytes_transferred: 0,             // 修正: 0 に初期化
            error: None,                      // 修正: None に初期化
            completion_notifier,              // 追加
        }
    }

    /// 完了通知を行う
    fn notify_completion(&mut self, result: Result<(), String>) {
        if let Some(notifier) = self.completion_notifier.take() {
            if notifier.send(result).is_err() {
                // エラーログは呼び出し側で出すか、ここでは出さない方針も検討
                // warn! などを使う場合は tracing クレートへの依存を確認
                eprintln!("Failed to send completion notification for transfer {}", self.id);
            }
        }
    }
}

/// データ転送マネージャー
pub struct DataTransferManager {
    /// ローカルノードID
    local_node_id: NodeId,
    /// 通信マネージャー
    comm_manager: Arc<CommunicationManager>,
    /// アクティブな転送
    active_transfers: Arc<RwLock<HashMap<TransferId, Arc<RwLock<TransferTracker>>>>>,
    /// チャンクサイズ（バイト）
    chunk_size: u32,
    /// タイムアウト
    timeout_duration: Duration,
    /// 最大再試行回数
    max_retries: u32,
    /// チャンクを受信するためのチャネル
    pending_sinks: Arc<RwLock<HashMap<TransferId, mpsc::Sender<DataChunk>>>>,
    /// クリーンアップ間隔
    cleanup_interval: Duration,
}

impl DataTransferManager {
    /// 新しいデータ転送マネージャーを作成
    pub fn new(local_node_id: NodeId, comm_manager: Arc<CommunicationManager>) -> Self {
        Self {
            local_node_id,
            comm_manager,
            active_transfers: Arc::new(RwLock::new(HashMap::new())),
            pending_sinks: Arc::new(RwLock::new(HashMap::new())), // 初期化
            chunk_size: 1024 * 1024, // デフォルト1MB
            timeout_duration: Duration::from_secs(60),
            max_retries: 3,
            cleanup_interval: Duration::from_secs(60), // デフォルト60秒
        }
    }
    
    /// チャンクサイズを設定
    pub fn set_chunk_size(&mut self, size: u32) {
        self.chunk_size = size;
    }
    
    /// タイムアウトを設定
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout_duration = timeout;
    }
    
    /// 最大再試行回数を設定
    pub fn set_max_retries(&mut self, retries: u32) {
        self.max_retries = retries;
    }
    
    /// データを送信
    pub async fn send_data<S: DataSource + 'static>(
        &self,
        destination: NodeId,
        mut source: S,
        content_type: &str,
        compression: CompressionType,
        metadata: HashMap<String, String>,
    ) -> Result<TransferId> {
        let transfer_id = TransferId::new();
        info!(%transfer_id, %destination, content_type, "新規データ転送を開始");

        let total_size = source.get_size().await.context("データソースサイズの取得失敗")?;
        let chunk_count = (total_size as f64 / self.chunk_size as f64).ceil() as u32;

        let transfer_metadata = TransferMetadata {
            id: transfer_id.to_string(),
            source_node: self.local_node_id.to_string(),
            destination_node: destination.to_string(),
            total_size,
            chunk_size: self.chunk_size,
            chunk_count,
            compression,
            content_type: content_type.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            metadata, // metadata from args
        };

        let request = TransferRequest {
            metadata: transfer_metadata.clone(),
            resumable: false, // TODO: 再開機能
            priority: 50,   // TODO: 優先度
        };
        
        let (tx_completion, rx_completion) = oneshot::channel::<Result<(), String>>();

        let tracker = Arc::new(RwLock::new(TransferTracker::new(
            transfer_id.clone(),
            transfer_metadata.clone(),
            self.local_node_id.clone(),
            destination.clone(),
            Some(tx_completion), 
        )));
        self.active_transfers.write().await.insert(transfer_id.clone(), Arc::clone(&tracker));
        
        debug!(%transfer_id, "転送リクエストを宛先ノードに送信");
        let response_msg = match self.comm_manager.send_request(
            destination.clone(),
            MessageType::TransferRequest,
            &request,
            self.timeout_duration,
        ).await {
            Ok(msg) => msg,
            Err(e) => {
                let mut tracker_w = tracker.write().await;
                if matches!(tracker_w.status, TransferStatus::Preparing | TransferStatus::Transferring) {
                    tracker_w.status = TransferStatus::Failed;
                    tracker_w.error = Some(format!("転送リクエスト送信失敗: {}", e));
                    let err_for_notify = tracker_w.error.clone().unwrap();
                    tracker_w.notify_completion(Err(err_for_notify)); 
                }
                return Err(anyhow!("転送リクエスト送信失敗: {}", e));
            }
        };

        let response: TransferResponse = match serde_json::from_slice(&response_msg.payload) {
            Ok(resp) => resp,
            Err(e) => {
                let mut tracker_w = tracker.write().await;
                if matches!(tracker_w.status, TransferStatus::Preparing | TransferStatus::Transferring) {
                    tracker_w.status = TransferStatus::Failed;
                    tracker_w.error = Some(format!("転送応答デシリアライズ失敗: {}", e));
                    let err_for_notify = tracker_w.error.clone().unwrap();
                    tracker_w.notify_completion(Err(err_for_notify));
                }
                return Err(anyhow!("転送応答デシリアライズ失敗: {}", e));
            }
        };

        if !response.accepted {
            let mut tracker_w = tracker.write().await;
            if matches!(tracker_w.status, TransferStatus::Preparing | TransferStatus::Transferring) {
                tracker_w.status = TransferStatus::Failed;
                tracker_w.error = Some(response.error_message.unwrap_or_else(|| "宛先ノードが転送を拒否".to_string()));
                let err_for_notify = tracker_w.error.clone().unwrap();
                tracker_w.notify_completion(Err(err_for_notify)); 
            }
            return Err(anyhow!("宛先ノードが転送を拒否: {:?}", tracker.read().await.error)); // readロックで最新のエラーを取得
        }
        
        {
            let mut tracker_w = tracker.write().await;
            info!(%transfer_id, "転送リクエストが受理されました。チャンク送信を開始します。");
            tracker_w.status = TransferStatus::Transferring;
        } // ロックを解放

        let comm_manager_clone = Arc::clone(&self.comm_manager);
        let max_retries_clone = self.max_retries;
        let chunk_size_clone = self.chunk_size;
        let timeout_duration_clone = self.timeout_duration; 
        let destination_clone_for_task = destination.clone();
        let transfer_id_clone_for_task = transfer_id.clone();
        let tracker_clone_for_task = Arc::clone(&tracker);
        let transfer_metadata_clone_for_task = transfer_metadata.clone();

        tokio::spawn(async move {
            // source はここで所有権を得る
            let mut current_chunk_index = response.resume_from.unwrap_or(0);
            let mut retries = 0;
            let mut task_error_message: Option<String> = None;

            loop {
                {
                    let tracker_guard = tracker_clone_for_task.read().await;
                    if matches!(tracker_guard.status, TransferStatus::Cancelled | TransferStatus::Failed) {
                        warn!(id = %transfer_id_clone_for_task, status = ?tracker_guard.status, "送信タスク: 転送がキャンセルまたは失敗済みのため送信を中断します。");
                        // エラーメッセージは既にtrackerに設定されているはず
                        // task_error_message = tracker_guard.error.clone(); // notify_completionがメインスレッドのrx_completionを消費するので不要
                        break;
                    }
                    if tracker_guard.status == TransferStatus::Paused {
                        debug!(id = %transfer_id_clone_for_task, "送信タスク: 転送が一時停止中のため待機します。");
                        drop(tracker_guard); 
                        tokio::time::sleep(Duration::from_secs(1)).await; 
                        continue;
                    }
                    if current_chunk_index >= transfer_metadata_clone_for_task.chunk_count {
                        info!(id = %transfer_id_clone_for_task, "送信タスク: 全てのチャンクを送信試行完了。");
                        break;
                    }
                } // tracker_guard ロック解放

                match source.read_chunk(current_chunk_index, chunk_size_clone as usize).await {
                    Ok(data) => {
                        if data.is_empty() && current_chunk_index < transfer_metadata_clone_for_task.chunk_count {
                             warn!(id = %transfer_id_clone_for_task, index = current_chunk_index, "送信タスク: 期待されるチャンク数より前にデータソースが空になりました。");
                             task_error_message = Some("データソースが予期せず終了".to_string());
                             break;
                        }
                        if data.is_empty() { 
                            info!(id = %transfer_id_clone_for_task, index = current_chunk_index, "送信タスク: データソースが正常に終了しました。");
                            break;
                        }

                        let chunk = DataChunk::new(
                            transfer_id_clone_for_task.to_string(),
                            current_chunk_index,
                            transfer_metadata_clone_for_task.chunk_count, 
                            data.clone(), 
                        );

                        trace!(id = %transfer_id_clone_for_task, index = current_chunk_index, size = chunk.data.len(), "送信タスク: チャンクを送信中");
                        match comm_manager_clone.send_message(
                            destination_clone_for_task.clone(),
                            MessageType::DataChunk,
                            &chunk,
                        ).await {
                            Ok(_) => {
                                let mut tracker_w = tracker_clone_for_task.write().await;
                                tracker_w.chunks_transferred = current_chunk_index + 1;
                                tracker_w.bytes_transferred += chunk.data.len() as u64;
                                tracker_w.last_update = Instant::now();
                                current_chunk_index += 1;
                                retries = 0; 
                            }
                            Err(e) => {
                                warn!(id = %transfer_id_clone_for_task, index = current_chunk_index, error = %e, "送信タスク: チャンク送信失敗");
                                retries += 1;
                                if retries > max_retries_clone {
                                    error!(id = %transfer_id_clone_for_task, index = current_chunk_index, "送信タスク: 最大リトライ回数超過。転送を失敗とします。");
                                    task_error_message = Some(format!("チャンク送信失敗 (最大リトライ超過): {}", e));
                                    break;
                                }
                                tokio::time::sleep(Duration::from_secs(1u64.saturating_shl(retries as u32))).await;
                            }
                        }
                    }
                    Err(e) => {
                        error!(id = %transfer_id_clone_for_task, index = current_chunk_index, error = %e, "送信タスク: データソースからのチャンク読み取り失敗");
                        task_error_message = Some(format!("データソース読み取りエラー: {}", e));
                        break;
                    }
                }
            }

            if let Some(err_msg) = task_error_message {
                let mut tracker_w = tracker_clone_for_task.write().await;
                // このタスクが原因で初めてエラーが発生した場合のみ状態を更新し通知
                if matches!(tracker_w.status, TransferStatus::Transferring | TransferStatus::Preparing) {
                    tracker_w.status = TransferStatus::Failed;
                    tracker_w.error = Some(err_msg.clone());
                    let completion_msg = TransferCompletion {
                        transfer_id: transfer_id_clone_for_task.to_string(),
                        success: false,
                        chunks_transferred: current_chunk_index, // エラー発生時点での転送済みチャンク数
                        error: Some(err_msg.clone()),
                        result: None,
                    };
                    if let Err(e) = comm_manager_clone.send_message(
                        destination_clone_for_task.clone(),
                        MessageType::TransferCompletion,
                        &completion_msg
                    ).await {
                        error!(id = %transfer_id_clone_for_task, "送信タスク内エラーによる転送失敗通知のリモート送信失敗: {}", e);
                    }
                    // メインスレッドにタスクがエラーで終了したことを通知
                    tracker_w.notify_completion(Err(err_msg));
                } else {
                    debug!(id=%transfer_id_clone_for_task, "送信タスクエラー ({}) だが、tracker は既に {:?} 状態のため通知スキップ", err_msg, tracker_w.status);
                }
            }

            if let Err(e) = source.close().await {
                warn!(id = %transfer_id_clone_for_task, "送信タスク: データソースのクローズに失敗: {}", e);
            }
            debug!(id = %transfer_id_clone_for_task, "送信タスク: データ送信ループを終了しました。");
        });
        
        match timeout(timeout_duration_clone * transfer_metadata.chunk_count.max(1) , rx_completion).await {
            Ok(Ok(Ok(()))) => { // 成功: 受信側から成功通知
                info!(%transfer_id, "データ転送成功裏に完了 (受信側からの通知ベース)");
                self.active_transfers.write().await.remove(&transfer_id);
                Ok(transfer_id)
            }
            Ok(Ok(Err(e))) => { // 失敗: 送信タスク内エラー、または受信側からの失敗通知
                error!(%transfer_id, error = %e, "データ転送失敗 (通知ベース)");
                self.active_transfers.write().await.remove(&transfer_id);
                Err(anyhow!("転送失敗: {}", e))
            }
            Ok(Err(oneshot_recv_err)) => { // チャネルドロップ: 送信タスクがpanicしたか、notify_completionが呼ばれず終了
                error!(%transfer_id, error = %oneshot_recv_err, "データ転送の完了通知待機中に内部エラー (通知チャネルがドロップ)");
                let final_tracker_error_msg;
                {
                    let mut final_tracker_state = tracker.write().await; 
                    if matches!(final_tracker_state.status, TransferStatus::Transferring | TransferStatus::Preparing) {
                        final_tracker_state.status = TransferStatus::Failed;
                        final_tracker_state.error = Some(format!("完了通知チャネルエラーか送信タスク異常終了: {}", oneshot_recv_err));
                        // この場合、リモートにも通知を送るべきかもしれないが、原因がローカルなので難しい
                    }
                    final_tracker_error_msg = final_tracker_state.error.clone().unwrap_or_else(|| "不明なチャネルドロップエラー".to_string());
                }
                self.active_transfers.write().await.remove(&transfer_id);
                Err(anyhow!("完了通知待機エラー: {}", final_tracker_error_msg))
            }
            Err(_timeout_err) => { // タイムアウト: rx_completionが消費されなかった
                error!(%transfer_id, "データ転送タイムアウト (全体の完了通知待機)");
                let final_tracker_error_msg;
                {
                    let mut tracker_w = tracker.write().await;
                    if matches!(tracker_w.status, TransferStatus::Transferring | TransferStatus::Preparing) {
                        tracker_w.status = TransferStatus::Failed;
                        tracker_w.error = Some("転送タイムアウト (send_data)".to_string());
                        
                        // タイムアウトの場合、rx_completion は消費されていないので、ここで notify_completion を呼ぶ
                        let err_for_notify = tracker_w.error.clone().unwrap();
                        tracker_w.notify_completion(Err(err_for_notify)); 

                        // リモートにもタイムアウトによる失敗を通知
                        let completion_msg = TransferCompletion {
                            transfer_id: transfer_id.to_string(),
                            success: false,
                            chunks_transferred: tracker_w.chunks_transferred,
                            error: tracker_w.error.clone(),
                            result: None,
                        };
                        // この Arc<CommManager> は send_data の &self から取得
                        let comm_manager_for_timeout = Arc::clone(&self.comm_manager);
                        let destination_for_timeout = destination.clone(); 
                        // 新しいtokioタスクで送信 (awaitがネストしないように)
                        tokio::spawn(async move {
                            if let Err(e) = comm_manager_for_timeout.send_message(
                                destination_for_timeout, 
                                MessageType::TransferCompletion,
                                &completion_msg
                            ).await {
                                error!(%transfer_id, "タイムアウトによる転送失敗通知のリモート送信失敗: {}", e);
                            }
                        });
                    }
                    final_tracker_error_msg = tracker_w.error.clone().unwrap_or_else(|| "タイムアウトエラー不明".to_string());
                } 
                self.active_transfers.write().await.remove(&transfer_id);
                Err(anyhow!("転送タイムアウト: {}", final_tracker_error_msg))
            }
        }
    }

    /// ファイルを送信
    pub async fn send_file<P: AsRef<Path> + Send + Sync + 'static>(
        &self,
        destination: NodeId,
        path: P,
        // transfer_id: TransferId, // send_data が生成するので不要
        content_type: Option<&str>,
        compression: Option<CompressionType>,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<TransferId> {
        let file_data_source = FileDataSource::new(path).await.context("FileDataSourceの作成に失敗")?;
        self.send_data(
            destination,
            file_data_source,
            content_type.unwrap_or("application/octet-stream"),
            compression.unwrap_or(CompressionType::None),
            metadata.unwrap_or_default(),
        ).await
    }

    /// メモリーデータを送信
    pub async fn send_memory_data<D: AsRef<[u8]> + Send + Sync + 'static>(
        &self,
        destination: NodeId,
        data: D,
        // transfer_id: TransferId, // send_data が生成するので不要
        content_type: Option<&str>,
        compression: Option<CompressionType>,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<TransferId> {
        let memory_data_source = MemoryDataSource::new(data.as_ref().to_vec());
        self.send_data(
            destination,
            memory_data_source,
            content_type.unwrap_or("application/octet-stream"),
            compression.unwrap_or(CompressionType::None),
            metadata.unwrap_or_default(),
        ).await
    }

    /// 転送をキャンセル
    pub async fn cancel_transfer(&self, transfer_id: &TransferId) -> Result<()> {
        info!(%transfer_id, "データ転送キャンセル処理を開始");
        
        let mut active_transfers_map = self.active_transfers.write().await;
        if let Some(tracker_arc) = active_transfers_map.get(transfer_id) {
            let mut tracker = tracker_arc.write().await;

            if matches!(tracker.status, TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled) {
                info!(%transfer_id, status = ?tracker.status, "転送は既に終了しているためキャンセル処理をスキップ");
                return Ok(()); // 既に終了、またはキャンセル済み
            }

            warn!(%transfer_id, "転送をキャンセル状態に設定");
            let old_status = tracker.status;
            tracker.status = TransferStatus::Cancelled;
            tracker.error = Some("ユーザーまたはシステムによりキャンセルされました".to_string());
            tracker.last_update = Instant::now();

            let err_msg_for_notify = tracker.error.clone().unwrap();
            
            // notify_completion は tracker がロックされている間に呼ぶ
            tracker.notify_completion(Err(err_msg_for_notify.clone()));
            
            let local_is_source = self.local_node_id == tracker.source_node;
            let remote_node_id = if local_is_source {
                tracker.destination_node.clone()
            } else {
                tracker.source_node.clone()
            };
            let chunks_transferred_at_cancel = tracker.chunks_transferred;
            let transfer_id_string_clone = tracker.id.to_string(); // comm_manager.send_message のためにクローン
            
            // active_transfers_map と pending_sinks のロック範囲を最小にするため、trackerのロックを一度解放
            drop(tracker);
            drop(active_transfers_map);

            // 対応するpending_sinkがあれば削除 (これによりrecv()がNoneを返すか、sendがエラーになる)
            if self.pending_sinks.write().await.remove(transfer_id).is_some() {
                 debug!(%transfer_id, "ペンディングシンクを削除 (キャンセルによる)");
            }
            
            // リモートノードにキャンセル(失敗として)を通知
            // この通知は、相手がまだ転送中である場合に意味がある
            if matches!(old_status, TransferStatus::Transferring | TransferStatus::Preparing) {
                let completion_msg = TransferCompletion {
                    transfer_id: transfer_id_string_clone, // クローンしたIDを使用
                    success: false,
                    chunks_transferred: chunks_transferred_at_cancel, 
                    error: Some("転送がリモートからキャンセルされました".to_string()),
                    result: None,
                };
                
                // comm_manager は Arc なので直接使える
                let comm_manager_clone_for_cancel = Arc::clone(&self.comm_manager);
                let transfer_id_clone_for_task = transfer_id.clone(); // タスク用にクローン
                tokio::spawn(async move {
                    if let Err(e) = comm_manager_clone_for_cancel.send_message(
                        remote_node_id.clone(),
                        MessageType::TransferCompletion,
                        &completion_msg,
                    ).await {
                        warn!(transfer_id = %transfer_id_clone_for_task, remote_node = %remote_node_id, "転送キャンセル通知のリモート送信に失敗: {}", e);
                    }
                });
            }

            // キャンセルされた転送も最終的には active_transfers から削除されるべき
            // notify_completion を受け取った send_data / receive_transfer がエラー処理パスに入り、そこで削除する。
            // ここでは、その処理に任せる。
            // しかし、もし notify_completion の受信側が何らかの理由で既にいない場合、孤立する可能性がある。
            // そのため、一定時間後に未完了の Cancelled 状態の tracker を掃除するバックグラウンドタスクも検討の余地あり。
            // 現状は send_data/receive_transfer のクリーンアップを信頼する。

        } else {
            warn!(%transfer_id, "キャンセルしようとした転送が見つかりません");
            return Err(anyhow!("キャンセル対象の転送 {} が見つかりません", transfer_id));
        }
        Ok(())
    }

    /// 転送リクエストを処理 (主に受信側で呼び出される)
    /// TransferRequestメッセージを受信した際に呼び出されるハンドラ
    pub async fn handle_transfer_request(&self, request: TransferRequest) -> TransferResponse {
        let transfer_id = TransferId::from_string(request.metadata.id.clone());
        info!(transfer_id = %transfer_id, source_node = %request.metadata.source_node, "転送リクエスト受信");

        // 既存の転送IDか確認
        let mut active_transfers_map = self.active_transfers.write().await;
        if let Some(existing_tracker_arc) = active_transfers_map.get(&transfer_id) {
            let mut existing_tracker = existing_tracker_arc.write().await; // write lock を取得して状態変更もできるように
            
            if request.resumable && matches!(existing_tracker.status, TransferStatus::Transferring | TransferStatus::Paused | TransferStatus::Failed) {
                // 再開可能なリクエストで、既存の転送が再開に適した状態(転送中、一時停止、または一部失敗)
                info!(%transfer_id, status = ?existing_tracker.status, chunks_done = existing_tracker.chunks_transferred, "既存の転送への再開リクエストを受理");
                
                // 既存のtrackerの状態をリセットまたは調整する必要があるか検討
                // ここでは単純に現在のチャンク数を返す
                // Failed の場合、エラー状態をリセットして Transferring に戻すか、
                // またはクライアントがそれを理解して再開するか。
                // ここでは Failed のままでも chunks_transferred を返すことで、
                // 送信側がそこから再試行することを期待する。
                // もし Failed 状態でも再開を許可する場合、ステータスを Transferring に戻すことを検討
                // existing_tracker.status = TransferStatus::Transferring; // 例: 状態をリセット
                // existing_tracker.error = None;

                return TransferResponse {
                    transfer_id: transfer_id.to_string(),
                    accepted: true,
                    resume_from: Some(existing_tracker.chunks_transferred),
                    error_message: None,
                };
            } else if matches!(existing_tracker.status, TransferStatus::Preparing | TransferStatus::Transferring | TransferStatus::Paused) {
                warn!(%transfer_id, "既にアクティブな転送IDです ({:?})。重複リクエストとして処理します。", existing_tracker.status);
                return TransferResponse {
                    transfer_id: transfer_id.to_string(),
                    accepted: false,
                    resume_from: None, 
                    error_message: Some("指定された転送IDは既にアクティブ（かつ非再開リクエストまたは非再開可能状態）です。".to_string()),
                };
            } else {
                info!(%transfer_id, "以前の転送 ({:?}) は終了済み。新しい転送として処理します。", existing_tracker.status);
                // 完了・失敗・キャンセル済みの場合は上書き、または新しいIDを強制するなどのポリシーがありうる
                // ここでは、古いtrackerを削除して新しいリクエストを受け入れる
                // existing_tracker_arc を使うのではなく、mapから削除する
                drop(existing_tracker); // write lock を解放
                active_transfers_map.remove(&transfer_id);
                // active_transfers_map のロックはこの後の処理で再度取得されるか、ここで抜ける
            }
        }
        // active_transfers_map のロックがここまで続いている場合があるため、必要なら再取得の形にする
        // drop(active_transfers_map); // 一旦解放
        // let mut active_transfers_map = self.active_transfers.write().await; // 再度取得

        // TODO: リソース状況の確認、ポリシーによる受け入れ可否判断など
        let accepted = true; 
        // let resume_from = None; // 新規転送なので resume_from は None

        if accepted {
            let tracker = Arc::new(RwLock::new(TransferTracker::new(
                transfer_id.clone(),
                request.metadata.clone(),
                NodeId::from_string(request.metadata.source_node.clone()),
                self.local_node_id.clone(),
                None, 
            )));
            
            active_transfers_map.insert(transfer_id.clone(), tracker);
            
            TransferResponse {
                transfer_id: transfer_id.to_string(),
                accepted: true,
                resume_from: None, // 新規転送なので None
                error_message: None,
            }
        } else {
            TransferResponse {
                transfer_id: transfer_id.to_string(),
                accepted: false,
                resume_from: None,
                error_message: Some("転送が（ポリシーにより）拒否されました。".to_string()),
            }
        }
    }
    
    /// データチャンクを処理 (主に受信側で呼び出される)
    pub async fn handle_data_chunk(&self, chunk: DataChunk) -> Result<()> {
        let transfer_id = TransferId::from_string(chunk.transfer_id.clone());
        trace!(transfer_id = %transfer_id, chunk_index = chunk.index, "データチャンク受信");

        if !chunk.verify_checksum() {
            warn!(transfer_id = %transfer_id, chunk_index = chunk.index, "チェックサム検証失敗");
            // 送信元にエラー通知または再送要求を行うのが理想的だが、ここではエラーを返すのみ
            return Err(anyhow!("チェックサム検証失敗: 転送ID {}, チャンク {}", transfer_id, chunk.index));
        }

        // active_transfers を読み取り、転送がまだアクティブか確認
        // (キャンセル/失敗/完了済みの場合、チャンク処理は不要)
        let tracker_status = {
            let active_transfers_map = self.active_transfers.read().await;
            if let Some(tracker_arc) = active_transfers_map.get(&transfer_id) {
                let tracker = tracker_arc.read().await;
                tracker.status
            } else {
                warn!(%transfer_id, chunk_index = chunk.index, "対応するアクティブな転送が見つかりません (trackerなし)。チャンクを破棄します。");
                return Ok(()); // tracker がなければ処理しようがない
            }
        };

        if !matches!(tracker_status, TransferStatus::Transferring | TransferStatus::Preparing) {
            warn!(%transfer_id, chunk_index = chunk.index, status = ?tracker_status, "転送がアクティブでないためチャンクを無視します。");
            return Ok(());
        }

        let pending_sinks_map = self.pending_sinks.read().await;
        if let Some(sender) = pending_sinks_map.get(&transfer_id) {
            if let Err(e) = sender.send(chunk).await {
                error!(transfer_id = %transfer_id, "シンクへのチャンク送信に失敗 (チャネルクローズの可能性): {}", e);
                // チャネルが閉じている = receive_transfer 側のループが終了している可能性
                // receive_transfer 側でtrackerの状態更新とクリーンアップが行われるはず
                // ここでエラーを返すと、CommunicationManagerがエラーをログするかもしれない
                return Err(anyhow!("シンクへのチャンク送信失敗 (チャネルクローズの可能性): {} ({})", transfer_id, e));
            }
        } else {
            warn!(transfer_id = %transfer_id, chunk_index = chunk.index, "対応するペンディングシンクが見つかりません。転送がセットアップされていないか、既に終了している可能性があります。チャンクを破棄。");
            // receive_transfer がまだ pending_sinks に登録していないか、既に削除した後かもしれない
            // tracker の状態が Transferring であれば、これは一時的なタイミングの問題か、エラー
        }
        Ok(())
    }

    /// 転送完了通知を処理 (主に送信側がリモートからの通知を受信した際に呼び出される)
    pub async fn handle_transfer_completion(&self, completion: TransferCompletion) -> Result<()> {
        let transfer_id = TransferId::from_string(completion.transfer_id.clone());
        info!(transfer_id = %transfer_id, success = completion.success, "転送完了通知を受信 (リモートから)");

        let mut active_transfers_map = self.active_transfers.write().await;
        if let Some(tracker_arc) = active_transfers_map.get(&transfer_id) {
            let mut tracker = tracker_arc.write().await;

            // 既にローカルで完了/失敗している場合は、状態を上書きしないことが多いが、
            // リモートの状態を最終として扱うか、ローカルの状態を優先するかはポリシーによる。
            // ここでは、まだ進行中(Preparing/Transferring)の場合のみリモートの状態を反映する。
            if matches!(tracker.status, TransferStatus::Preparing | TransferStatus::Transferring) {
                if completion.success {
                    tracker.status = TransferStatus::Completed;
                    // リモートから受信したチャンク数が信頼できる情報源である場合、更新も検討
                    // tracker.chunks_transferred = completion.chunks_transferred.max(tracker.chunks_transferred);
                } else {
                    tracker.status = TransferStatus::Failed;
                    tracker.error = completion.error.clone().or_else(|| Some("リモートから不明なエラー通知".to_string()));
                }
                tracker.last_update = Instant::now();
                debug!(transfer_id = %transfer_id, new_status = ?tracker.status, "転送トラッカーを更新 (リモート通知による)");
                
                // send_data側で待機している処理に結果を通知
                let result_for_notifier = if tracker.status == TransferStatus::Completed { 
                    Ok(())
                } else { 
                    Err(tracker.error.clone().unwrap_or_else(|| "不明なエラー".to_string())) 
                };
                // notify_completion は tracker のロック中に呼び出す
                tracker.notify_completion(result_for_notifier);

                // 転送が完了または失敗したら、アクティブな転送リストから削除
                // send_data 側では rx_completion の結果を見て削除するので、二重削除にならないように注意
                // ここで削除するのは、send_data が既にタイムアウト等で終了し、この通知が後から来た場合など
                // ただし、notify_completionが呼ばれた時点でsend_data側のrx_completionは消費されるので、
                // send_data側での削除に任せるのが一貫性がある。
                // ここでは削除しないでおく。send_dataのrx_completionハンドリングに委ねる。
                // active_transfers_map.remove(&transfer_id); 

            } else {
                info!(%transfer_id, current_status = ?tracker.status, remote_success = completion.success, "ローカルで既に転送が終了状態のため、リモートからの完了通知は情報として記録するのみ。");
                // 例えば、ローカルでタイムアウトしたがリモートは成功していた、などのケースのログ
                if completion.success && tracker.status != TransferStatus::Completed {
                    warn!(%transfer_id, "リモートは成功通知だがローカルは {:?} 状態", tracker.status);
                } else if !completion.success && tracker.status == TransferStatus::Completed {
                    warn!(%transfer_id, "リモートは失敗通知 ({:?}) だがローカルは Completed 状態", completion.error);
                }
            }
        } else {
            warn!(transfer_id = %transfer_id, "完了通知に対応するアクティブな転送が見つかりません (リモートからの通知)");
        }
        Ok(())
    }

    pub async fn receive_transfer<S: DataSink + 'static>(
        &self,
        expected_transfer_id: TransferId,
        mut sink: S, // DataSink の所有権を取り、mut で操作可能にする
    ) -> Result<()> {
        info!(transfer_id = %expected_transfer_id, "データ受信処理を開始");

        // 完了通知用のチャネル (receive_transfer自身の完了を通知するものではない)
        // trackerに紐づけて、外部の監視などが利用することを想定。ここではrxは使わない。
        let (tx_tracker_completion, _rx_tracker_completion) = oneshot::channel::<Result<(), String>>();

        let tracker_arc = {
            let mut active_transfers_map = self.active_transfers.write().await;
            if let Some(existing_tracker_arc) = active_transfers_map.get_mut(&expected_transfer_id) {
                let mut tracker_w = existing_tracker_arc.write().await;
                if matches!(tracker_w.status, TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled) {
                    warn!(transfer_id = %expected_transfer_id, status = ?tracker_w.status, "受信開始試行時点で転送は既に終了しています。");
                    return if tracker_w.status == TransferStatus::Completed { Ok(()) } else { Err(anyhow!("転送は既に {:?} ({:?})", tracker_w.status, tracker_w.error)) };
                }
                tracker_w.status = TransferStatus::Transferring;
                tracker_w.last_update = Instant::now();
                // trackerに完了通知用Senderをセット (既にセットされていれば上書き)
                tracker_w.completion_notifier = Some(tx_tracker_completion);
                Arc::clone(existing_tracker_arc)
            } else {
                // handle_transfer_request が先に呼ばれ、tracker が存在しているはず
                error!(transfer_id = %expected_transfer_id, "受信処理を開始しようとしましたが、アクティブな転送情報が見つかりません。");
                return Err(anyhow!("転送 {} の情報が見つかりません。handle_transfer_requestが事前に処理されている必要があります。", expected_transfer_id));
            }
        };
        
        let source_node_id = tracker_arc.read().await.source_node.clone(); // Read lock で十分
        let transfer_metadata_chunk_count = tracker_arc.read().await.metadata.chunk_count; 

        let (tx_chunk_to_sink, mut rx_chunk_from_handler) = mpsc::channel::<DataChunk>( (self.chunk_size / 1024).max(1) as usize );
        self.pending_sinks.write().await.insert(expected_transfer_id.clone(), tx_chunk_to_sink);
        
        debug!(transfer_id = %expected_transfer_id, "シンクを登録し、チャンク受信待機開始");
        let comm_manager_clone = Arc::clone(&self.comm_manager);
        let self_timeout_duration = self.timeout_duration; // ループ内で使うためコピー

        let mut received_chunks_count = 0u32;
        let mut received_all_expected_chunks = false;

        let final_result: Result<()> = loop {
            let current_tracker_status = tracker_arc.read().await.status; // ループの最初にステータス確認
            if matches!(current_tracker_status, TransferStatus::Cancelled | TransferStatus::Failed) {
                warn!(transfer_id = %expected_transfer_id, status = ?current_tracker_status, "転送がキャンセルまたは失敗しました。受信処理を中断します。");
                let err_msg = tracker_arc.read().await.error.clone().unwrap_or_else(|| format!("転送は {:?} 状態です", current_tracker_status));
                break Err(anyhow!(err_msg));
            }
            if received_all_expected_chunks {
                 debug!(%expected_transfer_id, "全期待チャンク受信済みのためループを終了");
                 break Ok(());
            }

            match timeout(self_timeout_duration, rx_chunk_from_handler.recv()).await {
                Ok(Some(chunk)) => { 
                    if TransferId::from_string(chunk.transfer_id.clone()) != expected_transfer_id {
                        warn!(received_id = %chunk.transfer_id, expected_id = %expected_transfer_id, "期待しない転送IDのチャンクを受信、無視します。");
                        continue;
                    }
                    // チェックサムはhandle_data_chunkで検証済みと仮定
                    tracker_arc.write().await.last_update = Instant::now();
                    trace!(transfer_id = %expected_transfer_id, index = chunk.index, "チャンクをシンクに書き込み");

                    if let Err(e) = sink.write_chunk(chunk.clone()).await {
                        error!(transfer_id = %expected_transfer_id, index = chunk.index, error = %e, "シンクへのチャンク書き込み失敗");
                        let mut tracker_w = tracker_arc.write().await;
                        tracker_w.status = TransferStatus::Failed;
                        tracker_w.error = Some(format!("シンク書き込みエラー: {}", e));
                        break Err(anyhow!(tracker_w.error.clone().unwrap()));
                    }
                    received_chunks_count += 1;
                    tracker_arc.write().await.bytes_transferred += chunk.data.len() as u64;

                    // chunk.total_chunks または transfer_metadata_chunk_count を信頼
                    if received_chunks_count >= chunk.total_chunks.max(transfer_metadata_chunk_count) { 
                        info!(transfer_id = %expected_transfer_id, chunks_received = received_chunks_count, "全ての期待されるチャンクを受信完了");
                        received_all_expected_chunks = true;
                    }
                }
                Ok(None) => { 
                    warn!(transfer_id = %expected_transfer_id, "チャンク受信チャネルがクローズされました (handle_data_chunk側で問題発生の可能性あり)。");
                    let mut tracker_w = tracker_arc.write().await;
                    if matches!(tracker_w.status, TransferStatus::Transferring | TransferStatus::Preparing) {
                        tracker_w.status = TransferStatus::Failed;
                        tracker_w.error = Some("チャンク受信チャネルが予期せずクローズ".to_string());
                    }
                    break Err(anyhow!(tracker_w.error.clone().unwrap_or_else(|| "チャネルクローズによる不明なエラー".to_string())));
                }
                Err(_) => { 
                    warn!(transfer_id = %expected_transfer_id, "チャンク受信タイムアウト");
                    let mut tracker_w = tracker_arc.write().await;
                    tracker_w.status = TransferStatus::Failed;
                    tracker_w.error = Some("チャンク受信タイムアウト".to_string());
                    break Err(anyhow!(tracker_w.error.clone().unwrap()));
                }
            }
        };

        self.pending_sinks.write().await.remove(&expected_transfer_id);
        let mut tracker_w = tracker_arc.write().await; // 最終処理のために再度ロック

        match final_result {
            Ok(_) => { 
                if matches!(tracker_w.status, TransferStatus::Transferring | TransferStatus::Completed ) { // Completed は既に外部で設定された場合
                    if tracker_w.status == TransferStatus::Transferring { // 通常はこちら
                        if let Err(e) = sink.complete().await {
                            error!(transfer_id = %expected_transfer_id, error = %e, "シンクの完了処理に失敗");
                            tracker_w.status = TransferStatus::Failed;
                            tracker_w.error = Some(format!("シンク完了エラー: {}", e));
                        } else {
                            info!(transfer_id = %expected_transfer_id, "転送成功裏に完了 (受信側)");
                            tracker_w.status = TransferStatus::Completed;
                        }
                    }
                } else if tracker_w.status == TransferStatus::Preparing {
                     warn!(%expected_transfer_id, "全てのチャンクを受信したが、TrackerがまだPreparing状態でした。念のためFailedとします。");
                     tracker_w.status = TransferStatus::Failed;
                     tracker_w.error = Some("不正な状態遷移: Preparingで全チャンク受信".to_string());
                }
                // Cancelled or Failed の場合は final_result が Err になっているはず
            }
            Err(ref e) => { 
                error!(transfer_id = %expected_transfer_id, error = %e, "受信処理中にエラー発生、または既にエラー状態でした。");
                 if matches!(tracker_w.status, TransferStatus::Transferring | TransferStatus::Preparing) {
                    tracker_w.status = TransferStatus::Failed;
                    if tracker_w.error.is_none() { 
                        tracker_w.error = Some(format!("受信エラー: {}", e));
                    }
                 }
                // シンクの中止処理 (冪等性を期待)
                if let Err(abort_err) = sink.abort().await {
                    error!(%expected_transfer_id, "シンクの中止処理にも失敗: {}", abort_err);
                }
            }
        }
        
        tracker_w.chunks_transferred = received_chunks_count; // 最終的な受信数を反映
        tracker_w.last_update = Instant::now();

        let completion_msg = TransferCompletion {
            transfer_id: expected_transfer_id.to_string(),
            success: tracker_w.status == TransferStatus::Completed,
            chunks_transferred: tracker_w.chunks_transferred,
            error: tracker_w.error.clone(),
            result: None, 
        };

        // 送信元に最終結果を通知 (新しいタスクで)
        let transfer_id_for_send = expected_transfer_id.clone();
        tokio::spawn(async move {
            if let Err(send_err) = comm_manager_clone.send_message(
                source_node_id, // captured from above
                MessageType::TransferCompletion,
                &completion_msg,
            ).await {
                error!(transfer_id = %transfer_id_for_send, "最終的な転送完了/失敗通知の送信元への送信に失敗: {}", send_err);
            }
        });

        let final_status_is_ok = tracker_w.status == TransferStatus::Completed;
        let notify_result = if final_status_is_ok { Ok(()) } else { Err(tracker_w.error.clone().unwrap_or_else(||"受信側で不明なエラー".to_string()))};
        
        // tracker に設定された oneshot チャネルで通知 (もしあれば)
        // ただし、この receive_transfer の呼び出し元が直接 Result を受け取るので、主要な通知手段ではない
        tracker_w.notify_completion(notify_result.clone()); 

        if tracker_w.status == TransferStatus::Completed || tracker_w.status == TransferStatus::Failed || tracker_w.status == TransferStatus::Cancelled {
            debug!(%expected_transfer_id, status = ?tracker_w.status, "受信処理完了/失敗/キャンセルにつきアクティブ転送から削除");
            // tracker_w のロックがここで解放される前に remove する必要があるため、先にIDをクローン
            let id_to_remove = tracker_w.id.clone();
            drop(tracker_w); // 明示的にロックを解放
            self.active_transfers.write().await.remove(&id_to_remove);
        } else {
             warn!(%expected_transfer_id, status = ?tracker_w.status, "receive_transfer終了時、予期せぬトラッカー状態のためアクティブリストに残存");
        }

        if final_status_is_ok {
            Ok(())
        } else {
            Err(anyhow!("転送 {} は {:?} で終了しました: {:?}", expected_transfer_id, tracker_arc.read().await.status, tracker_arc.read().await.error))
        }
    }

    /// 孤立した転送情報をクリーンアップ (Cancelled状態で長時間経過したもの)
    async fn cleanup_isolated_transfers(self: Arc<Self>) {
        let now = Instant::now();
        // クリーンアップ対象のTransferIdを収集
        let mut ids_to_remove = Vec::new();
        
        let active_transfers_map = self.active_transfers.read().await;
        for (id, tracker_arc) in active_transfers_map.iter() {
            let tracker = tracker_arc.read().await;
            if tracker.status == TransferStatus::Cancelled {
                // cleanup_interval の2倍の時間更新がなければ孤立とみなす (調整可能)
                if now.duration_since(tracker.last_update) > self.cleanup_interval.saturating_mul(2) {
                    warn!(transfer_id = %id, last_update = ?tracker.last_update, "孤立したキャンセル済み転送を検出、クリーンアップ対象とします。");
                    ids_to_remove.push(id.clone());
                }
            }
        }
        drop(active_transfers_map); // Readロックを早期に解放

        if !ids_to_remove.is_empty() {
            let mut active_transfers_map_w = self.active_transfers.write().await;
            let mut pending_sinks_map_w = self.pending_sinks.write().await;
            for id in ids_to_remove {
                if active_transfers_map_w.remove(&id).is_some() {
                    info!(transfer_id = %id, "アクティブ転送リストから孤立したキャンセル済み転送を削除しました。");
                }
                // 対応するペンディングシンクもあれば削除
                if pending_sinks_map_w.remove(&id).is_some() {
                    debug!(transfer_id = %id, "ペンディングシンクから孤立したキャンセル済み転送に関連する情報を削除しました。");
                }
            }
        }
    }
    
    /// クリーンアップタスクを起動します。
    /// このメソッドは DataTransferManager が Arc でラップされた後に呼び出されることを想定しています。
    pub fn spawn_cleanup_task(self: Arc<Self>) {
        info!("データ転送マネージャーの孤立転送クリーンアップタスクを起動します。間隔: {:?}", self.cleanup_interval);
        let manager_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(manager_clone.cleanup_interval);
            loop {
                interval.tick().await; // 次の実行時間まで待機
                debug!("孤立した転送情報の定期クリーンアップ処理を実行します。");
                let cleanup_fut = manager_clone.clone().cleanup_isolated_transfers();
                if let Err(e) = tokio::time::timeout(manager_clone.cleanup_interval.saturating_mul(2), cleanup_fut).await {
                    error!("孤立転送クリーンアップ処理がタイムアウトまたはエラー: {:?}", e);
                } else {
                    trace!("孤立転送クリーンアップ処理が正常に完了。");
                }
            }
        });
    }
}

/// 必要なuseステートメント
use std::collections::HashSet;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_transfer_id() {
        let id = TransferId::new();
        assert!(!id.as_str().is_empty());
        
        let id2 = TransferId::from_string("test-id".to_string());
        assert_eq!(id2.as_str(), "test-id");
    }
    
    #[test]
    fn test_data_chunk() {
        let data = vec![1, 2, 3, 4, 5];
        let chunk = DataChunk::new(
            "test-transfer".to_string(),
            0,
            1,
            data.clone(),
        );
        
        assert_eq!(chunk.transfer_id, "test-transfer");
        assert_eq!(chunk.index, 0);
        assert_eq!(chunk.total_chunks, 1);
        assert_eq!(chunk.data, data);
        assert!(chunk.verify_checksum());
    }
    
    #[tokio::test]
    async fn test_memory_data_source() {
        let data = vec![1, 2, 3, 4, 5];
        let mut source = MemoryDataSource::new(data.clone());
        
        let size = source.get_size().await.unwrap();
        assert_eq!(size, 5);
        
        let chunk = source.read_chunk(0, 3).await.unwrap();
        assert_eq!(chunk, vec![1, 2, 3]);
        
        let chunk2 = source.read_chunk(1, 3).await.unwrap();
        assert_eq!(chunk2, vec![4, 5]);
        
        // これ以上読み取れないはず
        assert!(source.read_chunk(2, 3).await.is_err());
    }
    
    #[tokio::test]
    async fn test_memory_data_sink() {
        let transfer_id = "test-transfer".to_string();
        let mut sink = MemoryDataSink::new(transfer_id.clone());
        
        // チャンク1を書き込み
        let chunk1 = DataChunk::new(
            transfer_id.clone(),
            0,
            2,
            vec![1, 2, 3],
        );
        sink.write_chunk(chunk1).await.unwrap();
        
        // まだすべてのチャンクが揃っていないので結合できない
        assert!(sink.combined_data.is_none());
        
        // チャンク2を書き込み
        let chunk2 = DataChunk::new(
            transfer_id.clone(),
            1,
            2,
            vec![4, 5],
        );
        sink.write_chunk(chunk2).await.unwrap();
        
        // 完了
        sink.complete().await.unwrap();
        
        // データが結合されているはず
        let combined = sink.get_data().unwrap();
        assert_eq!(combined, &vec![1, 2, 3, 4, 5]);
    }
} 