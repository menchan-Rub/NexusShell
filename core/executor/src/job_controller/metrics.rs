use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use log::{debug, trace};
use metrics::{counter, gauge, histogram};

use super::error::JobError;
use super::job::{Job, JobStatus};
use super::resource_monitor::ResourceMonitor;

/// ジョブメトリクス
/// ジョブの実行に関する各種メトリクス情報
#[derive(Debug, Clone)]
pub struct JobMetrics {
    /// ジョブID
    pub job_id: String,
    /// CPU使用率（%）
    pub cpu_usage: f64,
    /// メモリ使用量（バイト）
    pub memory_usage: u64,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
    /// 標準出力のバイト数
    pub stdout_bytes: usize,
    /// 標準エラー出力のバイト数
    pub stderr_bytes: usize,
    /// キュー待機時間（ミリ秒）
    pub queue_time_ms: u64,
    /// ディスク読み取り（バイト）
    pub disk_read_bytes: u64,
    /// ディスク書き込み（バイト）
    pub disk_write_bytes: u64,
    /// ネットワーク受信（バイト）
    pub network_rx_bytes: u64,
    /// ネットワーク送信（バイト）
    pub network_tx_bytes: u64,
    /// ジョブの現在のステータス
    pub status: JobStatus,
    /// ジョブのPID
    pub pid: Option<u32>,
    /// 子プロセスの数
    pub child_process_count: usize,
    /// 終了コード
    pub exit_code: Option<i32>,
    /// タイムスタンプ（UTC、エポックからのミリ秒）
    pub timestamp: u64,
    /// カスタムメトリクス
    pub custom_metrics: HashMap<String, f64>,
}

impl Default for JobMetrics {
    fn default() -> Self {
        Self {
            job_id: String::new(),
            cpu_usage: 0.0,
            memory_usage: 0,
            execution_time_ms: 0,
            stdout_bytes: 0,
            stderr_bytes: 0,
            queue_time_ms: 0,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            status: JobStatus::Pending,
            pid: None,
            child_process_count: 0,
            exit_code: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_millis() as u64,
            custom_metrics: HashMap::new(),
        }
    }
}

impl JobMetrics {
    /// 新しいジョブメトリクスを作成します
    pub fn new(job_id: &str) -> Self {
        Self {
            job_id: job_id.to_string(),
            ..Default::default()
        }
    }
    
    /// CPU使用率を設定します
    pub fn set_cpu_usage(&mut self, usage: f64) -> &mut Self {
        self.cpu_usage = usage;
        self
    }
    
    /// メモリ使用量を設定します
    pub fn set_memory_usage(&mut self, usage: u64) -> &mut Self {
        self.memory_usage = usage;
        self
    }
    
    /// 実行時間を設定します
    pub fn set_execution_time(&mut self, time_ms: u64) -> &mut Self {
        self.execution_time_ms = time_ms;
        self
    }
    
    /// 標準出力のバイト数を設定します
    pub fn set_stdout_bytes(&mut self, bytes: usize) -> &mut Self {
        self.stdout_bytes = bytes;
        self
    }
    
    /// 標準エラー出力のバイト数を設定します
    pub fn set_stderr_bytes(&mut self, bytes: usize) -> &mut Self {
        self.stderr_bytes = bytes;
        self
    }
    
    /// キュー待機時間を設定します
    pub fn set_queue_time(&mut self, time_ms: u64) -> &mut Self {
        self.queue_time_ms = time_ms;
        self
    }
    
    /// ディスク読み取りバイト数を設定します
    pub fn set_disk_read_bytes(&mut self, bytes: u64) -> &mut Self {
        self.disk_read_bytes = bytes;
        self
    }
    
    /// ディスク書き込みバイト数を設定します
    pub fn set_disk_write_bytes(&mut self, bytes: u64) -> &mut Self {
        self.disk_write_bytes = bytes;
        self
    }
    
    /// ネットワーク受信バイト数を設定します
    pub fn set_network_rx_bytes(&mut self, bytes: u64) -> &mut Self {
        self.network_rx_bytes = bytes;
        self
    }
    
    /// ネットワーク送信バイト数を設定します
    pub fn set_network_tx_bytes(&mut self, bytes: u64) -> &mut Self {
        self.network_tx_bytes = bytes;
        self
    }
    
    /// ステータスを設定します
    pub fn set_status(&mut self, status: JobStatus) -> &mut Self {
        self.status = status;
        self
    }
    
    /// PIDを設定します
    pub fn set_pid(&mut self, pid: Option<u32>) -> &mut Self {
        self.pid = pid;
        self
    }
    
    /// 子プロセス数を設定します
    pub fn set_child_process_count(&mut self, count: usize) -> &mut Self {
        self.child_process_count = count;
        self
    }
    
    /// 終了コードを設定します
    pub fn set_exit_code(&mut self, code: Option<i32>) -> &mut Self {
        self.exit_code = code;
        self
    }
    
    /// カスタムメトリクスを追加します
    pub fn add_custom_metric(&mut self, key: &str, value: f64) -> &mut Self {
        self.custom_metrics.insert(key.to_string(), value);
        self
    }
    
    /// タイムスタンプを現在時刻で更新します
    pub fn update_timestamp(&mut self) -> &mut Self {
        self.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_millis() as u64;
        self
    }
    
    /// メトリクスの概要を文字列として返します
    pub fn summary(&self) -> String {
        format!(
            "JobMetrics {{ id: {}, status: {:?}, cpu: {:.1}%, mem: {}B, time: {}ms, exit: {:?} }}",
            self.job_id,
            self.status,
            self.cpu_usage,
            self.memory_usage,
            self.execution_time_ms,
            self.exit_code
        )
    }
}

/// メトリクスコレクター
/// ジョブのメトリクスを収集・保持するコンポーネント
pub struct MetricsCollector {
    /// ジョブメトリクスのキャッシュ
    metrics_cache: Arc<RwLock<HashMap<String, JobMetrics>>>,
    /// ジョブのキュー投入時刻
    queue_times: Arc<RwLock<HashMap<String, Instant>>>,
    /// リソースモニター
    resource_monitor: Arc<ResourceMonitor>,
    /// 履歴を保持する最大ジョブ数
    max_history_size: usize,
    /// カスタムメトリクスのコールバック
    custom_metrics_collectors: Arc<RwLock<Vec<Arc<dyn Fn(&Job) -> HashMap<String, f64> + Send + Sync>>>>,
}

impl MetricsCollector {
    /// 新しいメトリクスコレクターを作成します
    pub fn new() -> Self {
        Self {
            metrics_cache: Arc::new(RwLock::new(HashMap::new())),
            queue_times: Arc::new(RwLock::new(HashMap::new())),
            resource_monitor: Arc::new(ResourceMonitor::new()),
            max_history_size: 1000,
            custom_metrics_collectors: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// 履歴を保持する最大ジョブ数を設定します
    pub fn set_max_history_size(&mut self, size: usize) {
        self.max_history_size = size;
    }
    
    /// カスタムメトリクスコレクターを追加します
    pub async fn add_custom_collector<F>(&self, collector: F)
    where
        F: Fn(&Job) -> HashMap<String, f64> + Send + Sync + 'static,
    {
        let mut collectors = self.custom_metrics_collectors.write().await;
        collectors.push(Arc::new(collector));
    }
    
    /// ジョブメトリクスを収集します
    pub async fn collect_job_metrics(&self, job: &Job) -> Result<JobMetrics, JobError> {
        let mut metrics = JobMetrics::new(job.id());
        
        // ジョブ情報を収集
        metrics.set_status(job.status());
        metrics.set_pid(job.pid().await);
        metrics.set_exit_code(job.exit_code().await);
        
        // 標準出力・標準エラー出力のサイズ
        metrics.set_stdout_bytes(job.stdout().await.len());
        metrics.set_stderr_bytes(job.stderr().await.len());
        
        // 実行時間を計算
        if let Some(start_time) = job.started_at().await {
            let runtime = if let Some(end_time) = job.finished_at().await {
                end_time.duration_since(start_time)
            } else {
                Instant::now().duration_since(start_time)
            };
            
            metrics.set_execution_time(runtime.as_millis() as u64);
        }
        
        // キュー待機時間を計算
        {
            let queue_times = self.queue_times.read().await;
            if let Some(queue_time) = queue_times.get(job.id()) {
                if let Some(start_time) = job.started_at().await {
                    let wait_time = start_time.duration_since(*queue_time);
                    metrics.set_queue_time(wait_time.as_millis() as u64);
                }
            }
        }
        
        // プロセスリソース使用状況を収集
        if let Some(pid) = job.pid().await {
            if let Some(process_usage) = self.resource_monitor.process_usage(pid).await {
                if let Some(cpu_usage) = process_usage.get("cpu_usage") {
                    metrics.set_cpu_usage(*cpu_usage);
                }
                
                if let Some(memory_used) = process_usage.get("memory_used") {
                    metrics.set_memory_usage(*memory_used as u64);
                }
                
                if let Some(disk_read) = process_usage.get("disk_read_bytes") {
                    metrics.set_disk_read_bytes(*disk_read as u64);
                }
                
                if let Some(disk_write) = process_usage.get("disk_write_bytes") {
                    metrics.set_disk_write_bytes(*disk_write as u64);
                }
            }
            
            // 子プロセスの数を取得
            let child_pids = job.child_pids().await;
            metrics.set_child_process_count(child_pids.len());
            
            // カスタムメトリクスを収集
            let collectors = self.custom_metrics_collectors.read().await;
            for collector in collectors.iter() {
                let custom_metrics = collector(job);
                for (key, value) in custom_metrics {
                    metrics.add_custom_metric(&key, value);
                }
            }
        }
        
        // タイムスタンプを更新
        metrics.update_timestamp();
        
        // メトリクスキャッシュを更新
        {
            let mut cache = self.metrics_cache.write().await;
            cache.insert(job.id().to_string(), metrics.clone());
            
            // キャッシュサイズを制限
            if cache.len() > self.max_history_size {
                let oldest_key = cache.keys()
                    .min_by_key(|k| {
                        cache.get(*k).map(|m| m.timestamp).unwrap_or(u64::MAX)
                    })
                    .cloned();
                
                if let Some(key) = oldest_key {
                    cache.remove(&key);
                }
            }
        }
        
        // Prometheusメトリクスを更新
        self.update_prometheus_metrics(&metrics).await;
        
        Ok(metrics)
    }
    
    /// ジョブのキュー投入時刻を記録します
    pub async fn record_queue_time(&self, job_id: &str) {
        let mut queue_times = self.queue_times.write().await;
        queue_times.insert(job_id.to_string(), Instant::now());
    }
    
    /// ジョブステータス変更時にメトリクスを更新します
    pub async fn update_job_status(&self, job: &Job, status: JobStatus) -> Result<(), JobError> {
        // 現在のメトリクスを取得または新規作成
        let mut metrics = {
            let cache = self.metrics_cache.read().await;
            cache.get(job.id()).cloned().unwrap_or_else(|| JobMetrics::new(job.id()))
        };
        
        // ステータスを更新
        metrics.set_status(status);
        
        // キャッシュを更新
        {
            let mut cache = self.metrics_cache.write().await;
            cache.insert(job.id().to_string(), metrics.clone());
        }
        
        // Prometheusメトリクスを更新
        match status {
            JobStatus::Completed => {
                counter!("nexusshell_jobs_completed_total").increment(1);
                if let Some(exit_code) = job.exit_code().await {
                    if exit_code == 0 {
                        counter!("nexusshell_jobs_succeeded_total").increment(1);
                    } else {
                        counter!("nexusshell_jobs_failed_total").increment(1);
                    }
                }
            }
            JobStatus::Failed => {
                counter!("nexusshell_jobs_failed_total").increment(1);
            }
            JobStatus::Cancelled => {
                counter!("nexusshell_jobs_cancelled_total").increment(1);
            }
            _ => {}
        }
        
        Ok(())
    }
    
    /// キュー内のジョブ待機時間メトリクスを更新します
    pub async fn update_queue_metrics(&self) {
        let now = Instant::now();
        let mut max_wait_time = 0;
        let mut total_wait_time = 0;
        let mut count = 0;
        
        // 現在キュー内のジョブの待機時間を計算
        {
            let queue_times = self.queue_times.read().await;
            
            for (_, queue_time) in queue_times.iter() {
                let wait_time = now.duration_since(*queue_time).as_millis() as u64;
                max_wait_time = max_wait_time.max(wait_time);
                total_wait_time += wait_time;
                count += 1;
            }
        }
        
        // Prometheusメトリクスを更新
        if count > 0 {
            let avg_wait_time = total_wait_time as f64 / count as f64;
            gauge!("nexusshell_job_max_queue_time_ms").set(max_wait_time as f64);
            gauge!("nexusshell_job_avg_queue_time_ms").set(avg_wait_time);
        }
    }
    
    /// ジョブのキュー投入時刻情報をクリーンアップします
    pub async fn cleanup_queue_times(&self, job_id: &str) {
        let mut queue_times = self.queue_times.write().await;
        queue_times.remove(job_id);
    }
    
    /// メトリクスのキャッシュをクリアします
    pub async fn clear_metrics_cache(&self) {
        let mut cache = self.metrics_cache.write().await;
        cache.clear();
        
        debug!("メトリクスキャッシュをクリアしました");
    }
    
    /// ジョブメトリクスの履歴を取得します
    pub async fn get_metrics_history(&self, job_id: &str) -> Option<JobMetrics> {
        let cache = self.metrics_cache.read().await;
        cache.get(job_id).cloned()
    }
    
    /// Prometheusメトリクスを更新します
    async fn update_prometheus_metrics(&self, metrics: &JobMetrics) {
        let job_id = &metrics.job_id;
        
        // ゲージメトリクスを更新
        gauge!("nexusshell_job_cpu_usage", "job_id" => job_id.clone()).set(metrics.cpu_usage);
        gauge!("nexusshell_job_memory_usage", "job_id" => job_id.clone()).set(metrics.memory_usage as f64);
        
        // ジョブ実行時間をヒストグラムに記録
        if metrics.execution_time_ms > 0 {
            histogram!("nexusshell_job_execution_time_ms", "job_id" => job_id.clone())
                .record(metrics.execution_time_ms as f64);
        }
        
        // キュー待機時間をヒストグラムに記録
        if metrics.queue_time_ms > 0 {
            histogram!("nexusshell_job_queue_time_ms", "job_id" => job_id.clone())
                .record(metrics.queue_time_ms as f64);
        }
        
        // 子プロセス数
        gauge!("nexusshell_job_child_processes", "job_id" => job_id.clone())
            .set(metrics.child_process_count as f64);
            
        // I/Oメトリクス
        gauge!("nexusshell_job_stdout_bytes", "job_id" => job_id.clone())
            .set(metrics.stdout_bytes as f64);
        gauge!("nexusshell_job_stderr_bytes", "job_id" => job_id.clone())
            .set(metrics.stderr_bytes as f64);
        gauge!("nexusshell_job_disk_read_bytes", "job_id" => job_id.clone())
            .set(metrics.disk_read_bytes as f64);
        gauge!("nexusshell_job_disk_write_bytes", "job_id" => job_id.clone())
            .set(metrics.disk_write_bytes as f64);
            
        // カスタムメトリクス
        for (key, value) in &metrics.custom_metrics {
            gauge!("nexusshell_job_custom", "job_id" => job_id.clone(), "metric" => key.clone())
                .set(*value);
        }
        
        trace!("ジョブメトリクスを更新しました: {}", metrics.summary());
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_controller::job::JobType;
    
    #[test]
    fn test_job_metrics_basic() {
        let mut metrics = JobMetrics::new("test-job");
        
        metrics.set_cpu_usage(25.5)
               .set_memory_usage(1024 * 1024)
               .set_execution_time(1500)
               .set_status(JobStatus::Running);
        
        assert_eq!(metrics.job_id, "test-job");
        assert_eq!(metrics.cpu_usage, 25.5);
        assert_eq!(metrics.memory_usage, 1024 * 1024);
        assert_eq!(metrics.execution_time_ms, 1500);
        assert_eq!(metrics.status, JobStatus::Running);
    }
    
    #[test]
    fn test_job_metrics_custom() {
        let mut metrics = JobMetrics::new("test-job");
        
        metrics.add_custom_metric("test_metric", 42.0)
               .add_custom_metric("another_metric", 123.45);
        
        assert_eq!(metrics.custom_metrics.get("test_metric"), Some(&42.0));
        assert_eq!(metrics.custom_metrics.get("another_metric"), Some(&123.45));
    }
    
    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        let job = Job::new(JobType::Foreground, "echo test");
        
        // キュー時間を記録
        collector.record_queue_time(job.id()).await;
        
        // 少し待ってからメトリクスを収集
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        let metrics = collector.collect_job_metrics(&job).await.unwrap();
        
        assert_eq!(metrics.job_id, job.id());
        assert_eq!(metrics.status, JobStatus::Pending);
        assert!(metrics.queue_time_ms >= 10);
        
        // キュー時間のクリーンアップ
        collector.cleanup_queue_times(job.id()).await;
        
        // メトリクスキャッシュをクリア
        collector.clear_metrics_cache().await;
    }
} 