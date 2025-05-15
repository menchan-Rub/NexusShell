use std::collections::{HashSet, HashMap};
use std::sync::Arc;
use uuid::Uuid;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use log::{debug, info, trace};

use super::job::JobResourceLimits;

/// ジョブグループ
/// 複数のジョブをまとめて管理するためのコンポーネント
#[derive(Clone)]
pub struct JobGroup {
    /// グループID
    id: String,
    /// グループ名
    name: String,
    /// グループ内のジョブID
    job_ids: Arc<RwLock<HashSet<String>>>,
    /// グループの作成時間
    created_at: Instant,
    /// グループのタグ
    tags: HashMap<String, String>,
    /// グループのリソース制限（グループ全体の制限）
    resource_limits: Option<JobResourceLimits>,
    /// グループの説明
    description: Option<String>,
    /// グループの優先度（0-100）
    priority: u8,
    /// グループの有効期限
    expiration: Option<Instant>,
}

impl JobGroup {
    /// 新しいジョブグループを作成します
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            job_ids: Arc::new(RwLock::new(HashSet::new())),
            created_at: Instant::now(),
            tags: HashMap::new(),
            resource_limits: None,
            description: None,
            priority: 50, // デフォルト優先度
            expiration: None,
        }
    }

    /// グループIDを取得します
    pub fn id(&self) -> &str {
        &self.id
    }

    /// グループ名を取得します
    pub fn name(&self) -> &str {
        &self.name
    }

    /// グループにジョブを追加します
    pub fn add_job(&mut self, job_id: String) {
        let job_ids = self.job_ids.clone();
        
        tokio::spawn(async move {
            let mut ids = job_ids.write().await;
            ids.insert(job_id.clone());
            debug!("ジョブ {} をグループに追加しました", job_id);
        });
    }

    /// グループからジョブを削除します
    pub fn remove_job(&mut self, job_id: &str) {
        let job_ids = self.job_ids.clone();
        let job_id_copy = job_id.to_string();
        
        tokio::spawn(async move {
            let mut ids = job_ids.write().await;
            ids.remove(&job_id_copy);
            debug!("ジョブ {} をグループから削除しました", job_id_copy);
        });
    }

    /// グループ内のジョブIDリストを取得します
    pub fn job_ids(&self) -> Vec<String> {
        let job_ids = self.job_ids.clone();
        
        // ブロッキング操作だが、一般的に短時間で完了するため許容
        let ids = match job_ids.try_read() {
            Ok(ids) => ids.clone(),
            Err(_) => HashSet::new(),
        };
        
        ids.into_iter().collect()
    }

    /// グループ内のジョブ数を取得します
    pub async fn job_count(&self) -> usize {
        let ids = self.job_ids.read().await;
        ids.len()
    }

    /// グループにタグを設定します
    pub fn set_tag(&mut self, key: &str, value: &str) {
        self.tags.insert(key.to_string(), value.to_string());
    }

    /// グループからタグを削除します
    pub fn remove_tag(&mut self, key: &str) {
        self.tags.remove(key);
    }

    /// グループのタグを取得します
    pub fn tags(&self) -> &HashMap<String, String> {
        &self.tags
    }

    /// グループのリソース制限を設定します
    pub fn set_resource_limits(&mut self, limits: JobResourceLimits) {
        self.resource_limits = Some(limits);
    }

    /// グループのリソース制限を取得します
    pub fn resource_limits(&self) -> Option<&JobResourceLimits> {
        self.resource_limits.as_ref()
    }

    /// グループの説明を設定します
    pub fn set_description(&mut self, description: &str) {
        self.description = Some(description.to_string());
    }

    /// グループの説明を取得します
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// グループの優先度を設定します（0-100）
    pub fn set_priority(&mut self, priority: u8) {
        self.priority = priority.min(100);
    }

    /// グループの優先度を取得します
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// グループの有効期限を設定します
    pub fn set_expiration(&mut self, duration: Duration) {
        self.expiration = Some(Instant::now() + duration);
    }

    /// グループの有効期限を取得します
    pub fn expiration(&self) -> Option<Instant> {
        self.expiration
    }

    /// グループが期限切れかどうかをチェックします
    pub fn is_expired(&self) -> bool {
        if let Some(expiration) = self.expiration {
            Instant::now() > expiration
        } else {
            false
        }
    }

    /// グループの作成時間を取得します
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// グループの経過時間を取得します
    pub fn elapsed(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// グループの情報を文字列として取得します
    pub fn to_string(&self) -> String {
        format!(
            "JobGroup {{ id: {}, name: {}, jobs: {}, priority: {}, created: {:?} ago }}",
            self.id,
            self.name,
            self.job_ids().len(),
            self.priority,
            self.created_at.elapsed()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_job_group_basic() {
        let mut group = JobGroup::new("test_group");
        assert_eq!(group.name(), "test_group");
        
        group.add_job("job1".to_string());
        group.add_job("job2".to_string());
        
        // 非同期処理の完了を待つ
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        assert_eq!(group.job_count().await, 2);
        
        let job_ids = group.job_ids();
        assert!(job_ids.contains(&"job1".to_string()));
        assert!(job_ids.contains(&"job2".to_string()));
        
        group.remove_job("job1");
        
        // 非同期処理の完了を待つ
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        assert_eq!(group.job_count().await, 1);
    }
    
    #[test]
    fn test_job_group_tags() {
        let mut group = JobGroup::new("test_group");
        
        group.set_tag("env", "production");
        group.set_tag("owner", "admin");
        
        assert_eq!(group.tags().get("env"), Some(&"production".to_string()));
        assert_eq!(group.tags().get("owner"), Some(&"admin".to_string()));
        
        group.remove_tag("env");
        assert_eq!(group.tags().get("env"), None);
    }
    
    #[test]
    fn test_job_group_expiration() {
        let mut group = JobGroup::new("test_group");
        
        assert!(!group.is_expired());
        
        group.set_expiration(Duration::from_millis(1));
        
        // 期限切れになるまで少し待つ
        std::thread::sleep(Duration::from_millis(10));
        
        assert!(group.is_expired());
    }
} 