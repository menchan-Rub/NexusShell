/**
 * 分散モニタリングモジュール
 * 
 * 分散パイプライン実行におけるモニタリングを担当するモジュール
 */

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use anyhow::{Result, anyhow, Context};
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, info, warn, error, trace};
use uuid::Uuid;
use hyper::{Body, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use std::convert::Infallible;
use std::net::SocketAddr;

use super::node::{NodeId, NodeInfo, NodeStatus, NodeLoad};
use super::task::{DistributedTask, TaskStatus};
use super::communication::CommunicationManager;

/// メトリクス種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    /// CPU使用率（％）
    CpuUsage,
    /// メモリ使用率（％）
    MemoryUsage,
    /// ディスク使用率（％）
    DiskUsage,
    /// ネットワーク使用率（％）
    NetworkUsage,
    /// タスク実行時間（ミリ秒）
    TaskExecutionTime,
    /// キュー待ち時間（ミリ秒）
    QueueWaitTime,
    /// タスク成功率（％）
    TaskSuccessRate,
    /// ノード応答時間（ミリ秒）
    NodeResponseTime,
}

/// メトリクス値
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    /// メトリクス種別
    pub metric_type: MetricType,
    /// 値
    pub value: f64,
    /// タイムスタンプ（ミリ秒）
    pub timestamp: u64,
    /// ラベル
    pub labels: HashMap<String, String>,
}

impl MetricValue {
    /// 新しいメトリクス値を作成
    pub fn new(metric_type: MetricType, value: f64) -> Self {
        Self {
            metric_type,
            value,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            labels: HashMap::new(),
        }
    }
    
    /// ラベルを追加
    pub fn with_label(mut self, key: &str, value: &str) -> Self {
        self.labels.insert(key.to_string(), value.to_string());
        self
    }
    
    /// ノードラベルを追加
    pub fn with_node(self, node_id: &NodeId) -> Self {
        self.with_label("node_id", &node_id.to_string())
    }
    
    /// タスクラベルを追加
    pub fn with_task(self, task_id: &str) -> Self {
        self.with_label("task_id", task_id)
    }
}

/// メトリクス時系列
#[derive(Debug, Clone)]
pub struct MetricTimeSeries {
    /// メトリクス種別
    pub metric_type: MetricType,
    /// ラベル
    pub labels: HashMap<String, String>,
    /// データポイント（タイムスタンプ, 値）
    pub data_points: VecDeque<(u64, f64)>,
    /// 最大保持ポイント数
    pub max_points: usize,
}

impl MetricTimeSeries {
    /// 新しいメトリクス時系列を作成
    pub fn new(metric_type: MetricType, max_points: usize) -> Self {
        Self {
            metric_type,
            labels: HashMap::new(),
            data_points: VecDeque::with_capacity(max_points),
            max_points,
        }
    }
    
    /// ラベルを追加
    pub fn with_label(mut self, key: &str, value: &str) -> Self {
        self.labels.insert(key.to_string(), value.to_string());
        self
    }
    
    /// メトリクス値を追加
    pub fn add_value(&mut self, value: &MetricValue) {
        if value.metric_type != self.metric_type {
            return;
        }
        
        // ラベルが一致するか確認
        for (key, val) in &value.labels {
            if let Some(existing) = self.labels.get(key) {
                if existing != val {
                    return;
                }
            } else {
                return;
            }
        }
        
        // データポイントを追加
        self.data_points.push_back((value.timestamp, value.value));
        
        // 最大ポイント数を超えたら古いものを削除
        while self.data_points.len() > self.max_points {
            self.data_points.pop_front();
        }
    }
    
    /// 最新の値を取得
    pub fn latest_value(&self) -> Option<(u64, f64)> {
        self.data_points.back().cloned()
    }
    
    /// 平均値を計算
    pub fn average(&self) -> Option<f64> {
        if self.data_points.is_empty() {
            return None;
        }
        
        let sum: f64 = self.data_points.iter().map(|(_, value)| *value).sum();
        Some(sum / self.data_points.len() as f64)
    }
}

/// モニタリング設定
#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    /// メトリクス収集間隔（秒）
    pub collection_interval_sec: u64,
    /// メトリクス保持期間（秒）
    pub retention_period_sec: u64,
    /// 最大時系列数
    pub max_time_series: usize,
    /// 時系列あたりの最大ポイント数
    pub max_points_per_series: usize,
    /// メトリクスエクスポート先
    pub exporters: Vec<MetricsExporter>,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            collection_interval_sec: 10,
            retention_period_sec: 3600, // 1時間
            max_time_series: 1000,
            max_points_per_series: 360, // 1時間（10秒間隔）
            exporters: vec![],
        }
    }
}

/// メトリクスエクスポーター
#[derive(Debug, Clone)]
pub enum MetricsExporter {
    /// プロメテウス
    Prometheus {
        /// エンドポイント
        endpoint: String,
    },
    /// ログ
    Log {
        /// 間隔（秒）
        interval_sec: u64,
    },
}

/// モニタリングマネージャー
pub struct MonitoringManager {
    /// 設定
    config: MonitoringConfig,
    /// 通信マネージャー
    comm_manager: Arc<CommunicationManager>,
    /// メトリクス時系列
    metrics: Arc<RwLock<HashMap<String, MetricTimeSeries>>>,
    /// メトリクス収集チャネル
    metrics_tx: mpsc::Sender<MetricValue>,
    /// メトリクス収集チャネル（受信側）
    metrics_rx: Arc<Mutex<mpsc::Receiver<MetricValue>>>,
    /// 実行状態
    running: Arc<RwLock<bool>>,
}

impl MonitoringManager {
    /// 新しいモニタリングマネージャーを作成
    pub fn new(comm_manager: Arc<CommunicationManager>, config: MonitoringConfig) -> Self {
        let (metrics_tx, metrics_rx) = mpsc::channel(1000);
        
        Self {
            config,
            comm_manager,
            metrics: Arc::new(RwLock::new(HashMap::new())),
            metrics_tx,
            metrics_rx: Arc::new(Mutex::new(metrics_rx)),
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// モニタリングを開始
    pub async fn start(&self) -> Result<()> {
        // 実行状態を設定
        {
            let mut running = self.running.write().await;
            if *running {
                return Ok(());
            }
            *running = true;
        }
        
        // メトリクス収集ループを開始
        let metrics = self.metrics.clone();
        let metrics_rx = self.metrics_rx.clone();
        let running = self.running.clone();
        let max_points = self.config.max_points_per_series;
        
        tokio::spawn(async move {
            while let Ok(true) = {
                let guard = running.read().await;
                Ok::<_, anyhow::Error>(*guard)
            } {
                let mut rx = metrics_rx.lock().await;
                
                match rx.try_recv() {
                    Ok(metric) => {
                        // メトリクスを処理
                        Self::process_metric(metrics.clone(), metric, max_points).await;
                    },
                    Err(mpsc::error::TryRecvError::Empty) => {
                        // キューが空なので少し待機
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    },
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        error!("メトリクス収集チャネルが切断されました");
                        break;
                    }
                }
            }
            
            debug!("メトリクス収集ループを終了しました");
        });
        
        // 定期的なメトリクスエクスポートを開始
        for exporter in &self.config.exporters {
            match exporter {
                MetricsExporter::Prometheus { endpoint } => {
                    self.start_prometheus_exporter(endpoint).await?;
                },
                MetricsExporter::Log { interval_sec } => {
                    self.start_log_exporter(*interval_sec).await?;
                },
            }
        }
        
        info!("モニタリングマネージャーを開始しました");
        Ok(())
    }
    
    /// モニタリングを停止
    pub async fn stop(&self) -> Result<()> {
        // 実行状態を更新
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        
        info!("モニタリングマネージャーを停止しました");
        Ok(())
    }
    
    /// メトリクスを追加
    pub async fn add_metric(&self, metric: MetricValue) -> Result<()> {
        self.metrics_tx.send(metric).await
            .map_err(|e| anyhow!("メトリクスの送信に失敗: {}", e))
    }
    
    /// メトリクスを処理
    async fn process_metric(
        metrics: Arc<RwLock<HashMap<String, MetricTimeSeries>>>,
        metric: MetricValue,
        max_points: usize,
    ) {
        let mut metrics_map = metrics.write().await;
        
        // メトリクスキーを生成（メトリクス種別+ラベル）
        let key = Self::generate_metric_key(&metric);
        
        // 時系列を取得または作成
        let time_series = metrics_map
            .entry(key)
            .or_insert_with(|| {
                let mut ts = MetricTimeSeries::new(metric.metric_type, max_points);
                
                // ラベルをコピー
                for (k, v) in &metric.labels {
                    ts.labels.insert(k.clone(), v.clone());
                }
                
                ts
            });
        
        // メトリクス値を追加
        time_series.add_value(&metric);
    }
    
    /// メトリクスキーを生成
    fn generate_metric_key(metric: &MetricValue) -> String {
        let mut key = format!("{:?}", metric.metric_type);
        
        // ラベルをアルファベット順にソート
        let mut labels: Vec<_> = metric.labels.iter().collect();
        labels.sort_by(|(a, _), (b, _)| a.cmp(b));
        
        // ラベルを追加
        for (k, v) in labels {
            key.push_str(&format!("_{}_{}", k, v));
        }
        
        key
    }
    
    /// プロメテウスエクスポーターを開始
    async fn start_prometheus_exporter(&self, endpoint: &str) -> Result<()> {
        debug!("プロメテウスエクスポーターを開始します: {}", endpoint);
        
        // エンドポイントをパース
        let addr: SocketAddr = endpoint.parse()
            .map_err(|e| anyhow!("プロメテウスエンドポイントのパースに失敗: {}", e))?;
        
        // メトリクスへの参照をクローン
        let metrics = self.metrics.clone();
        
        // メトリクスハンドラーサービス
        let make_service = make_service_fn(move |_conn| {
            let metrics = metrics.clone();
            
            async move {
                let metrics_handler = move |_req| {
                    let metrics_ref = metrics.clone();
                    
                    async move {
                        let metrics_data = Self::generate_prometheus_metrics(metrics_ref).await;
                        let response = Response::builder()
                            .header("Content-Type", "text/plain")
                            .body(Body::from(metrics_data))
                            .unwrap();
                        
                        Ok::<_, Infallible>(response)
                    }
                };
                
                Ok::<_, Infallible>(service_fn(metrics_handler))
            }
        });

        // サーバーを開始（バックグラウンドで実行）
        let server = Server::bind(&addr).serve(make_service);
        info!("プロメテウスエクスポーターを開始しました: http://{}/metrics", addr);
        
        // サーバーを別タスクで実行
        tokio::spawn(async move {
            if let Err(e) = server.await {
                error!("プロメテウスエクスポーターの実行中にエラーが発生しました: {}", e);
            }
        });
        
        Ok(())
    }
    
    /// プロメテウス形式のメトリクスを生成
    async fn generate_prometheus_metrics(metrics: Arc<RwLock<HashMap<String, MetricTimeSeries>>>) -> String {
        let mut output = String::new();
        
        // ヘッダー情報
        output.push_str("# HELP nexusshell_metric NexusShellメトリクス\n");
        output.push_str("# TYPE nexusshell_metric gauge\n");
        
        // メトリクスの読み取り
        let metrics_map = metrics.read().await;
        
        // メトリクスをプロメテウス形式に変換
        for (key, time_series) in metrics_map.iter() {
            if let Some((timestamp, value)) = time_series.latest_value() {
                // メトリクス名とラベルを分離
                let parts: Vec<&str> = key.split('_').collect();
                let metric_name = parts.first().unwrap_or(&"unknown");
                
                // ラベルを構築
                let labels = if parts.len() > 1 {
                    let label_parts = &parts[1..];
                    let mut labels = String::new();
                    
                    for i in (0..label_parts.len()).step_by(2) {
                        if i + 1 < label_parts.len() {
                            if !labels.is_empty() {
                                labels.push_str(",");
                            }
                            labels.push_str(&format!("{}=\"{}\"", label_parts[i], label_parts[i+1]));
                        }
                    }
                    
                    if !labels.is_empty() {
                        format!("{{{}}}", labels)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                
                // プロメテウス形式の行を追加
                output.push_str(&format!("nexusshell_{}{} {} {}\n", 
                    metric_name, 
                    labels,
                    value,
                    timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs()
                ));
            }
        }
        
        output
    }
    
    /// ログエクスポーターを開始
    async fn start_log_exporter(&self, interval_sec: u64) -> Result<()> {
        let metrics = self.metrics.clone();
        let running = self.running.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_sec));
            
            while let Ok(true) = {
                let guard = running.read().await;
                Ok::<_, anyhow::Error>(*guard)
            } {
                interval.tick().await;
                
                // 現在のメトリクスをログ出力
                let metrics_map = metrics.read().await;
                
                info!("現在のメトリクス数: {}", metrics_map.len());
                
                for (key, time_series) in metrics_map.iter() {
                    if let Some((timestamp, value)) = time_series.latest_value() {
                        info!(
                            "メトリクス[{}]: {:?} = {:.2} (@{})",
                            key,
                            time_series.metric_type,
                            value,
                            timestamp
                        );
                    }
                }
            }
            
            debug!("メトリクスログエクスポーターを終了しました");
        });
        
        Ok(())
    }
    
    /// 特定のメトリクスを取得
    pub async fn get_metric(
        &self,
        metric_type: MetricType,
        labels: &HashMap<String, String>,
    ) -> Option<MetricTimeSeries> {
        let metrics = self.metrics.read().await;
        
        // メトリクスキーを生成
        let mut key = format!("{:?}", metric_type);
        
        // ラベルをアルファベット順にソート
        let mut label_pairs: Vec<_> = labels.iter().collect();
        label_pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        
        // ラベルを追加
        for (k, v) in label_pairs {
            key.push_str(&format!("_{}_{}", k, v));
        }
        
        metrics.get(&key).cloned()
    }
    
    /// ノードのCPU使用率メトリクスを取得
    pub async fn get_node_cpu_usage(&self, node_id: &NodeId) -> Option<f64> {
        let mut labels = HashMap::new();
        labels.insert("node_id".to_string(), node_id.to_string());
        
        self.get_metric(MetricType::CpuUsage, &labels).await
            .and_then(|ts| ts.latest_value())
            .map(|(_, value)| value)
    }
    
    /// ノードのメモリ使用率メトリクスを取得
    pub async fn get_node_memory_usage(&self, node_id: &NodeId) -> Option<f64> {
        let mut labels = HashMap::new();
        labels.insert("node_id".to_string(), node_id.to_string());
        
        self.get_metric(MetricType::MemoryUsage, &labels).await
            .and_then(|ts| ts.latest_value())
            .map(|(_, value)| value)
    }
    
    /// クラスター全体のCPU使用率の平均を取得
    pub async fn get_cluster_average_cpu_usage(&self) -> Option<f64> {
        let metrics = self.metrics.read().await;
        
        let cpu_metrics: Vec<_> = metrics.values()
            .filter(|ts| ts.metric_type == MetricType::CpuUsage)
            .collect();
        
        if cpu_metrics.is_empty() {
            return None;
        }
        
        let sum: f64 = cpu_metrics.iter()
            .filter_map(|ts| ts.latest_value().map(|(_, value)| value))
            .sum();
            
        Some(sum / cpu_metrics.len() as f64)
    }
    
    /// クラスター全体のメモリ使用率の平均を取得
    pub async fn get_cluster_average_memory_usage(&self) -> Option<f64> {
        let metrics = self.metrics.read().await;
        
        let memory_metrics: Vec<_> = metrics.values()
            .filter(|ts| ts.metric_type == MetricType::MemoryUsage)
            .collect();
        
        if memory_metrics.is_empty() {
            return None;
        }
        
        let sum: f64 = memory_metrics.iter()
            .filter_map(|ts| ts.latest_value().map(|(_, value)| value))
            .sum();
            
        Some(sum / memory_metrics.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metric_value() {
        let node_id = NodeId::from_string("test-node".to_string());
        let metric = MetricValue::new(MetricType::CpuUsage, 75.5)
            .with_node(&node_id);
        
        assert_eq!(metric.metric_type, MetricType::CpuUsage);
        assert_eq!(metric.value, 75.5);
        assert_eq!(metric.labels.get("node_id").unwrap(), "test-node");
    }
    
    #[test]
    fn test_metric_time_series() {
        let mut ts = MetricTimeSeries::new(MetricType::CpuUsage, 5);
        
        // データポイントを追加
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
            
        for i in 0..10 {
            let metric = MetricValue::new(MetricType::CpuUsage, i as f64 * 10.0);
            ts.add_value(&metric);
        }
        
        // 最大ポイント数を超えたら古いものが削除されるはず
        assert_eq!(ts.data_points.len(), 5);
        
        // 最新の値が正しいか確認
        let (_, latest_value) = ts.latest_value().unwrap();
        assert_eq!(latest_value, 90.0);
        
        // 平均値が正しいか確認
        assert_eq!(ts.average().unwrap(), (50.0 + 60.0 + 70.0 + 80.0 + 90.0) / 5.0);
    }
} 