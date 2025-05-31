use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// ボリューム情報
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mount_point: PathBuf,
    pub created: SystemTime,
    pub labels: HashMap<String, String>,
    #[allow(dead_code)]
    pub status: VolumeStatus,
    pub size: u64,
}

impl VolumeInfo {
    pub fn new(name: String, driver: String, mount_point: PathBuf) -> Self {
        Self {
            name,
            driver,
            mount_point,
            created: SystemTime::now(),
            labels: HashMap::new(),
            status: VolumeStatus::Available,
            size: 0,
        }
    }
}

/// ボリュームスコープ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum VolumeScope {
    Local,
    Global,
}

/// ボリューム状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolumeStatus {
    Available,
    InUse,
    Error,
}

/// ボリューム設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct VolumeConfig {
    pub name: String,
    pub driver: String,
    pub driver_opts: HashMap<String, String>,
    pub labels: HashMap<String, String>,
}

/// ボリュームヘルパー
#[allow(dead_code)]
pub struct VolumeHelper {
    root_path: PathBuf,
}

impl VolumeHelper {
    #[allow(dead_code)]
    pub fn new<P: AsRef<Path>>(root_path: P) -> Self {
        Self {
            root_path: root_path.as_ref().to_path_buf(),
        }
    }
    
    /// ボリューム名の検証
    #[allow(dead_code)]
    pub fn validate_volume_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow::anyhow!("Volume name cannot be empty"));
        }
        
        if name.len() > 64 {
            return Err(anyhow::anyhow!("Volume name too long (max 64 characters)"));
        }
        
        // 英数字、ハイフン、アンダースコア、ピリオドのみ許可
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(anyhow::anyhow!("Volume name can only contain alphanumeric characters, hyphens, underscores, and periods"));
        }
        
        // 先頭と末尾は英数字のみ
        if let (Some(first), Some(last)) = (name.chars().next(), name.chars().last()) {
            if !first.is_alphanumeric() || !last.is_alphanumeric() {
                return Err(anyhow::anyhow!("Volume name must start and end with an alphanumeric character"));
            }
        }
        
        // 予約語チェック
        let reserved_names = [".", "..", "none", "local"];
        if reserved_names.contains(&name) {
            return Err(anyhow::anyhow!("Volume name '{}' is reserved", name));
        }
        
        Ok(())
    }
    
    /// ボリュームパスを取得
    #[allow(dead_code)]
    pub fn get_volume_path(&self, name: &str) -> PathBuf {
        self.root_path.join("volumes").join(name)
    }
    
    /// ボリュームメタデータパスを取得
    #[allow(dead_code)]
    pub fn get_volume_metadata_path(&self, name: &str) -> PathBuf {
        self.get_volume_path(name).join("_metadata.json")
    }
    
    /// ボリュームを作成
    #[allow(dead_code)]
    pub fn create_volume(&self, config: &VolumeConfig) -> Result<VolumeInfo> {
        Self::validate_volume_name(&config.name)?;
        
        let volume_path = self.get_volume_path(&config.name);
        
        if volume_path.exists() {
            return Err(anyhow::anyhow!("Volume '{}' already exists", config.name));
        }
        
        // ボリュームディレクトリを作成
        fs::create_dir_all(&volume_path)?;
        
        let mut volume_info = VolumeInfo::new(
            config.name.clone(),
            config.driver.clone(),
            volume_path.clone(),
        );
        
        volume_info.labels = config.labels.clone();
        
        // メタデータを保存
        self.save_volume_metadata(&volume_info)?;
        
        Ok(volume_info)
    }
    
    /// ボリュームを削除
    #[allow(dead_code)]
    pub fn remove_volume(&self, name: &str, force: bool) -> Result<()> {
        let volume_path = self.get_volume_path(name);
        
        if !volume_path.exists() {
            return Err(anyhow::anyhow!("Volume '{}' not found", name));
        }
        
        // 使用中チェック（簡易実装）
        if !force {
            let volume_info = self.get_volume_info(name)?;
            if matches!(volume_info.status, VolumeStatus::InUse) {
                return Err(anyhow::anyhow!("Volume '{}' is in use", name));
            }
        }
        
        // ボリュームディレクトリを削除
        fs::remove_dir_all(&volume_path)?;
        
        Ok(())
    }
    
    /// ボリューム情報を取得
    #[allow(dead_code)]
    pub fn get_volume_info(&self, name: &str) -> Result<VolumeInfo> {
        let metadata_path = self.get_volume_metadata_path(name);
        
        if !metadata_path.exists() {
            return Err(anyhow::anyhow!("Volume '{}' not found", name));
        }
        
        self.load_volume_metadata(&metadata_path)
    }
    
    /// ボリューム一覧を取得
    #[allow(dead_code)]
    pub fn list_volumes(&self) -> Result<Vec<VolumeInfo>> {
        let volumes_dir = self.root_path.join("volumes");
        
        if !volumes_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut volumes = Vec::new();
        
        for entry in fs::read_dir(volumes_dir)? {
            let entry = entry?;
            let metadata_path = entry.path().join("_metadata.json");
            
            if metadata_path.exists() {
                if let Ok(volume_info) = self.load_volume_metadata(&metadata_path) {
                    volumes.push(volume_info);
                }
            }
        }
        
        Ok(volumes)
    }
    
    /// ボリュームサイズを計算
    #[allow(dead_code)]
    pub fn calculate_volume_size(&self, name: &str) -> Result<u64> {
        let volume_path = self.get_volume_path(name);
        
        if !volume_path.exists() {
            return Err(anyhow::anyhow!("Volume '{}' not found", name));
        }
        
        Self::calculate_directory_size(&volume_path)
    }
    
    /// 未使用ボリュームを検索
    #[allow(dead_code)]
    pub fn find_unused_volumes(&self) -> Result<Vec<VolumeInfo>> {
        let volumes = self.list_volumes()?;
        Ok(volumes.into_iter()
            .filter(|v| matches!(v.status, VolumeStatus::Available))
            .collect())
    }
    
    /// ボリュームをバックアップ
    #[allow(dead_code)]
    pub fn backup_volume(&self, name: &str, backup_path: &Path) -> Result<()> {
        let volume_path = self.get_volume_path(name);
        
        if !volume_path.exists() {
            return Err(anyhow::anyhow!("Volume '{}' not found", name));
        }
        
        // バックアップディレクトリを作成
        if let Some(parent) = backup_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        Self::copy_directory(&volume_path, backup_path)?;
        
        Ok(())
    }
    
    /// ボリュームをリストア
    #[allow(dead_code)]
    pub fn restore_volume(&self, name: &str, backup_path: &Path) -> Result<()> {
        if !backup_path.exists() {
            return Err(anyhow::anyhow!("Backup path does not exist"));
        }
        
        let volume_path = self.get_volume_path(name);
        
        // 既存のボリュームがある場合は削除
        if volume_path.exists() {
            fs::remove_dir_all(&volume_path)?;
        }
        
        // ボリュームディレクトリを作成
        fs::create_dir_all(&volume_path)?;
        
        Self::copy_directory(backup_path, &volume_path)?;
        
        Ok(())
    }
    
    /// ボリュームメタデータを保存
    #[allow(dead_code)]
    fn save_volume_metadata(&self, info: &VolumeInfo) -> Result<()> {
        let metadata_path = self.get_volume_metadata_path(&info.name);
        
        let json_data = serde_json::to_string_pretty(info)?;
        fs::write(metadata_path, json_data)?;
        
        Ok(())
    }
    
    /// ボリュームメタデータを読み込み
    #[allow(dead_code)]
    fn load_volume_metadata(&self, metadata_path: &Path) -> Result<VolumeInfo> {
        let json_data = fs::read_to_string(metadata_path)?;
        let volume_info: VolumeInfo = serde_json::from_str(&json_data)?;
        Ok(volume_info)
    }
    
    /// ディレクトリサイズを計算
    #[allow(dead_code)]
    fn calculate_directory_size(dir: &Path) -> Result<u64> {
        let mut total_size = 0;
        
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            
            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += Self::calculate_directory_size(&entry.path())?;
            }
        }
        
        Ok(total_size)
    }
    
    /// ディレクトリをコピー
    #[allow(dead_code)]
    fn copy_directory(src: &Path, dst: &Path) -> Result<()> {
        if !src.exists() {
            return Err(anyhow::anyhow!("Source directory does not exist"));
        }
        
        fs::create_dir_all(dst)?;
        
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            
            if src_path.is_file() {
                fs::copy(&src_path, &dst_path)?;
            } else if src_path.is_dir() {
                Self::copy_directory(&src_path, &dst_path)?;
            }
        }
        
        Ok(())
    }
}

/// ボリュームマウント情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VolumeMount {
    pub source: String,
    pub target: String,
    pub mount_type: VolumeMountType,
    pub read_only: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum VolumeMountType {
    #[allow(dead_code)]
    Volume,
    #[allow(dead_code)]
    Bind,
    #[allow(dead_code)]
    Tmpfs,
}

impl VolumeMount {
    /// マウント文字列をパース
    #[allow(dead_code)]
    pub fn parse(mount_str: &str) -> Result<Self> {
        let parts: Vec<&str> = mount_str.split(':').collect();
        
        match parts.len() {
            2 => {
                // source:target
                Ok(Self {
                    source: parts[0].to_string(),
                    target: parts[1].to_string(),
                    mount_type: VolumeMountType::Volume,
                    read_only: false,
                })
            }
            3 => {
                // source:target:options
                let read_only = parts[2].contains("ro");
                Ok(Self {
                    source: parts[0].to_string(),
                    target: parts[1].to_string(),
                    mount_type: VolumeMountType::Volume,
                    read_only,
                })
            }
            _ => Err(anyhow::anyhow!("Invalid mount format: {}", mount_str)),
        }
    }
    
    #[allow(dead_code)]
    pub fn to_string(&self) -> String {
        let mut result = format!("{}:{}", self.source, self.target);
        
        if self.read_only {
            result.push_str(":ro");
        }
        
        result
    }
}

impl fmt::Display for VolumeMount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = format!("{}:{}", self.source, self.target);
        
        if self.read_only {
            result.push_str(":ro");
        }
        
        write!(f, "{}", result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_validate_volume_name() {
        assert!(VolumeHelper::validate_volume_name("my-volume").is_ok());
        assert!(VolumeHelper::validate_volume_name("volume_1").is_ok());
        assert!(VolumeHelper::validate_volume_name("data.vol").is_ok());
        
        assert!(VolumeHelper::validate_volume_name("").is_err());
        assert!(VolumeHelper::validate_volume_name("-invalid").is_err());
        assert!(VolumeHelper::validate_volume_name("invalid-").is_err());
        assert!(VolumeHelper::validate_volume_name("with spaces").is_err());
        assert!(VolumeHelper::validate_volume_name("none").is_err());
    }
    
    #[test]
    fn test_volume_mount_parse() {
        let mount = VolumeMount::parse("data:/app/data").unwrap();
        assert_eq!(mount.source, "data");
        assert_eq!(mount.target, "/app/data");
        assert!(!mount.read_only);
        
        let mount = VolumeMount::parse("data:/app/data:ro").unwrap();
        assert_eq!(mount.source, "data");
        assert_eq!(mount.target, "/app/data");
        assert!(mount.read_only);
    }
    
    #[test]
    fn test_volume_helper() {
        let temp_dir = TempDir::new().unwrap();
        let helper = VolumeHelper::new(temp_dir.path());
        
        let config = VolumeConfig {
            name: "test-volume".to_string(),
            driver: "local".to_string(),
            driver_opts: HashMap::new(),
            labels: HashMap::new(),
        };
        
        let volume_info = helper.create_volume(&config).unwrap();
        assert_eq!(volume_info.name, "test-volume");
        assert_eq!(volume_info.driver, "local");
        
        let volumes = helper.list_volumes().unwrap();
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].name, "test-volume");
        
        helper.remove_volume("test-volume", false).unwrap();
        let volumes = helper.list_volumes().unwrap();
        assert_eq!(volumes.len(), 0);
    }
} 