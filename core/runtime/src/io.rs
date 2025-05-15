/*!
# 入出力モジュール

シェルの入出力操作を管理するモジュールです。標準入出力、
リダイレクト、パイプなどの機能を提供します。
*/

use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info, warn};

/// 入出力ストリームのタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// カスタム
    Custom,
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
    
    /// ストリームが読み取り可能かどうかを確認
    fn is_readable(&self) -> bool {
        matches!(self.mode(), StreamMode::Read | StreamMode::ReadWrite)
    }
    
    /// ストリームが書き込み可能かどうかを確認
    fn is_writable(&self) -> bool {
        matches!(self.mode(), StreamMode::Write | StreamMode::ReadWrite | StreamMode::Append)
    }
    
    /// データを読み取り
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    
    /// データを書き込み
    async fn write(&mut self, buf: &[u8]) -> Result<usize>;
    
    /// 書き込みバッファをフラッシュ
    async fn flush(&mut self) -> Result<()>;
    
    /// ストリームを閉じる
    async fn close(&mut self) -> Result<()>;
}

/// ファイルストリーム
pub struct FileStream {
    /// ファイルパス
    path: PathBuf,
    /// ストリームモード
    mode: StreamMode,
    /// ファイルハンドル（同期版）
    file: Option<Mutex<File>>,
}

impl FileStream {
    /// 新しいファイルストリームを作成
    pub fn new(path: impl Into<PathBuf>, mode: StreamMode) -> Result<Self> {
        let path = path.into();
        
        // ファイルを開く
        let file = match mode {
            StreamMode::Read => {
                OpenOptions::new()
                    .read(true)
                    .open(&path)
                    .with_context(|| format!("ファイルをオープンできません: {:?}", path))?
            },
            StreamMode::Write => {
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path)
                    .with_context(|| format!("ファイルをオープンできません: {:?}", path))?
            },
            StreamMode::ReadWrite => {
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&path)
                    .with_context(|| format!("ファイルをオープンできません: {:?}", path))?
            },
            StreamMode::Append => {
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(&path)
                    .with_context(|| format!("ファイルをオープンできません: {:?}", path))?
            },
        };
        
        Ok(Self {
            path,
            mode,
            file: Some(Mutex::new(file)),
        })
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
        format!("ファイル: {:?}", self.path)
    }
    
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.is_readable() {
            return Err(anyhow!("ストリームは読み取り可能ではありません"));
        }
        
        let file_guard = match &self.file {
            Some(file) => file.lock().map_err(|_| anyhow!("ファイルロックの取得に失敗"))?,
            None => return Err(anyhow!("ストリームはクローズされています")),
        };
        
        // 同期読み取りを行う
        // 非同期コンテキストで呼び出すため、blocking機能を使用
        tokio::task::block_in_place(|| {
            let mut file = &*file_guard;
            file.read(buf).context("ファイルの読み取りに失敗")
        })
    }
    
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if !self.is_writable() {
            return Err(anyhow!("ストリームは書き込み可能ではありません"));
        }
        
        let file_guard = match &self.file {
            Some(file) => file.lock().map_err(|_| anyhow!("ファイルロックの取得に失敗"))?,
            None => return Err(anyhow!("ストリームはクローズされています")),
        };
        
        // 同期書き込みを行う
        tokio::task::block_in_place(|| {
            let mut file = &*file_guard;
            file.write(buf).context("ファイルの書き込みに失敗")
        })
    }
    
    async fn flush(&mut self) -> Result<()> {
        if !self.is_writable() {
            return Ok(());
        }
        
        let file_guard = match &self.file {
            Some(file) => file.lock().map_err(|_| anyhow!("ファイルロックの取得に失敗"))?,
            None => return Err(anyhow!("ストリームはクローズされています")),
        };
        
        // 同期フラッシュを行う
        tokio::task::block_in_place(|| {
            let mut file = &*file_guard;
            file.flush().context("ファイルのフラッシュに失敗")
        })
    }
    
    async fn close(&mut self) -> Result<()> {
        // ファイルをドロップすることで閉じる
        self.file = None;
        Ok(())
    }
}

/// 標準入出力ストリーム
pub struct StdioStream {
    /// ストリームタイプ
    stream_type: StreamType,
    /// バッファ
    buffer: Vec<u8>,
    /// 位置
    position: usize,
}

impl StdioStream {
    /// 標準入力ストリームを作成
    pub fn stdin() -> Self {
        Self {
            stream_type: StreamType::StdIn,
            buffer: Vec::new(),
            position: 0,
        }
    }
    
    /// 標準出力ストリームを作成
    pub fn stdout() -> Self {
        Self {
            stream_type: StreamType::StdOut,
            buffer: Vec::new(),
            position: 0,
        }
    }
    
    /// 標準エラーストリームを作成
    pub fn stderr() -> Self {
        Self {
            stream_type: StreamType::StdErr,
            buffer: Vec::new(),
            position: 0,
        }
    }
}

#[async_trait]
impl IoStream for StdioStream {
    fn stream_type(&self) -> StreamType {
        self.stream_type
    }
    
    fn mode(&self) -> StreamMode {
        match self.stream_type {
            StreamType::StdIn => StreamMode::Read,
            StreamType::StdOut | StreamType::StdErr => StreamMode::Write,
            _ => unreachable!("不正なストリームタイプ"),
        }
    }
    
    fn description(&self) -> String {
        match self.stream_type {
            StreamType::StdIn => "標準入力".to_string(),
            StreamType::StdOut => "標準出力".to_string(),
            StreamType::StdErr => "標準エラー".to_string(),
            _ => unreachable!("不正なストリームタイプ"),
        }
    }
    
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.stream_type != StreamType::StdIn {
            return Err(anyhow!("このストリームは読み取り可能ではありません"));
        }
        
        // バッファが空なら標準入力から読み込む
        if self.position >= self.buffer.len() {
            self.buffer.resize(4096, 0);
            self.position = 0;
            
            // 標準入力を非同期に読み込む
            let stdin = tokio::io::stdin();
            let mut bytes_read = 0;
            
            tokio::select! {
                result = stdin.read(&mut self.buffer) => {
                    bytes_read = result.context("標準入力の読み取りに失敗")?;
                }
            }
            
            if bytes_read == 0 {
                // EOF
                return Ok(0);
            }
            
            self.buffer.truncate(bytes_read);
        }
        
        // バッファからデータをコピー
        let available = self.buffer.len() - self.position;
        let copy_size = buf.len().min(available);
        
        buf[..copy_size].copy_from_slice(&self.buffer[self.position..self.position + copy_size]);
        self.position += copy_size;
        
        Ok(copy_size)
    }
    
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match self.stream_type {
            StreamType::StdOut => {
                // 標準出力に書き込む
                let mut stdout = tokio::io::stdout();
                stdout.write(buf).await.context("標準出力への書き込みに失敗")
            },
            StreamType::StdErr => {
                // 標準エラーに書き込む
                let mut stderr = tokio::io::stderr();
                stderr.write(buf).await.context("標準エラーへの書き込みに失敗")
            },
            _ => Err(anyhow!("このストリームは書き込み可能ではありません")),
        }
    }
    
    async fn flush(&mut self) -> Result<()> {
        match self.stream_type {
            StreamType::StdOut => {
                let mut stdout = tokio::io::stdout();
                stdout.flush().await.context("標準出力のフラッシュに失敗")
            },
            StreamType::StdErr => {
                let mut stderr = tokio::io::stderr();
                stderr.flush().await.context("標準エラーのフラッシュに失敗")
            },
            _ => Ok(()),
        }
    }
    
    async fn close(&mut self) -> Result<()> {
        // 標準入出力は閉じることができないので何もしない
        Ok(())
    }
}

/// パイプストリーム
pub struct PipeStream {
    /// 読み取り用チャネル
    reader: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    /// 書き込み用チャネル
    writer: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
    /// バッファ
    buffer: Vec<u8>,
    /// 位置
    position: usize,
    /// モード
    mode: StreamMode,
}

impl PipeStream {
    /// 新しいパイプペアを作成
    pub fn new_pair(buffer_size: usize) -> (Self, Self) {
        let (tx1, rx1) = tokio::sync::mpsc::channel(buffer_size);
        let (tx2, rx2) = tokio::sync::mpsc::channel(buffer_size);
        
        // 最初のストリームは書き込み専用
        let writer = Self {
            reader: None,
            writer: Some(tx1),
            buffer: Vec::new(),
            position: 0,
            mode: StreamMode::Write,
        };
        
        // 2番目のストリームは読み取り専用
        let reader = Self {
            reader: Some(rx1),
            writer: None,
            buffer: Vec::new(),
            position: 0,
            mode: StreamMode::Read,
        };
        
        (writer, reader)
    }
}

#[async_trait]
impl IoStream for PipeStream {
    fn stream_type(&self) -> StreamType {
        StreamType::Pipe
    }
    
    fn mode(&self) -> StreamMode {
        self.mode
    }
    
    fn description(&self) -> String {
        "パイプ".to_string()
    }
    
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.is_readable() {
            return Err(anyhow!("このストリームは読み取り可能ではありません"));
        }
        
        // バッファが空ならチャネルから読み込む
        if self.position >= self.buffer.len() {
            // 読み取りチャネルがなければエラー
            let reader = match &mut self.reader {
                Some(r) => r,
                None => return Err(anyhow!("読み取りチャネルがありません")),
            };
            
            // チャネルからデータを受信
            match reader.recv().await {
                Some(data) => {
                    self.buffer = data;
                    self.position = 0;
                },
                None => {
                    // 送信側がクローズされた
                    return Ok(0);
                }
            }
            
            if self.buffer.is_empty() {
                // データなし
                return Ok(0);
            }
        }
        
        // バッファからデータをコピー
        let available = self.buffer.len() - self.position;
        let copy_size = buf.len().min(available);
        
        buf[..copy_size].copy_from_slice(&self.buffer[self.position..self.position + copy_size]);
        self.position += copy_size;
        
        Ok(copy_size)
    }
    
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if !self.is_writable() {
            return Err(anyhow!("このストリームは書き込み可能ではありません"));
        }
        
        // 書き込みチャネルがなければエラー
        let writer = match &self.writer {
            Some(w) => w,
            None => return Err(anyhow!("書き込みチャネルがありません")),
        };
        
        // データをコピーしてチャネルに送信
        let data = buf.to_vec();
        let len = data.len();
        
        match writer.send(data).await {
            Ok(_) => Ok(len),
            Err(_) => Err(anyhow!("パイプの書き込みに失敗")),
        }
    }
    
    async fn flush(&mut self) -> Result<()> {
        // パイプはフラッシュの必要なし
        Ok(())
    }
    
    async fn close(&mut self) -> Result<()> {
        // チャネルをドロップして閉じる
        self.reader = None;
        self.writer = None;
        Ok(())
    }
}

/// 入出力管理クラス
pub struct IoManager {
    /// 標準入力
    stdin: Mutex<Option<Box<dyn IoStream>>>,
    /// 標準出力
    stdout: Mutex<Option<Box<dyn IoStream>>>,
    /// 標準エラー
    stderr: Mutex<Option<Box<dyn IoStream>>>,
    /// 開いているファイルストリーム
    open_files: dashmap::DashMap<PathBuf, Arc<Mutex<Box<dyn IoStream>>>>,
}

impl IoManager {
    /// 新しい入出力マネージャを作成
    pub fn new() -> Self {
        Self {
            stdin: Mutex::new(Some(Box::new(StdioStream::stdin()))),
            stdout: Mutex::new(Some(Box::new(StdioStream::stdout()))),
            stderr: Mutex::new(Some(Box::new(StdioStream::stderr()))),
            open_files: dashmap::DashMap::new(),
        }
    }
    
    /// 標準入力を取得
    pub fn get_stdin(&self) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let mut stdin = self.stdin.lock().map_err(|_| anyhow!("標準入力のロックに失敗"))?;
        
        if stdin.is_none() {
            // 標準入力を再作成
            *stdin = Some(Box::new(StdioStream::stdin()));
        }
        
        let stream = stdin.take().unwrap();
        let stream_arc = Arc::new(Mutex::new(stream));
        
        // arcをクローンして元の場所にも置く
        *stdin = Some(Box::new(StdioStream::stdin()));
        
        Ok(stream_arc)
    }
    
    /// 標準出力を取得
    pub fn get_stdout(&self) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let mut stdout = self.stdout.lock().map_err(|_| anyhow!("標準出力のロックに失敗"))?;
        
        if stdout.is_none() {
            // 標準出力を再作成
            *stdout = Some(Box::new(StdioStream::stdout()));
        }
        
        let stream = stdout.take().unwrap();
        let stream_arc = Arc::new(Mutex::new(stream));
        
        // arcをクローンして元の場所にも置く
        *stdout = Some(Box::new(StdioStream::stdout()));
        
        Ok(stream_arc)
    }
    
    /// 標準エラーを取得
    pub fn get_stderr(&self) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let mut stderr = self.stderr.lock().map_err(|_| anyhow!("標準エラーのロックに失敗"))?;
        
        if stderr.is_none() {
            // 標準エラーを再作成
            *stderr = Some(Box::new(StdioStream::stderr()));
        }
        
        let stream = stderr.take().unwrap();
        let stream_arc = Arc::new(Mutex::new(stream));
        
        // arcをクローンして元の場所にも置く
        *stderr = Some(Box::new(StdioStream::stderr()));
        
        Ok(stream_arc)
    }
    
    /// ファイルを開く
    pub fn open_file(&self, path: impl Into<PathBuf>, mode: StreamMode) -> Result<Arc<Mutex<Box<dyn IoStream>>>> {
        let path = path.into();
        
        // すでに開いているファイルがあるか確認
        if let Some(stream) = self.open_files.get(&path) {
            return Ok(stream.clone());
        }
        
        // 新しいファイルストリームを作成
        let file_stream = FileStream::new(&path, mode)?;
        let stream: Box<dyn IoStream> = Box::new(file_stream);
        let stream_arc = Arc::new(Mutex::new(stream));
        
        // 開いているファイルリストに追加
        self.open_files.insert(path, stream_arc.clone());
        
        Ok(stream_arc)
    }
    
    /// パイプを作成
    pub fn create_pipe(&self) -> (Arc<Mutex<Box<dyn IoStream>>>, Arc<Mutex<Box<dyn IoStream>>>) {
        let (writer, reader) = PipeStream::new_pair(4096);
        
        let writer_box: Box<dyn IoStream> = Box::new(writer);
        let reader_box: Box<dyn IoStream> = Box::new(reader);
        
        (
            Arc::new(Mutex::new(writer_box)),
            Arc::new(Mutex::new(reader_box)),
        )
    }
    
    /// パスをリダイレクト用に解決
    pub fn resolve_redirect_path(&self, path: &str, append: bool) -> Result<(PathBuf, StreamMode)> {
        // チルダを展開
        let expanded_path = if path.starts_with('~') {
            match dirs::home_dir() {
                Some(home) => {
                    if path.len() == 1 {
                        home
                    } else if path.starts_with("~/") || path.starts_with("~\\") {
                        home.join(&path[2..])
                    } else {
                        PathBuf::from(path)
                    }
                },
                None => PathBuf::from(path),
            }
        } else {
            PathBuf::from(path)
        };
        
        // モードを決定
        let mode = if append {
            StreamMode::Append
        } else {
            StreamMode::Write
        };
        
        Ok((expanded_path, mode))
    }
    
    /// ファイルを閉じる
    pub fn close_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        
        if let Some((_, stream)) = self.open_files.remove(path) {
            // ファイルがまだ他の場所で使われている可能性があるため、
            // ここでは明示的に閉じる操作は行わない
            Ok(())
        } else {
            Err(anyhow!("ファイルは開かれていません: {:?}", path))
        }
    }
    
    /// すべてのリソースをクリア
    pub fn clear(&self) {
        self.open_files.clear();
    }
}

impl Default for IoManager {
    fn default() -> Self {
        Self::new()
    }
} 

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;
    
    #[tokio::test]
    async fn test_file_stream() -> Result<()> {
        let temp_dir = tempdir()?;
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
        let temp_dir = tempdir()?;
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