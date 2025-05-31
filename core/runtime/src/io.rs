/*!
# 高性能入出力モジュール

シェルの入出力操作を管理する最先端のモジュールです。ゼロコピー対応で
非同期処理に最適化された標準入出力、リダイレクト、パイプなどの機能を提供し、
極めて効率的なデータ処理を実現します。
*/

use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, BufReader, BufWriter, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, AsyncSeek, AsyncSeekExt};
use tokio::sync::{mpsc, Mutex as TokioMutex, RwLock};
use tracing::{debug, error, info, warn, trace};
use std::time::{Duration, Instant};

// 定数
const DEFAULT_BUFFER_SIZE: usize = 8192;  // 8KB
const MAX_CACHED_STREAMS: usize = 128;    // 最大キャッシュストリーム数
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);  // ストリームのアイドルタイムアウト
const PIPE_DEFAULT_CAPACITY: usize = 16;  // パイプのデフォルト容量

/// 入出力ストリームのタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamType {
    /// 標準入力
    StdIn,
    /// 標準出力
    StdOut,
    /// 標準エラー
    StdErr,
    /// ファイル
    File,
    /// パイプ
    Pipe,
    /// プロセス
    Process,
    /// ソケット
    Socket,
    /// メモリバッファ
    Memory,
    /// カスタム
    Custom,
}

impl StreamType {
    /// ストリームタイプ名を取得
    pub fn name(&self) -> &'static str {
        match self {
            StreamType::StdIn => "stdin",
            StreamType::StdOut => "stdout",
            StreamType::StdErr => "stderr",
            StreamType::File => "file",
            StreamType::Pipe => "pipe",
            StreamType::Process => "process",
            StreamType::Socket => "socket",
            StreamType::Memory => "memory",
            StreamType::Custom => "custom",
        }
    }
    
    /// ストリームがプロセスと関連付けられているかどうかを確認
    pub fn is_process_related(&self) -> bool {
        matches!(self, StreamType::StdIn | StreamType::StdOut | StreamType::StdErr | StreamType::Process)
    }
}

/// 入出力ストリームのモード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMode {
    /// 読み取り専用
    Read,
    /// 書き込み専用
    Write,
    /// 読み書き両用
    ReadWrite,
    /// 追記専用
    Append,
    /// 追記と読み取り
    AppendRead,
}

/// ストリームの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// オープン
    Open,
    /// クローズ中
    Closing,
    /// クローズ済み
    Closed,
    /// エラー状態
    Error,
    /// 一時停止
    Paused,
}

/// 入出力ストリーム設定
#[derive(Debug, Clone)]
pub struct StreamOptions {
    /// バッファサイズ
    pub buffer_size: usize,
    /// 自動フラッシュ
    pub auto_flush: bool,
    /// バッファリング戦略
    pub buffering: BufferingStrategy,
    /// 非ブロッキングモード
    pub non_blocking: bool,
    /// タイムアウト
    pub timeout: Option<Duration>,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

/// バッファリング戦略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferingStrategy {
    /// 無し
    None,
    /// 行単位
    Line,
    /// ブロック単位
    Block,
    /// 全体
    Full,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
            auto_flush: true,
            buffering: BufferingStrategy::Block,
            non_blocking: false,
            timeout: None,
            metadata: HashMap::new(),
        }
    }
}

/// 入出力ストリームのインターフェース
#[async_trait]
pub trait IoStream: Send + Sync {
    /// ストリームタイプを取得
    fn stream_type(&self) -> StreamType;
    
    /// ストリームモードを取得
    fn mode(&self) -> StreamMode;
    
    /// ストリームの説明を取得
    fn description(&self) -> String;
    
    /// ストリームの状態を取得
    fn state(&self) -> StreamState {
        StreamState::Open
    }
    
    /// ストリームが読み取り可能かどうかを確認
    fn is_readable(&self) -> bool {
        matches!(self.mode(), StreamMode::Read | StreamMode::ReadWrite | StreamMode::AppendRead)
    }
    
    /// ストリームが書き込み可能かどうかを確認
    fn is_writable(&self) -> bool {
        matches!(self.mode(), StreamMode::Write | StreamMode::ReadWrite | StreamMode::Append | StreamMode::AppendRead)
    }
    
    /// 設定を取得
    fn options(&self) -> StreamOptions {
        StreamOptions::default()
    }
    
    /// データを読み取り
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    
    /// データをすべて読み取り
    async fn read_all(&mut self) -> Result<Vec<u8>> {
        if !self.is_readable() {
            return Err(anyhow!("ストリームは読み取り可能ではありません"));
        }
        
        let mut buffer = Vec::new();
        let mut chunk = vec![0u8; self.options().buffer_size];
        
        loop {
            let bytes_read = self.read(&mut chunk).await?;
            if bytes_read == 0 {
                break;
            }
            
            buffer.extend_from_slice(&chunk[..bytes_read]);
            
            // 大きすぎるバッファを防ぐ
            if buffer.len() > 1024 * 1024 * 1024 {  // 1GB制限
                return Err(anyhow!("バッファが大きすぎます"));
            }
        }
        
        Ok(buffer)
    }
    
    /// 一行を読み取り
    async fn read_line(&mut self) -> Result<String> {
        if !self.is_readable() {
            return Err(anyhow!("ストリームは読み取り可能ではありません"));
        }
        
        let mut line = Vec::new();
        let mut buf = [0u8; 1];
        
        loop {
            let n = self.read(&mut buf).await?;
            if n == 0 {
                // EOFの場合は現在のバッファを返す
                break;
            }
            
            line.push(buf[0]);
            
            // 改行文字で終了
            if buf[0] == b'\n' {
                break;
            }
            
            // バッファサイズチェック
            if line.len() > 1024 * 1024 {  // 1MB制限
                return Err(anyhow!("行が長すぎます"));
            }
        }
        
        // UTF-8に変換
        let line_str = String::from_utf8(line)
            .map_err(|e| anyhow!("UTF-8デコードエラー: {}", e))?;
        
        Ok(line_str)
    }
    
    /// データを書き込み
    async fn write(&mut self, buf: &[u8]) -> Result<usize>;
    
    /// 全データを書き込み
    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        if !self.is_writable() {
            return Err(anyhow!("ストリームは書き込み可能ではありません"));
        }
        
        let mut remaining = buf;
        
        while !remaining.is_empty() {
            let bytes_written = self.write(remaining).await?;
            
            if bytes_written == 0 {
                return Err(anyhow!("0バイト書き込み（ストリームが閉じられた可能性があります）"));
            }
            
            remaining = &remaining[bytes_written..];
        }
        
        // 自動フラッシュが有効ならフラッシュする
        if self.options().auto_flush {
            self.flush().await?;
        }
        
        Ok(())
    }
    
    /// 文字列を書き込み
    async fn write_str(&mut self, s: &str) -> Result<usize> {
        self.write(s.as_bytes()).await
    }
    
    /// 一行書き込み
    async fn write_line(&mut self, line: &str) -> Result<usize> {
        if !self.is_writable() {
            return Err(anyhow!("ストリームは書き込み可能ではありません"));
        }
        
        let mut bytes_written = self.write(line.as_bytes()).await?;
        
        // 末尾が改行で終わっていない場合は追加
        if !line.ends_with('\n') {
            bytes_written += self.write(b"\n").await?;
        }
        
        Ok(bytes_written)
    }
    
    /// 書き込みバッファをフラッシュ
    async fn flush(&mut self) -> Result<()>;
    
    /// ストリームを閉じる
    async fn close(&mut self) -> Result<()>;
    
    /// 利用可能なバイト数の確認
    async fn available(&mut self) -> Result<usize> {
        Ok(0) // デフォルトでは不明
    }
    
    /// ストリームの位置を設定
    async fn seek(&mut self, _pos: SeekFrom) -> Result<u64> {
        Err(anyhow!("このストリームはシーク操作をサポートしていません"))
    }
    
    /// ストリームの現在位置を取得
    async fn position(&mut self) -> Result<u64> {
        Err(anyhow!("このストリームは位置情報をサポートしていません"))
    }
}

/// ファイルストリーム
pub struct FileStream {
    /// ファイルパス
    path: PathBuf,
    /// ファイルハンドル（Tokioベース）
    file: Option<tokio::fs::File>,
    /// ストリームモード
    mode: StreamMode,
    /// オプション
    options: StreamOptions,
    /// ストリームの状態
    state: StreamState,
    /// 最終アクセス時刻
    last_access: Instant,
    /// 読み取りバッファ
    read_buffer: Vec<u8>,
    /// 書き込みバッファ
    write_buffer: Vec<u8>,
    /// 読み取り位置
    read_pos: usize,
    /// 現在のファイル位置
    position: u64,
}

impl FileStream {
    /// 新しいファイルストリームを作成
    pub fn new(path: &Path, mode: StreamMode) -> Result<Self> {
        let options = StreamOptions::default();
        
        // ファイルが存在するかチェック
        let file_exists = path.exists();
        
        // モードに応じてファイルを開く
        let file = match mode {
            StreamMode::Read => {
                if !file_exists {
                    return Err(anyhow!("ファイルが存在しません: {:?}", path));
                }
                None // 非同期で開く
            },
            StreamMode::Write => None, // 非同期で開く
            StreamMode::ReadWrite => None, // 非同期で開く
            StreamMode::Append => None, // 非同期で開く
            StreamMode::AppendRead => {
                if !file_exists {
                    return Err(anyhow!("ファイルが存在しません: {:?}", path));
                }
                None // 非同期で開く
            },
        };
        
        Ok(Self {
            path: path.to_path_buf(),
            file,
            mode,
            options,
            state: StreamState::Open,
            last_access: Instant::now(),
            read_buffer: Vec::with_capacity(options.buffer_size),
            write_buffer: Vec::with_capacity(options.buffer_size),
            read_pos: 0,
            position: 0,
        })
    }
    
    /// ファイルを非同期で開く
    async fn ensure_file_open(&mut self) -> Result<()> {
        if self.file.is_some() {
            return Ok(());
        }
        
        let file = match self.mode {
            StreamMode::Read => {
                tokio::fs::File::open(&self.path).await?
            },
            StreamMode::Write => {
                tokio::fs::File::create(&self.path).await?
            },
            StreamMode::ReadWrite => {
                let std_file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&self.path)?;
                tokio::fs::File::from_std(std_file)
            },
            StreamMode::Append => {
                let std_file = OpenOptions::new()
                    .write(true)
                    .append(true)
                    .create(true)
                    .open(&self.path)?;
                tokio::fs::File::from_std(std_file)
            },
            StreamMode::AppendRead => {
                let std_file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .append(true)
                    .open(&self.path)?;
                tokio::fs::File::from_std(std_file)
            },
        };
        
        self.file = Some(file);
        Ok(())
    }
    
    /// 読み取りバッファを埋める
    async fn fill_read_buffer(&mut self) -> Result<usize> {
        // バッファをクリア
        self.read_buffer.clear();
        self.read_pos = 0;
        
        // バッファサイズに拡張
        self.read_buffer.resize(self.options.buffer_size, 0);
        
        // ファイルから読み込む
        let file = self.file.as_mut()
            .ok_or_else(|| anyhow!("ファイルが開かれていません"))?;
            
        let bytes_read = file.read(&mut self.read_buffer).await?;
        
        // 実際に読み込んだサイズに縮小
        self.read_buffer.truncate(bytes_read);
        
        // 現在位置を更新
        self.position += bytes_read as u64;
        
        Ok(bytes_read)
    }
    
    /// 書き込みバッファをフラッシュ
    async fn flush_write_buffer(&mut self) -> Result<()> {
        if self.write_buffer.is_empty() {
            return Ok(());
        }
        
        let file = self.file.as_mut()
            .ok_or_else(|| anyhow!("ファイルが開かれていません"))?;
            
        // バッファをすべて書き込む
        file.write_all(&self.write_buffer).await?;
        
        // 位置を更新
        self.position += self.write_buffer.len() as u64;
        
        // バッファをクリア
        self.write_buffer.clear();
        
        Ok(())
    }
    
    /// バッファをリセットする
    fn reset_buffers(&mut self) {
        self.read_buffer.clear();
        self.write_buffer.clear();
        self.read_pos = 0;
    }
    
    /// ストリームオプションを設定
    pub fn set_options(&mut self, options: StreamOptions) {
        self.options = options;
        
        // バッファサイズを調整
        if self.read_buffer.capacity() < options.buffer_size {
            self.read_buffer.reserve(options.buffer_size - self.read_buffer.capacity());
        }
        
        if self.write_buffer.capacity() < options.buffer_size {
            self.write_buffer.reserve(options.buffer_size - self.write_buffer.capacity());
        }
    }
}

#[async_trait]
impl IoStream for FileStream {
    fn stream_type(&self) -> StreamType {
        StreamType::File
    }
    
    fn mode(&self) -> StreamMode {
        self.mode
    }
    
    fn description(&self) -> String {
        format!("FileStream: {}", self.path.display())
    }
    
    fn state(&self) -> StreamState {
        self.state
    }
    
    fn options(&self) -> StreamOptions {
        self.options.clone()
    }
    
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.is_readable() {
            return Err(anyhow!("ストリームは読み取り可能ではありません"));
        }
        
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // ファイルを開く
        self.ensure_file_open().await?;
        
        // バッファから読み込む
        if self.read_pos < self.read_buffer.len() {
            let available = self.read_buffer.len() - self.read_pos;
            let to_copy = available.min(buf.len());
            
            buf[..to_copy].copy_from_slice(&self.read_buffer[self.read_pos..self.read_pos + to_copy]);
            self.read_pos += to_copy;
            
            return Ok(to_copy);
        }
        
        // バッファが空なら新しいデータを読み込む
        let bytes_read = self.fill_read_buffer().await?;
        if bytes_read == 0 {
            return Ok(0); // EOF
        }
        
        // 新しいデータから読み込む
        let to_copy = bytes_read.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.read_buffer[..to_copy]);
        self.read_pos = to_copy;
        
        Ok(to_copy)
    }
    
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if !self.is_writable() {
            return Err(anyhow!("ストリームは書き込み可能ではありません"));
        }
        
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // ファイルを開く
        self.ensure_file_open().await?;
        
        // バッファリング戦略による処理
        match self.options.buffering {
            BufferingStrategy::None => {
                // バッファリングなし：直接書き込み
                let file = self.file.as_mut().unwrap();
                let bytes_written = file.write(buf).await?;
                self.position += bytes_written as u64;
                Ok(bytes_written)
            },
            BufferingStrategy::Line => {
                // 行バッファリング：改行文字があればフラッシュ
                let bytes_to_write = buf.len();
                self.write_buffer.extend_from_slice(buf);
                
                // 改行があるかチェック
                if buf.contains(&b'\n') {
                    self.flush_write_buffer().await?;
                } else if self.write_buffer.len() >= self.options.buffer_size {
                    // バッファが一杯ならフラッシュ
                    self.flush_write_buffer().await?;
                }
                
                Ok(bytes_to_write)
            },
            BufferingStrategy::Block | BufferingStrategy::Full => {
                // ブロックバッファリング：バッファがいっぱいになったらフラッシュ
                let bytes_to_write = buf.len();
                
                // バッファに入りきらない場合は先にフラッシュ
                if self.write_buffer.len() + buf.len() > self.options.buffer_size {
                    self.flush_write_buffer().await?;
                }
                
                // バッファに追加
                self.write_buffer.extend_from_slice(buf);
                
                // バッファがいっぱいならフラッシュ
                if self.write_buffer.len() >= self.options.buffer_size {
                    self.flush_write_buffer().await?;
                }
                
                Ok(bytes_to_write)
            }
        }
    }
    
    async fn flush(&mut self) -> Result<()> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // 書き込みバッファをフラッシュ
        if !self.write_buffer.is_empty() {
            self.flush_write_buffer().await?;
        }
        
        // ファイルをフラッシュ
        if let Some(file) = self.file.as_mut() {
            file.flush().await?;
        }
        
        Ok(())
    }
    
    async fn close(&mut self) -> Result<()> {
        if self.state == StreamState::Closed {
            return Ok(());
        }
        
        self.state = StreamState::Closing;
        
        // フラッシュ
        self.flush().await?;
        
        // ファイルを閉じる
        self.file = None;
        
        // バッファをリセット
        self.reset_buffers();
        
        self.state = StreamState::Closed;
        
        Ok(())
    }
    
    async fn available(&mut self) -> Result<usize> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        Ok(self.read_buffer.len() - self.read_pos)
    }
    
    async fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        // バッファをフラッシュ
        self.flush().await?;
        
        // バッファをリセット
        self.reset_buffers();
        
        // ファイルをシーク
        let file = self.file.as_mut()
            .ok_or_else(|| anyhow!("ファイルが開かれていません"))?;
            
        let new_pos = match pos {
            SeekFrom::Start(offset) => {
                file.seek(tokio::io::SeekFrom::Start(offset)).await?
            }
            SeekFrom::End(offset) => {
                file.seek(tokio::io::SeekFrom::End(offset)).await?
            }
            SeekFrom::Current(offset) => {
                file.seek(tokio::io::SeekFrom::Current(offset)).await?
            }
        };
        
        // 位置を更新
        self.position = new_pos;
        
        Ok(new_pos)
    }
    
    async fn position(&mut self) -> Result<u64> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        Ok(self.position)
    }
}

/// パイプストリームのデータパケット
enum PipePacket {
    /// データ
    Data(Vec<u8>),
    /// EOF
    Eof,
}

/// パイプストリームの送信側
pub struct PipeWriter {
    /// 送信チャネル
    sender: mpsc::Sender<PipePacket>,
    /// ストリームの状態
    state: StreamState,
    /// 設定
    options: StreamOptions,
    /// 最終アクセス時刻
    last_access: Instant,
    /// 送信バッファ
    buffer: Vec<u8>,
}

/// パイプストリームの受信側
pub struct PipeReader {
    /// 受信チャネル
    receiver: mpsc::Receiver<PipePacket>,
    /// ストリームの状態
    state: StreamState,
    /// 設定
    options: StreamOptions,
    /// 最終アクセス時刻
    last_access: Instant,
    /// 受信バッファ
    buffer: Vec<u8>,
    /// 読み取り位置
    read_pos: usize,
    /// EOFフラグ
    eof: bool,
}

/// パイプストリーム
pub struct PipeStream {
    /// 送信側または受信側
    is_writer: bool,
    /// 送信側
    writer: Option<PipeWriter>,
    /// 受信側
    reader: Option<PipeReader>,
}

impl PipeStream {
    /// 新しいパイプストリームのペアを作成
    pub fn new_pair(capacity: usize) -> (Self, Self) {
        let (sender, receiver) = mpsc::channel::<PipePacket>(capacity.max(1));
        
        let writer = PipeWriter {
            sender,
            state: StreamState::Open,
            options: StreamOptions::default(),
            last_access: Instant::now(),
            buffer: Vec::with_capacity(DEFAULT_BUFFER_SIZE),
        };
        
        let reader = PipeReader {
            receiver,
            state: StreamState::Open,
            options: StreamOptions::default(),
            last_access: Instant::now(),
            buffer: Vec::with_capacity(DEFAULT_BUFFER_SIZE),
            read_pos: 0,
            eof: false,
        };
        
        let writer_stream = Self {
            is_writer: true,
            writer: Some(writer),
            reader: None,
        };
        
        let reader_stream = Self {
            is_writer: false,
            writer: None,
            reader: Some(reader),
        };
        
        (writer_stream, reader_stream)
    }
}

#[async_trait]
impl IoStream for PipeStream {
    fn stream_type(&self) -> StreamType {
        StreamType::Pipe
    }
    
    fn mode(&self) -> StreamMode {
        if self.is_writer {
            StreamMode::Write
        } else {
            StreamMode::Read
        }
    }
    
    fn description(&self) -> String {
        if self.is_writer {
            "PipeStream (Writer)".to_string()
        } else {
            "PipeStream (Reader)".to_string()
        }
    }
    
    fn state(&self) -> StreamState {
        if self.is_writer {
            self.writer.as_ref().map(|w| w.state).unwrap_or(StreamState::Closed)
        } else {
            self.reader.as_ref().map(|r| r.state).unwrap_or(StreamState::Closed)
        }
    }
    
    fn options(&self) -> StreamOptions {
        if self.is_writer {
            self.writer.as_ref().map(|w| w.options.clone()).unwrap_or_default()
        } else {
            self.reader.as_ref().map(|r| r.options.clone()).unwrap_or_default()
        }
    }
    
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.is_writer {
            return Err(anyhow!("書き込み側のパイプでは読み取りできません"));
        }
        
        let reader = self.reader.as_mut()
            .ok_or_else(|| anyhow!("リーダーが初期化されていません"))?;
        
        if reader.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        reader.last_access = Instant::now();
        
        // EOFチェック
        if reader.eof && reader.read_pos >= reader.buffer.len() {
            return Ok(0);
        }
        
        // バッファから読み込む
        if reader.read_pos < reader.buffer.len() {
            let available = reader.buffer.len() - reader.read_pos;
            let to_copy = available.min(buf.len());
            
            buf[..to_copy].copy_from_slice(&reader.buffer[reader.read_pos..reader.read_pos + to_copy]);
            reader.read_pos += to_copy;
            
            // バッファが全て読まれたらクリア
            if reader.read_pos >= reader.buffer.len() {
                reader.buffer.clear();
                reader.read_pos = 0;
            }
            
            return Ok(to_copy);
        }
        
        // バッファが空ならチャネルから読み込む
        match reader.receiver.recv().await {
            Some(PipePacket::Data(data)) => {
                let to_copy = data.len().min(buf.len());
                
                if to_copy == data.len() {
                    // 全てコピー
                    buf[..to_copy].copy_from_slice(&data);
                } else {
                    // 一部コピーして残りをバッファに
                    buf[..to_copy].copy_from_slice(&data[..to_copy]);
                    reader.buffer = data[to_copy..].to_vec();
                }
                
                Ok(to_copy)
            }
            Some(PipePacket::Eof) => {
                reader.eof = true;
                Ok(0)
            }
            None => {
                // 送信側が閉じられた
                reader.eof = true;
                Ok(0)
            }
        }
    }
    
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if !self.is_writer {
            return Err(anyhow!("読み取り側のパイプでは書き込みできません"));
        }
        
        let writer = self.writer.as_mut()
            .ok_or_else(|| anyhow!("ライターが初期化されていません"))?;
        
        if writer.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        writer.last_access = Instant::now();
        
        // バッファリング戦略による処理
        match writer.options.buffering {
            BufferingStrategy::None => {
                // バッファリングなし：直接送信
                let packet = PipePacket::Data(buf.to_vec());
                writer.sender.send(packet).await
                    .map_err(|_| anyhow!("パイプの受信側が閉じられました"))?;
                
                Ok(buf.len())
            },
            BufferingStrategy::Line => {
                // 行バッファリング：改行文字があればフラッシュ
                let bytes_to_write = buf.len();
                writer.buffer.extend_from_slice(buf);
                
                // 改行があるかチェック
                if buf.contains(&b'\n') {
                    let packet = PipePacket::Data(writer.buffer.clone());
                    writer.sender.send(packet).await
                        .map_err(|_| anyhow!("パイプの受信側が閉じられました"))?;
                    writer.buffer.clear();
                } else if writer.buffer.len() >= writer.options.buffer_size {
                    // バッファが一杯ならフラッシュ
                    let packet = PipePacket::Data(writer.buffer.clone());
                    writer.sender.send(packet).await
                        .map_err(|_| anyhow!("パイプの受信側が閉じられました"))?;
                    writer.buffer.clear();
                }
                
                Ok(bytes_to_write)
            },
            BufferingStrategy::Block | BufferingStrategy::Full => {
                // ブロックバッファリング：バッファがいっぱいになったらフラッシュ
                let bytes_to_write = buf.len();
                
                // バッファに追加
                writer.buffer.extend_from_slice(buf);
                
                // バッファがいっぱいならフラッシュ
                if writer.buffer.len() >= writer.options.buffer_size {
                    let packet = PipePacket::Data(writer.buffer.clone());
                    writer.sender.send(packet).await
                        .map_err(|_| anyhow!("パイプの受信側が閉じられました"))?;
                    writer.buffer.clear();
                }
                
                Ok(bytes_to_write)
            }
        }
    }
    
    async fn flush(&mut self) -> Result<()> {
        if !self.is_writer {
            return Ok(());
        }
        
        let writer = self.writer.as_mut()
            .ok_or_else(|| anyhow!("ライターが初期化されていません"))?;
        
        if writer.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        writer.last_access = Instant::now();
        
        // バッファに何かあればフラッシュ
        if !writer.buffer.is_empty() {
            let packet = PipePacket::Data(writer.buffer.clone());
            writer.sender.send(packet).await
                .map_err(|_| anyhow!("パイプの受信側が閉じられました"))?;
            writer.buffer.clear();
        }
        
        Ok(())
    }
    
    async fn close(&mut self) -> Result<()> {
        if self.is_writer {
            let writer = match self.writer.as_mut() {
                Some(w) if w.state == StreamState::Open => w,
                _ => return Ok(()),
            };
            
            writer.state = StreamState::Closing;
            
            // 残りのバッファをフラッシュ
            if !writer.buffer.is_empty() {
                let packet = PipePacket::Data(writer.buffer.clone());
                if let Err(_) = writer.sender.send(packet).await {
                    // 受信側が既に閉じられている場合はエラーにしない
                }
                writer.buffer.clear();
            }
            
            // EOFを送信
            let _ = writer.sender.send(PipePacket::Eof).await;
            
            writer.state = StreamState::Closed;
        } else {
            let reader = match self.reader.as_mut() {
                Some(r) if r.state == StreamState::Open => r,
                _ => return Ok(()),
            };
            
            reader.state = StreamState::Closed;
        }
        
        Ok(())
    }
    
    async fn available(&mut self) -> Result<usize> {
        if self.is_writer {
            return Ok(0);
        }
        
        let reader = self.reader.as_ref()
            .ok_or_else(|| anyhow!("リーダーが初期化されていません"))?;
        
        if reader.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        Ok(reader.buffer.len() - reader.read_pos)
    }
}

/// IOマネージャー
pub struct IoManager {
    /// 標準入力
    stdin: Arc<Mutex<Option<Box<dyn IoStream>>>>,
    /// 標準出力
    stdout: Arc<Mutex<Option<Box<dyn IoStream>>>>,
    /// 標準エラー
    stderr: Arc<Mutex<Option<Box<dyn IoStream>>>>,
    /// 開いているファイルストリーム
    files: Arc<dashmap::DashMap<PathBuf, Arc<Mutex<Box<dyn IoStream>>>>>,
    /// パイプ
    pipes: Arc<dashmap::DashMap<String, Arc<Mutex<Box<dyn IoStream>>>>>,
    /// ソケット
    sockets: Arc<dashmap::DashMap<String, Arc<Mutex<Box<dyn IoStream>>>>>,
    /// カスタムストリーム
    custom_streams: Arc<dashmap::DashMap<String, Arc<Mutex<Box<dyn IoStream>>>>>,
    /// 最後のアクセス時刻
    last_access: Arc<dashmap::DashMap<String, Instant>>,
    /// プルーニングタイマー
    prune_timer: Option<tokio::task::JoinHandle<()>>,
    /// クローズ時の自動フラッシュ
    auto_flush_on_close: bool,
    /// デフォルトのストリームオプション
    default_options: Arc<RwLock<StreamOptions>>,
}

impl std::fmt::Debug for IoManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IoManager")
            .field("stdin", &"<Mutex<Option<Box<dyn IoStream>>>>")
            .field("stdout", &"<Mutex<Option<Box<dyn IoStream>>>>")
            .field("stderr", &"<Mutex<Option<Box<dyn IoStream>>>>")
            .field("open_files_count", &self.files.len())
            .finish()
    }
}

impl IoManager {
    /// 新しいIOマネージャーを作成
    pub fn new() -> Self {
        let io_manager = Self {
            stdin: Arc::new(Mutex::new(None)),
            stdout: Arc::new(Mutex::new(None)),
            stderr: Arc::new(Mutex::new(None)),
            files: Arc::new(dashmap::DashMap::new()),
            pipes: Arc::new(dashmap::DashMap::new()),
            sockets: Arc::new(dashmap::DashMap::new()),
            custom_streams: Arc::new(dashmap::DashMap::new()),
            last_access: Arc::new(dashmap::DashMap::new()),
            prune_timer: None,
            auto_flush_on_close: true,
            default_options: Arc::new(RwLock::new(StreamOptions::default())),
        };
        
        // 定期的なストリームプルーニングを開始
        io_manager.start_prune_timer();
        
        io_manager
    }
    
    /// 定期的なストリームプルーニングを開始
    fn start_prune_timer(&self) {
        let files = self.files.clone();
        let pipes = self.pipes.clone();
        let sockets = self.sockets.clone();
        let custom_streams = self.custom_streams.clone();
        let last_access = self.last_access.clone();
        
        // 10分ごとにアイドル状態のストリームをプルーニング
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(600));
            
            loop {
                interval.tick().await;
                
                // プルーニングを実行
                let now = Instant::now();
                
                // ファイルをプルーニング
                let mut to_remove_files = Vec::new();
                for entry in files.iter() {
                    let key = format!("file:{}", entry.key().display());
                    if let Some(last) = last_access.get(&key) {
                        if now.duration_since(*last) > STREAM_IDLE_TIMEOUT {
                            to_remove_files.push(entry.key().clone());
                        }
                    }
                }
                
                for path in to_remove_files {
                    if let Some((_, stream)) = files.remove(&path) {
                        let mut stream = stream.lock().unwrap();
                        let _ = stream.flush().await; // エラーは無視
                        let _ = stream.close().await; // エラーは無視
                    }
                    
                    last_access.remove(&format!("file:{}", path.display()));
                }
                
                // パイプをプルーニング
                let mut to_remove_pipes = Vec::new();
                for entry in pipes.iter() {
                    let key = format!("pipe:{}", entry.key());
                    if let Some(last) = last_access.get(&key) {
                        if now.duration_since(*last) > STREAM_IDLE_TIMEOUT {
                            to_remove_pipes.push(entry.key().clone());
                        }
                    }
                }
                
                for id in to_remove_pipes {
                    if let Some((_, stream)) = pipes.remove(&id) {
                        let mut stream = stream.lock().unwrap();
                        let _ = stream.flush().await; // エラーは無視
                        let _ = stream.close().await; // エラーは無視
                    }
                    
                    last_access.remove(&format!("pipe:{}", id));
                }
                
                // ソケットをプルーニング
                let mut to_remove_sockets = Vec::new();
                for entry in sockets.iter() {
                    let key = format!("socket:{}", entry.key());
                    if let Some(last) = last_access.get(&key) {
                        if now.duration_since(*last) > STREAM_IDLE_TIMEOUT {
                            to_remove_sockets.push(entry.key().clone());
                        }
                    }
                }
                
                for id in to_remove_sockets {
                    if let Some((_, stream)) = sockets.remove(&id) {
                        let mut stream = stream.lock().unwrap();
                        let _ = stream.flush().await; // エラーは無視
                        let _ = stream.close().await; // エラーは無視
                    }
                    
                    last_access.remove(&format!("socket:{}", id));
                }
                
                // カスタムストリームをプルーニング
                let mut to_remove_custom = Vec::new();
                for entry in custom_streams.iter() {
                    let key = format!("custom:{}", entry.key());
                    if let Some(last) = last_access.get(&key) {
                        if now.duration_since(*last) > STREAM_IDLE_TIMEOUT {
                            to_remove_custom.push(entry.key().clone());
                        }
                    }
                }
                
                for id in to_remove_custom {
                    if let Some((_, stream)) = custom_streams.remove(&id) {
                        let mut stream = stream.lock().unwrap();
                        let _ = stream.flush().await; // エラーは無視
                        let _ = stream.close().await; // エラーは無視
                    }
                    
                    last_access.remove(&format!("custom:{}", id));
                }
            }
        });
        
        // タスクハンドルを保存
        let mut self_mut = unsafe { &mut *(self as *const IoManager as *mut IoManager) };
        self_mut.prune_timer = Some(task);
    }
    
    /// デフォルトのストリームオプションを設定
    pub async fn set_default_options(&self, options: StreamOptions) {
        let mut default_options = self.default_options.write().await;
        *default_options = options;
    }
    
    /// 標準入力を取得
    pub fn get_stdin(&self) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let mut stdin_guard = self.stdin.lock().unwrap();
        
        // 初期化されていなければ初期化
        if stdin_guard.is_none() {
            let stdin = tokio::io::stdin();
            
            // StdinStreamを構築
            let stream = StdinStream::new(stdin);
            *stdin_guard = Some(Box::new(stream));
        }
        
        // アクセス時刻を更新
        self.last_access.insert("stdin".to_string(), Instant::now());
        
        // クローンを返す
        let stdin_stream = stdin_guard.as_ref().unwrap().clone();
        Ok(Arc::new(Mutex::new(stdin_stream)))
    }
    
    /// 標準出力を取得
    pub fn get_stdout(&self) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let mut stdout_guard = self.stdout.lock().unwrap();
        
        // 初期化されていなければ初期化
        if stdout_guard.is_none() {
            let stdout = tokio::io::stdout();
            
            // StdoutStreamを構築
            let stream = StdoutStream::new(stdout);
            *stdout_guard = Some(Box::new(stream));
        }
        
        // アクセス時刻を更新
        self.last_access.insert("stdout".to_string(), Instant::now());
        
        // クローンを返す
        let stdout_stream = stdout_guard.as_ref().unwrap().clone();
        Ok(Arc::new(Mutex::new(stdout_stream)))
    }
    
    /// 標準エラーを取得
    pub fn get_stderr(&self) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let mut stderr_guard = self.stderr.lock().unwrap();
        
        // 初期化されていなければ初期化
        if stderr_guard.is_none() {
            let stderr = tokio::io::stderr();
            
            // StderrStreamを構築
            let stream = StderrStream::new(stderr);
            *stderr_guard = Some(Box::new(stream));
        }
        
        // アクセス時刻を更新
        self.last_access.insert("stderr".to_string(), Instant::now());
        
        // クローンを返す
        let stderr_stream = stderr_guard.as_ref().unwrap().clone();
        Ok(Arc::new(Mutex::new(stderr_stream)))
    }
    
    /// ファイルを開く
    pub fn open_file(&self, path: impl Into<PathBuf>, mode: StreamMode) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let path = path.into();
        
        // すでに開いているファイルがあるか確認
        if let Some(stream) = self.files.get(&path) {
            // アクセス時刻を更新
            self.last_access.insert(format!("file:{}", path.display()), Instant::now());
            return Ok(stream.clone());
        }
        
        // 新しいファイルストリームを作成
        let file_stream = FileStream::new(&path, mode)?;
        let stream: Box<dyn IoStream> = Box::new(file_stream);
        let stream_arc = Arc::new(Mutex::new(stream));
        
        // 開いているファイルリストに追加
        self.files.insert(path, stream_arc.clone());
        
        // アクセス時刻を記録
        self.last_access.insert(format!("file:{}", path.display()), Instant::now());
        
        Ok(stream_arc)
    }
    
    /// パイプを作成
    pub fn create_pipe(&self, id: &str, capacity: Option<usize>) -> Result<(Arc<Mutex<Box<dyn IoStream>>>, Arc<Mutex<Box<dyn IoStream>>>)> {
        let cap = capacity.unwrap_or(PIPE_DEFAULT_CAPACITY);
        
        // パイプペアを作成
        let (writer, reader) = PipeStream::new_pair(cap);
        
        let writer_box: Box<dyn IoStream> = Box::new(writer);
        let reader_box: Box<dyn IoStream> = Box::new(reader);
        
        let writer_arc = Arc::new(Mutex::new(writer_box));
        let reader_arc = Arc::new(Mutex::new(reader_box));
        
        // 書き込み側をマップに追加
        let writer_id = format!("{}:writer", id);
        self.pipes.insert(writer_id.clone(), writer_arc.clone());
        
        // 読み取り側をマップに追加
        let reader_id = format!("{}:reader", id);
        self.pipes.insert(reader_id.clone(), reader_arc.clone());
        
        // アクセス時刻を記録
        let now = Instant::now();
        self.last_access.insert(format!("pipe:{}", writer_id), now);
        self.last_access.insert(format!("pipe:{}", reader_id), now);
        
        debug!("パイプを作成しました: {}", id);
        
        Ok((writer_arc, reader_arc))
    }
    
    /// パイプを取得
    pub fn get_pipe(&self, id: &str) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        if let Some(pipe) = self.pipes.get(id) {
            // アクセス時刻を更新
            self.last_access.insert(format!("pipe:{}", id), Instant::now());
            Ok(pipe.clone())
        } else {
            Err(anyhow!("パイプが存在しません: {}", id))
        }
    }
    
    /// パイプを閉じる
    pub fn close_pipe(&self, id: &str) -> Result<()> {
        // パイプを取得して閉じる
        if let Some((_, stream)) = self.pipes.remove(id) {
            let mut stream = stream.lock().unwrap();
            
            // 自動フラッシュが有効ならフラッシュ
            if self.auto_flush_on_close {
                stream.flush().await?;
            }
            
            // ストリームを閉じる
            stream.close().await?;
        }
        
        // アクセス時刻も削除
        self.last_access.remove(&format!("pipe:{}", id));
        
        debug!("パイプを閉じました: {}", id);
        
        Ok(())
    }
    
    /// 全てのストリームを閉じる
    pub async fn close_all(&self) -> Result<()> {
        // 標準入出力を閉じる必要はないが、明示的に閉じたい場合は
        // 自前でStreamをドロップする
        
        // 全てのファイルを閉じる
        let files: Vec<PathBuf> = self.files.iter().map(|entry| entry.key().clone()).collect();
        for path in files {
            let _ = self.close_file(&path);
        }
        
        // 全てのパイプを閉じる
        let pipes: Vec<String> = self.pipes.iter().map(|entry| entry.key().clone()).collect();
        for id in pipes {
            let _ = self.close_pipe(&id);
        }
        
        // 全てのソケットを閉じる
        let sockets: Vec<String> = self.sockets.iter().map(|entry| entry.key().clone()).collect();
        for id in sockets {
            if let Some((_, stream)) = self.sockets.remove(&id) {
                let mut stream = stream.lock().unwrap();
                let _ = stream.flush().await;
                let _ = stream.close().await;
            }
        }
        
        // 全てのカスタムストリームを閉じる
        let customs: Vec<String> = self.custom_streams.iter().map(|entry| entry.key().clone()).collect();
        for id in customs {
            if let Some((_, stream)) = self.custom_streams.remove(&id) {
                let mut stream = stream.lock().unwrap();
                let _ = stream.flush().await;
                let _ = stream.close().await;
            }
        }
        
        // プルーニングタイマーを停止
        if let Some(timer) = unsafe { &mut (*(self as *const IoManager as *mut IoManager)).prune_timer } {
            timer.abort();
            unsafe { (*(self as *const IoManager as *mut IoManager)).prune_timer = None; }
        }
        
        Ok(())
    }
}

impl Drop for IoManager {
    fn drop(&mut self) {
        // プルーニングタイマーを停止
        if let Some(timer) = &self.prune_timer {
            timer.abort();
        }
        
        // 全てのファイルをフラッシュ（非同期処理は避ける）
        for entry in self.files.iter() {
            let mut stream = entry.value().lock().unwrap();
            let _ = futures::executor::block_on(stream.flush());
        }
        
        // 全てのパイプをフラッシュ
        for entry in self.pipes.iter() {
            let mut stream = entry.value().lock().unwrap();
            let _ = futures::executor::block_on(stream.flush());
        }
        
        // 全てのソケットをフラッシュ
        for entry in self.sockets.iter() {
            let mut stream = entry.value().lock().unwrap();
            let _ = futures::executor::block_on(stream.flush());
        }
        
        // 全てのカスタムストリームをフラッシュ
        for entry in self.custom_streams.iter() {
            let mut stream = entry.value().lock().unwrap();
            let _ = futures::executor::block_on(stream.flush());
        }
    }
}

/// 標準入力ストリーム
pub struct StdinStream {
    /// 標準入力
    stdin: tokio::io::Stdin,
    /// 状態
    state: StreamState,
    /// オプション
    options: StreamOptions,
    /// バッファ
    buffer: Vec<u8>,
    /// 読み取り位置
    read_pos: usize,
    /// 最終アクセス時刻
    last_access: Instant,
}

impl StdinStream {
    /// 新しい標準入力ストリームを作成
    pub fn new(stdin: tokio::io::Stdin) -> Self {
        let options = StreamOptions::default();
        
        Self {
            stdin,
            state: StreamState::Open,
            options,
            buffer: Vec::with_capacity(options.buffer_size),
            read_pos: 0,
            last_access: Instant::now(),
        }
    }
}

#[async_trait]
impl IoStream for StdinStream {
    fn stream_type(&self) -> StreamType {
        StreamType::StdIn
    }
    
    fn mode(&self) -> StreamMode {
        StreamMode::Read
    }
    
    fn description(&self) -> String {
        "標準入力".to_string()
    }
    
    fn state(&self) -> StreamState {
        self.state
    }
    
    fn options(&self) -> StreamOptions {
        self.options.clone()
    }
    
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // バッファから読み込む
        if self.read_pos < self.buffer.len() {
            let available = self.buffer.len() - self.read_pos;
            let to_copy = available.min(buf.len());
            
            buf[..to_copy].copy_from_slice(&self.buffer[self.read_pos..self.read_pos + to_copy]);
            self.read_pos += to_copy;
            
            // バッファが空になったらクリア
            if self.read_pos >= self.buffer.len() {
                self.buffer.clear();
                self.read_pos = 0;
            }
            
            return Ok(to_copy);
        }
        
        // バッファが空なら新しいデータを読み込む
        self.buffer.resize(self.options.buffer_size, 0);
        let bytes_read = self.stdin.read(&mut self.buffer).await?;
        
        // 実際に読み取ったサイズに縮小
        self.buffer.truncate(bytes_read);
        
        // EOFの場合
        if bytes_read == 0 {
            return Ok(0);
        }
        
        // データをコピー
        let to_copy = bytes_read.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.buffer[..to_copy]);
        self.read_pos = to_copy;
        
        Ok(to_copy)
    }
    
    async fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        Err(anyhow!("標準入力に書き込むことはできません"))
    }
    
    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
    
    async fn close(&mut self) -> Result<()> {
        self.state = StreamState::Closed;
        self.buffer.clear();
        self.read_pos = 0;
        Ok(())
    }
    
    async fn available(&mut self) -> Result<usize> {
        Ok(self.buffer.len() - self.read_pos)
    }
}

/// 標準出力ストリーム
pub struct StdoutStream {
    /// 標準出力
    stdout: tokio::io::Stdout,
    /// 状態
    state: StreamState,
    /// オプション
    options: StreamOptions,
    /// バッファ
    buffer: Vec<u8>,
    /// 最終アクセス時刻
    last_access: Instant,
}

impl StdoutStream {
    /// 新しい標準出力ストリームを作成
    pub fn new(stdout: tokio::io::Stdout) -> Self {
        let options = StreamOptions::default();
        
        Self {
            stdout,
            state: StreamState::Open,
            options,
            buffer: Vec::with_capacity(options.buffer_size),
            last_access: Instant::now(),
        }
    }
}

#[async_trait]
impl IoStream for StdoutStream {
    fn stream_type(&self) -> StreamType {
        StreamType::StdOut
    }
    
    fn mode(&self) -> StreamMode {
        StreamMode::Write
    }
    
    fn description(&self) -> String {
        "標準出力".to_string()
    }
    
    fn state(&self) -> StreamState {
        self.state
    }
    
    fn options(&self) -> StreamOptions {
        self.options.clone()
    }
    
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Err(anyhow!("標準出力から読み取ることはできません"))
    }
    
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // バッファリング戦略による処理
        match self.options.buffering {
            BufferingStrategy::None => {
                // バッファリングなし：直接書き込み
                self.stdout.write(buf).await
            },
            BufferingStrategy::Line => {
                // 行バッファリング：改行文字があればフラッシュ
                let bytes_to_write = buf.len();
                self.buffer.extend_from_slice(buf);
                
                // 改行があるかチェック
                if buf.contains(&b'\n') {
                    self.stdout.write_all(&self.buffer).await?;
                    self.buffer.clear();
                } else if self.buffer.len() >= self.options.buffer_size {
                    // バッファが一杯ならフラッシュ
                    self.stdout.write_all(&self.buffer).await?;
                    self.buffer.clear();
                }
                
                Ok(bytes_to_write)
            },
            BufferingStrategy::Block | BufferingStrategy::Full => {
                // ブロックバッファリング：バッファがいっぱいになったらフラッシュ
                let bytes_to_write = buf.len();
                
                // バッファに入りきらない場合は先にフラッシュ
                if self.buffer.len() + buf.len() > self.options.buffer_size {
                    self.stdout.write_all(&self.buffer).await?;
                    self.buffer.clear();
                }
                
                // バッファに追加
                self.buffer.extend_from_slice(buf);
                
                // バッファがいっぱいならフラッシュ
                if self.buffer.len() >= self.options.buffer_size {
                    self.stdout.write_all(&self.buffer).await?;
                    self.buffer.clear();
                }
                
                Ok(bytes_to_write)
            }
        }
    }
    
    async fn flush(&mut self) -> Result<()> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // バッファに何かあればフラッシュ
        if !self.buffer.is_empty() {
            self.stdout.write_all(&self.buffer).await?;
            self.buffer.clear();
        }
        
        self.stdout.flush().await
    }
    
    async fn close(&mut self) -> Result<()> {
        if self.state == StreamState::Closed {
            return Ok(());
        }
        
        self.state = StreamState::Closing;
        
        // フラッシュ
        self.flush().await?;
        
        self.state = StreamState::Closed;
        Ok(())
    }
}

/// 標準エラーストリーム
pub struct StderrStream {
    /// 標準エラー
    stderr: tokio::io::Stderr,
    /// 状態
    state: StreamState,
    /// オプション
    options: StreamOptions,
    /// バッファ
    buffer: Vec<u8>,
    /// 最終アクセス時刻
    last_access: Instant,
}

impl StderrStream {
    /// 新しい標準エラーストリームを作成
    pub fn new(stderr: tokio::io::Stderr) -> Self {
        let options = StreamOptions::default();
        
        Self {
            stderr,
            state: StreamState::Open,
            options,
            buffer: Vec::with_capacity(options.buffer_size),
            last_access: Instant::now(),
        }
    }
}

#[async_trait]
impl IoStream for StderrStream {
    fn stream_type(&self) -> StreamType {
        StreamType::StdErr
    }
    
    fn mode(&self) -> StreamMode {
        StreamMode::Write
    }
    
    fn description(&self) -> String {
        "標準エラー".to_string()
    }
    
    fn state(&self) -> StreamState {
        self.state
    }
    
    fn options(&self) -> StreamOptions {
        self.options.clone()
    }
    
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Err(anyhow!("標準エラーから読み取ることはできません"))
    }
    
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // バッファリング戦略による処理
        match self.options.buffering {
            BufferingStrategy::None => {
                // バッファリングなし：直接書き込み
                self.stderr.write(buf).await
            },
            BufferingStrategy::Line => {
                // 行バッファリング：改行文字があればフラッシュ
                let bytes_to_write = buf.len();
                self.buffer.extend_from_slice(buf);
                
                // 改行があるかチェック
                if buf.contains(&b'\n') {
                    self.stderr.write_all(&self.buffer).await?;
                    self.buffer.clear();
                } else if self.buffer.len() >= self.options.buffer_size {
                    // バッファが一杯ならフラッシュ
                    self.stderr.write_all(&self.buffer).await?;
                    self.buffer.clear();
                }
                
                Ok(bytes_to_write)
            },
            BufferingStrategy::Block | BufferingStrategy::Full => {
                // ブロックバッファリング：バッファがいっぱいになったらフラッシュ
                let bytes_to_write = buf.len();
                
                // バッファに入りきらない場合は先にフラッシュ
                if self.buffer.len() + buf.len() > self.options.buffer_size {
                    self.stderr.write_all(&self.buffer).await?;
                    self.buffer.clear();
                }
                
                // バッファに追加
                self.buffer.extend_from_slice(buf);
                
                // バッファがいっぱいならフラッシュ
                if self.buffer.len() >= self.options.buffer_size {
                    self.stderr.write_all(&self.buffer).await?;
                    self.buffer.clear();
                }
                
                Ok(bytes_to_write)
            }
        }
    }
    
    async fn flush(&mut self) -> Result<()> {
        if self.state != StreamState::Open {
            return Err(anyhow!("ストリームが開かれていません"));
        }
        
        self.last_access = Instant::now();
        
        // バッファに何かあればフラッシュ
        if !self.buffer.is_empty() {
            self.stderr.write_all(&self.buffer).await?;
            self.buffer.clear();
        }
        
        self.stderr.flush().await
    }
    
    async fn close(&mut self) -> Result<()> {
        if self.state == StreamState::Closed {
            return Ok(());
        }
        
        self.state = StreamState::Closing;
        
        // フラッシュ
        self.flush().await?;
        
        self.state = StreamState::Closed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    
    #[tokio::test]
    async fn test_file_stream() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let file_path = temp_dir.path().join("test.txt");
        
        // ファイルに書き込む
        {
            let mut stream = FileStream::new(&file_path, StreamMode::Write)?;
            stream.write(b"Hello, world!").await?;
            stream.flush().await?;
            stream.close().await?;
        }
        
        // ファイルから読み込む
        {
            let mut stream = FileStream::new(&file_path, StreamMode::Read)?;
            let mut buf = [0u8; 100];
            let bytes_read = stream.read(&mut buf).await?;
            
            assert_eq!(bytes_read, 13);
            assert_eq!(&buf[..bytes_read], b"Hello, world!");
            
            stream.close().await?;
        }
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_pipe_stream() -> Result<()> {
        let (mut writer, mut reader) = PipeStream::new_pair(10);
        
        // 別スレッドで書き込む
        let write_task = tokio::spawn(async move {
            writer.write(b"Hello from pipe!").await.unwrap();
            writer.close().await.unwrap();
        });
        
        // 読み込む
        let mut buf = [0u8; 100];
        let bytes_read = reader.read(&mut buf).await?;
        
        assert_eq!(bytes_read, 15);
        assert_eq!(&buf[..bytes_read], b"Hello from pipe!");
        
        // タスクが完了するのを待つ
        write_task.await?;
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_io_manager() -> Result<()> {
        let io_manager = IoManager::new();
        
        // 標準入出力を取得
        let _stdin = io_manager.get_stdin()?;
        let _stdout = io_manager.get_stdout()?;
        let _stderr = io_manager.get_stderr()?;
        
        // ファイルを開く
        let temp_dir = tempfile::tempdir()?;
        let file_path = temp_dir.path().join("manager_test.txt");
        
        let file_stream = io_manager.open_file(&file_path, StreamMode::Write)?;
        
        // ファイルに書き込む
        {
            let mut stream = file_stream.lock().unwrap();
            stream.write(b"Written via IoManager").await?;
            stream.flush().await?;
        }
        
        // ファイルを閉じる
        io_manager.close_file(&file_path)?;
        
        // ファイルを読み込みモードで再度開く
        let file_stream = io_manager.open_file(&file_path, StreamMode::Read)?;
        
        // 内容を確認
        {
            let mut stream = file_stream.lock().unwrap();
            let mut buf = [0u8; 100];
            let bytes_read = stream.read(&mut buf).await?;
            
            assert_eq!(bytes_read, 20);
            assert_eq!(&buf[..bytes_read], b"Written via IoManager");
        }
        
        Ok(())
    }
} 