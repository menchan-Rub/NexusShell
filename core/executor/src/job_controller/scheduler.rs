use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use log::{debug, error, info};
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use tokio::io::AsyncReadExt;

use super::error::JobError;
use super::job::{Job, JobPriority, JobStatus};

/// ジョブスケジューラー
/// ジョブの実行スケジュールと管理を担当するコンポーネント
pub struct JobScheduler {
    /// 実行待ちのジョブキュー（優先度付き）
    queue: BinaryHeap<(JobPriority, Reverse<String>)>,
    /// 実行中のジョブ数
    running_jobs: usize,
    /// 最大同時実行ジョブ数
    max_concurrent_jobs: usize,
}

impl JobScheduler {
    /// 新しいジョブスケジューラーを作成します
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            running_jobs: 0,
            max_concurrent_jobs: num_cpus::get(),
        }
    }

    /// ジョブを実行スケジュールに追加します
    pub async fn schedule(&mut self, job: Job) -> Result<(), JobError> {
        // キューにジョブを追加
        self.queue.push((job.priority(), Reverse(job.id().to_string())));
        
        debug!("ジョブ {} をスケジュールに追加しました", job.id());
        
        // ジョブの実行を試みる
        self.try_execute_next_job(job).await
    }

    /// 次のジョブの実行を試みます
    async fn try_execute_next_job(&mut self, job: Job) -> Result<(), JobError> {
        // 実行中のジョブ数が上限に達している場合は何もしない
        if self.running_jobs >= self.max_concurrent_jobs {
            debug!("実行中のジョブ数が上限に達しています（{}）", self.max_concurrent_jobs);
            return Ok(());
        }
        
        // キューからジョブを取り出す（優先度順）
        if let Some((_, Reverse(job_id))) = self.queue.pop() {
            // 指定されたジョブでなければキューに戻す
            if job_id != job.id() {
                self.queue.push((job.priority(), Reverse(job.id().to_string())));
                return Ok(());
            }
            
            // ジョブの実行
            self.execute_job(job).await?;
            self.running_jobs += 1;
            
            debug!("ジョブ {} の実行を開始しました（実行中: {}/{}）", 
                  job_id, self.running_jobs, self.max_concurrent_jobs);
        }
        
        Ok(())
    }

    /// ジョブを実行します
    async fn execute_job(&self, job: Job) -> Result<(), JobError> {
        // ジョブの状態を実行中に変更
        job.set_status(JobStatus::Running).await;
        
        // プロセスを起動
        let mut command = Command::new(job.path());
        
        // 引数を設定
        command.args(job.args());
        
        // 環境変数を設定
        for (key, value) in job.env_vars() {
            command.env(key, value);
        }
        
        // 作業ディレクトリを設定
        command.current_dir(job.working_dir());
        
        // 標準入出力をキャプチャ
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        
        // プロセスを起動
        match command.spawn() {
            Ok(mut child) => {
                // プロセスIDを記録
                if let Some(pid) = child.id() {
                    job.set_pid(pid).await;
                    debug!("ジョブ {} のプロセスID: {}", job.id(), pid);
                }
                
                // 非同期でプロセスの完了を待機
                let job_clone = job.clone();
                tokio::spawn(async move {
                    // 標準出力と標準エラーをキャプチャ
                    let stdout = child.stdout.take();
                    let stderr = child.stderr.take();
                    
                    // 標準出力を読み取る
                    if let Some(mut stdout) = stdout {
                        let job_stdout = job_clone.clone();
                        tokio::spawn(async move {
                            let mut buffer = [0; 1024];
                            while let Ok(size) = stdout.read(&mut buffer).await {
                                if size == 0 {
                                    break;
                                }
                                job_stdout.append_stdout(&buffer[..size]).await;
                            }
                        });
                    }
                    
                    // 標準エラーを読み取る
                    if let Some(mut stderr) = stderr {
                        let job_stderr = job_clone.clone();
                        tokio::spawn(async move {
                            let mut buffer = [0; 1024];
                            while let Ok(size) = stderr.read(&mut buffer).await {
                                if size == 0 {
                                    break;
                                }
                                job_stderr.append_stderr(&buffer[..size]).await;
                            }
                        });
                    }
                    
                    // プロセスの完了を待機
                    match child.wait().await {
                        Ok(status) => {
                            let exit_code = status.code().unwrap_or(-1);
                            job_clone.set_exit_code(exit_code).await;
                            
                            // ジョブのステータスを更新
                            if status.success() {
                                info!("ジョブ {} が正常に完了しました (終了コード: {})", job_clone.id(), exit_code);
                                job_clone.set_status(JobStatus::Completed).await;
                            } else {
                                error!("ジョブ {} がエラーで終了しました (終了コード: {})", job_clone.id(), exit_code);
                                job_clone.set_status(JobStatus::Failed).await;
                            }
                        }
                        Err(e) => {
                            error!("ジョブ {} の実行中にエラーが発生しました: {}", job_clone.id(), e);
                            job_clone.set_status(JobStatus::Failed).await;
                        }
                    }
                });
                
                Ok(())
            }
            Err(e) => {
                error!("ジョブ {} の起動に失敗しました: {}", job.id(), e);
                job.set_status(JobStatus::Failed).await;
                Err(JobError::ProcessStartFailed(e.to_string()))
            }
        }
    }

    /// 指定されたIDのジョブをキャンセルします
    pub async fn cancel_job(&self, job: &Job) -> Result<(), JobError> {
        // プロセスが実行中かチェック
        if let Some(pid) = job.pid().await {
            // OSに応じたプロセス終了方法
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                
                if let Err(e) = kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                    return Err(JobError::CancellationFailed(format!(
                        "プロセス {} の終了に失敗しました: {}", pid, e
                    )));
                }
                
                // 少し待ってから SIGKILL を送信
                sleep(Duration::from_millis(500)).await;
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
            
            #[cfg(windows)]
            {
                use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
                use winapi::um::winnt::PROCESS_TERMINATE;
                use winapi::um::handleapi::CloseHandle;
                
                unsafe {
                    let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
                    if handle.is_null() {
                        return Err(JobError::CancellationFailed(format!(
                            "プロセス {} のハンドル取得に失敗しました", pid
                        )));
                    }
                    
                    if TerminateProcess(handle, 1) == 0 {
                        CloseHandle(handle);
                        return Err(JobError::CancellationFailed(format!(
                            "プロセス {} の終了に失敗しました", pid
                        )));
                    }
                    
                    CloseHandle(handle);
                }
            }
        }
        
        // ジョブの状態をキャンセルに変更
        job.set_status(JobStatus::Cancelled).await;
        
        debug!("ジョブ {} をキャンセルしました", job.id());
        
        Ok(())
    }

    /// 実行中のジョブ数を取得します
    pub fn running_jobs(&self) -> usize {
        self.running_jobs
    }

    /// キューにあるジョブ数を取得します
    pub fn queued_jobs(&self) -> usize {
        self.queue.len()
    }

    /// 最大同時実行ジョブ数を設定します
    pub fn set_max_concurrent_jobs(&mut self, max: usize) {
        self.max_concurrent_jobs = max;
    }

    /// 最大同時実行ジョブ数を取得します
    pub fn max_concurrent_jobs(&self) -> usize {
        self.max_concurrent_jobs
    }
}

impl Default for JobScheduler {
    fn default() -> Self {
        Self::new()
    }
} 