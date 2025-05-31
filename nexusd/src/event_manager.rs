use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio::time::Duration;
use tracing::{info, debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub action: String,
    pub actor: Actor,
    pub scope: String,
    pub time_nano: i64,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    Container,
    Image,
    Volume,
    Network,
    Daemon,
    Plugin,
    Node,
    Service,
    Secret,
    Config,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Container => write!(f, "container"),
            EventType::Image => write!(f, "image"),
            EventType::Volume => write!(f, "volume"),
            EventType::Network => write!(f, "network"),
            EventType::Daemon => write!(f, "daemon"),
            EventType::Plugin => write!(f, "plugin"),
            EventType::Node => write!(f, "node"),
            EventType::Service => write!(f, "service"),
            EventType::Secret => write!(f, "secret"),
            EventType::Config => write!(f, "config"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct EventFilter {
    #[allow(dead_code)]
    pub since: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub until: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub event_types: Vec<EventType>,
    #[allow(dead_code)]
    pub actions: Vec<String>,
    #[allow(dead_code)]
    pub actors: Vec<String>,
    #[allow(dead_code)]
    pub labels: HashMap<String, String>,
}


#[derive(Debug)]
pub struct EventManager {
    events: Arc<RwLock<VecDeque<Event>>>,
    event_sender: broadcast::Sender<Event>,
    max_events: usize,
    retention_duration: Duration,
}

impl EventManager {
    pub fn new() -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        
        Self {
            events: Arc::new(RwLock::new(VecDeque::new())),
            event_sender,
            max_events: 10000,
            retention_duration: Duration::from_secs(24 * 60 * 60), // 24時間
        }
    }

    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing event manager");
        self.start_cleanup_task().await;
        info!("Event manager initialized successfully");
        Ok(())
    }

    pub async fn start_processing(&self) -> Result<()> {
        info!("Starting event processing");
        Ok(())
    }

    async fn start_cleanup_task(&self) {
        let events = self.events.clone();
        let retention_duration = self.retention_duration;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // 1時間ごと
            
            loop {
                interval.tick().await;
                
                let cutoff_time = Utc::now() - chrono::Duration::from_std(retention_duration).unwrap();
                let mut events = events.write().await;
                
                let original_len = events.len();
                events.retain(|event| event.timestamp > cutoff_time);
                let removed = original_len - events.len();
                
                if removed > 0 {
                    debug!("Cleaned up {} old events", removed);
                }
            }
        });
    }

    pub async fn emit_event(&self, event: Event) -> Result<()> {
        debug!("Emitting event: {} - {}", event.event_type, event.action);
        
        // イベントをストレージに追加
        {
            let mut events = self.events.write().await;
            events.push_back(event.clone());
            
            // 最大イベント数を超えた場合、古いイベントを削除
            while events.len() > self.max_events {
                events.pop_front();
            }
        }
        
        // ブロードキャスト送信
        if let Err(e) = self.event_sender.send(event) {
            warn!("Failed to broadcast event: {}", e);
        }
        
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_events(&self, filter: EventFilter) -> Result<Vec<Event>> {
        let events = self.events.read().await;
        let mut filtered_events: Vec<Event> = events.iter().cloned().collect();
        
        // フィルター適用
        if let Some(since) = filter.since {
            filtered_events.retain(|e| e.timestamp >= since);
        }
        
        if let Some(until) = filter.until {
            filtered_events.retain(|e| e.timestamp <= until);
        }
        
        if !filter.event_types.is_empty() {
            filtered_events.retain(|e| filter.event_types.contains(&e.event_type));
        }
        
        if !filter.actions.is_empty() {
            filtered_events.retain(|e| filter.actions.contains(&e.action));
        }
        
        if !filter.actors.is_empty() {
            filtered_events.retain(|e| filter.actors.contains(&e.actor.id));
        }
        
        // ラベルフィルター
        for (key, value) in &filter.labels {
            filtered_events.retain(|e| {
                e.actor.attributes.get(key) == Some(value)
            });
        }
        
        // 時間順でソート（新しい順）
        filtered_events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(filtered_events)
    }

    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.event_sender.subscribe()
    }

    // コンテナイベント生成ヘルパー
    #[allow(dead_code)]
    pub async fn emit_container_event(&self, action: &str, container_id: &str, container_name: &str, image: &str) -> Result<()> {
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), container_name.to_string());
        attributes.insert("image".to_string(), image.to_string());
        
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Container,
            action: action.to_string(),
            actor: Actor {
                id: container_id.to_string(),
                attributes,
            },
            scope: "local".to_string(),
            time_nano: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            attributes: HashMap::new(),
        };
        
        self.emit_event(event).await
    }

    // イメージイベント生成ヘルパー
    #[allow(dead_code)]
    pub async fn emit_image_event(&self, action: &str, image_id: &str, image_name: &str, size: u64) -> Result<()> {
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), image_name.to_string());
        attributes.insert("size".to_string(), size.to_string());
        
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Image,
            action: action.to_string(),
            actor: Actor {
                id: image_id.to_string(),
                attributes,
            },
            scope: "local".to_string(),
            time_nano: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            attributes: HashMap::new(),
        };
        
        self.emit_event(event).await
    }

    // ボリュームイベント生成ヘルパー
    #[allow(dead_code)]
    pub async fn emit_volume_event(&self, action: &str, volume_name: &str, driver: &str) -> Result<()> {
        let mut attributes = HashMap::new();
        attributes.insert("driver".to_string(), driver.to_string());
        
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Volume,
            action: action.to_string(),
            actor: Actor {
                id: volume_name.to_string(),
                attributes,
            },
            scope: "local".to_string(),
            time_nano: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            attributes: HashMap::new(),
        };
        
        self.emit_event(event).await
    }

    // ネットワークイベント生成ヘルパー
    #[allow(dead_code)]
    pub async fn emit_network_event(&self, action: &str, network_id: &str, network_name: &str, driver: &str) -> Result<()> {
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), network_name.to_string());
        attributes.insert("type".to_string(), driver.to_string());
        
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Network,
            action: action.to_string(),
            actor: Actor {
                id: network_id.to_string(),
                attributes,
            },
            scope: "local".to_string(),
            time_nano: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            attributes: HashMap::new(),
        };
        
        self.emit_event(event).await
    }

    // デーモンイベント生成ヘルパー
    pub async fn emit_daemon_event(&self, action: &str, message: &str) -> Result<()> {
        let mut attributes = HashMap::new();
        attributes.insert("message".to_string(), message.to_string());
        
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: EventType::Daemon,
            action: action.to_string(),
            actor: Actor {
                id: "nexusd".to_string(),
                attributes,
            },
            scope: "local".to_string(),
            time_nano: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            attributes: HashMap::new(),
        };
        
        self.emit_event(event).await
    }

    // 統計情報取得
    #[allow(dead_code)]
    pub async fn get_event_stats(&self) -> HashMap<String, serde_json::Value> {
        let events = self.events.read().await;
        let mut stats = HashMap::new();
        
        stats.insert("total_events".to_string(), serde_json::json!(events.len()));
        
        // イベントタイプ別カウント
        let mut type_counts = HashMap::new();
        for event in events.iter() {
            let type_name = format!("{:?}", event.event_type);
            *type_counts.entry(type_name).or_insert(0) += 1;
        }
        stats.insert("events_by_type".to_string(), serde_json::to_value(type_counts).unwrap());
        
        // アクション別カウント
        let mut action_counts = HashMap::new();
        for event in events.iter() {
            *action_counts.entry(event.action.clone()).or_insert(0) += 1;
        }
        stats.insert("events_by_action".to_string(), serde_json::to_value(action_counts).unwrap());
        
        // 最新イベントのタイムスタンプ
        if let Some(latest) = events.back() {
            stats.insert("latest_event_time".to_string(), serde_json::json!(latest.timestamp));
        }
        
        // 最古イベントのタイムスタンプ
        if let Some(oldest) = events.front() {
            stats.insert("oldest_event_time".to_string(), serde_json::json!(oldest.timestamp));
        }
        
        stats
    }

    // イベントエクスポート
    #[allow(dead_code)]
    pub async fn export_events(&self, filter: EventFilter, format: &str) -> Result<String> {
        let events = self.get_events(filter).await?;
        
        match format.to_lowercase().as_str() {
            "json" => {
                Ok(serde_json::to_string_pretty(&events)?)
            }
            "csv" => {
                let mut csv = String::new();
                csv.push_str("timestamp,type,action,actor_id,scope\n");
                
                for event in events {
                    csv.push_str(&format!(
                        "{},{:?},{},{},{}\n",
                        event.timestamp.to_rfc3339(),
                        event.event_type,
                        event.action,
                        event.actor.id,
                        event.scope
                    ));
                }
                
                Ok(csv)
            }
            _ => Err(anyhow::anyhow!("Unsupported export format: {}", format)),
        }
    }

    // 設定変更
    #[allow(dead_code)]
    pub fn set_max_events(&mut self, max_events: usize) {
        self.max_events = max_events;
        info!("Max events set to: {}", max_events);
    }

    #[allow(dead_code)]
    pub fn set_retention_duration(&mut self, duration: Duration) {
        self.retention_duration = duration;
        info!("Event retention duration set to: {:?}", duration);
    }

    // イベント検索
    #[allow(dead_code)]
    pub async fn search_events(&self, query: &str) -> Result<Vec<Event>> {
        let events = self.events.read().await;
        let query_lower = query.to_lowercase();
        
        let filtered_events: Vec<Event> = events
            .iter()
            .filter(|event| {
                event.action.to_lowercase().contains(&query_lower) ||
                event.actor.id.to_lowercase().contains(&query_lower) ||
                event.actor.attributes.values().any(|v| v.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect();
        
        Ok(filtered_events)
    }

    // イベントクリア
    #[allow(dead_code)]
    pub async fn clear_events(&self) -> Result<()> {
        let mut events = self.events.write().await;
        events.clear();
        info!("All events cleared");
        Ok(())
    }

    // イベント数取得
    #[allow(dead_code)]
    pub async fn get_event_count(&self) -> usize {
        let events = self.events.read().await;
        events.len()
    }
} 