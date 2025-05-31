use crate::errors::{ContainerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub name: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub readonly: bool,
    pub volume_type: VolumeType,
    pub mount_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VolumeType {
    Bind,
    Tmpfs { size: Option<u64> },
    Volume,
}

#[derive(Debug, Clone)]
pub struct StorageDriver {
    root_dir: PathBuf,
    driver_type: StorageDriverType,
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub enum StorageDriverType {
    #[default]
    OverlayFS,
    DeviceMapper,
    AUFS,
}

pub struct StorageManager {
    driver: StorageDriver,
    volumes: HashMap<String, VolumeConfig>,
}

impl StorageManager {
    pub fn new<P: AsRef<Path>>(root_dir: P, driver_type: StorageDriverType) -> Result<Self> {
        let root_dir = root_dir.as_ref().to_path_buf();
        
        // ストレージルートディレクトリの作成
        create_dir_all(&root_dir)
            .map_err(|e| ContainerError::Storage(format!("Failed to create storage root: {}", e)))?;
        
        let driver = StorageDriver {
            root_dir,
            driver_type,
        };
        
        Ok(Self {
            driver,
            volumes: HashMap::new(),
        })
    }
    
    /// コンテナのルートファイルシステムを準備
    pub fn prepare_rootfs(&self, container_id: &str, image_layers: &[&Path]) -> Result<PathBuf> {
        match &self.driver.driver_type {
            StorageDriverType::OverlayFS => self.prepare_overlayfs_rootfs(container_id, image_layers),
            StorageDriverType::DeviceMapper => {
                // DeviceMapperの実装（将来的に追加）
                Err(ContainerError::Storage("DeviceMapper not implemented".to_string()))
            }
            StorageDriverType::AUFS => {
                // AUFSの実装（将来的に追加）
                Err(ContainerError::Storage("AUFS not implemented".to_string()))
            }
        }
    }
    
    /// OverlayFSを使用したルートファイルシステムの準備
    fn prepare_overlayfs_rootfs(&self, container_id: &str, image_layers: &[&Path]) -> Result<PathBuf> {
        log::debug!("Preparing OverlayFS rootfs for container {}", container_id);
        
        #[cfg(not(unix))]
        {
            return Err(ContainerError::UnsupportedFeature("OverlayFS not supported on this platform".to_string()));
        }
        
        #[cfg(unix)]
        {
            let container_dir = self.driver.root_dir.join("containers").join(container_id);
            let lower_dir = container_dir.join("lower");
            let upper_dir = container_dir.join("upper");
            let work_dir = container_dir.join("work");
            let merged_dir = container_dir.join("merged");
            
            // 必要なディレクトリの作成
            create_dir_all(&lower_dir)
                .map_err(|e| ContainerError::Storage(format!("Failed to create lower dir: {}", e)))?;
            create_dir_all(&upper_dir)
                .map_err(|e| ContainerError::Storage(format!("Failed to create upper dir: {}", e)))?;
            create_dir_all(&work_dir)
                .map_err(|e| ContainerError::Storage(format!("Failed to create work dir: {}", e)))?;
            create_dir_all(&merged_dir)
                .map_err(|e| ContainerError::Storage(format!("Failed to create merged dir: {}", e)))?;
            
            // イメージレイヤーのシンボリックリンクを作成
            for (i, layer_path) in image_layers.iter().enumerate() {
                let link_path = lower_dir.join(format!("layer{}", i));
                if link_path.exists() {
                    std::fs::remove_file(&link_path)
                        .map_err(|e| ContainerError::Storage(format!("Failed to remove existing link: {}", e)))?;
                }
                
                std::os::unix::fs::symlink(layer_path, &link_path)
                    .map_err(|e| ContainerError::Storage(format!("Failed to create layer symlink: {}", e)))?;
            }
            
            // OverlayFSマウント
            self.mount_overlayfs(&lower_dir, &upper_dir, &work_dir, &merged_dir, image_layers)?;
            
            log::info!("OverlayFS rootfs prepared for container {}", container_id);
            Ok(merged_dir)
        }
        
        #[cfg(not(unix))]
        {
            Err(ContainerError::UnsupportedFeature("Symbolic links not supported on this platform".to_string()))
        }
    }
    
    /// OverlayFSをマウント
    fn mount_overlayfs(
        &self,
        lower_dir: &Path,
        upper_dir: &Path,
        work_dir: &Path,
        merged_dir: &Path,
        image_layers: &[&Path],
    ) -> Result<()> {
        #[cfg(unix)]
        {
            // lowerdirの文字列構築（複数レイヤーをコロンで区切り）
            let lowerdir = image_layers
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join(":");
            
            let mount_options = format!(
                "lowerdir={},upperdir={},workdir={}",
                lowerdir,
                upper_dir.to_string_lossy(),
                work_dir.to_string_lossy()
            );
            
            log::debug!("Mounting OverlayFS with options: {}", mount_options);
            
            mount(
                Some("overlay"),
                merged_dir,
                Some("overlay"),
                MsFlags::empty(),
                Some(mount_options.as_str()),
            ).map_err(|e| ContainerError::Storage(format!("Failed to mount OverlayFS: {}", e)))?;
            
            Ok(())
        }
        
        #[cfg(not(unix))]
        {
            Err(ContainerError::UnsupportedFeature("OverlayFS mounting not supported on this platform".to_string()))
        }
    }
    
    /// ボリュームを作成
    pub fn create_volume(&mut self, config: VolumeConfig) -> Result<()> {
        log::info!("Creating volume: {}", config.name);
        
        // ボリューム作成の準備
        match &config.volume_type {
            VolumeType::Bind => {
                // バインドマウントの場合、ソースディレクトリの存在確認
                if !config.source.exists() {
                    return Err(ContainerError::Storage(
                        format!("Bind mount source does not exist: {}", config.source.display())
                    ));
                }
            }
            VolumeType::Tmpfs { size: _ } => {
                // tmpfsの場合、特別な準備は不要
            }
            VolumeType::Volume => {
                // 名前付きボリュームの場合、ボリュームディレクトリを作成
                let volume_path = self.driver.root_dir.join("volumes").join(&config.name);
                create_dir_all(&volume_path)
                    .map_err(|e| ContainerError::Storage(format!("Failed to create volume directory: {}", e)))?;
            }
        }
        
        self.volumes.insert(config.name.clone(), config);
        Ok(())
    }
    
    /// ボリュームをマウント
    pub fn mount_volume(&self, volume_name: &str, target: &Path) -> Result<()> {
        let volume = self.volumes.get(volume_name)
            .ok_or_else(|| ContainerError::Storage(format!("Volume not found: {}", volume_name)))?;
        
        log::debug!("Mounting volume {} to {}", volume_name, target.display());
        
        // ターゲットディレクトリの作成
        if let Some(parent) = target.parent() {
            create_dir_all(parent)
                .map_err(|e| ContainerError::Storage(format!("Failed to create target parent directory: {}", e)))?;
        }
        
        #[cfg(unix)]
        {
            use nix::mount::{mount, MsFlags};
            
            match &volume.volume_type {
                VolumeType::Bind => {
                    let mut flags = MsFlags::MS_BIND;
                    if volume.readonly {
                        flags |= MsFlags::MS_RDONLY;
                    }
                    
                    mount(
                        Some(volume.source.as_path()),
                        target,
                        None::<&str>,
                        flags,
                        None::<&str>,
                    ).map_err(|e| ContainerError::Storage(format!("Failed to bind mount: {}", e)))?;
                }
                VolumeType::Tmpfs { size } => {
                    let mut options = String::new();
                    if let Some(size) = size {
                        options = format!("size={}", size);
                    }
                    
                    mount(
                        Some("tmpfs"),
                        target,
                        Some("tmpfs"),
                        MsFlags::empty(),
                        if options.is_empty() { None } else { Some(options.as_str()) },
                    ).map_err(|e| ContainerError::Storage(format!("Failed to mount tmpfs: {}", e)))?;
                }
                VolumeType::Volume => {
                    let volume_path = self.driver.root_dir.join("volumes").join(&volume.name);
                    let mut flags = MsFlags::MS_BIND;
                    if volume.readonly {
                        flags |= MsFlags::MS_RDONLY;
                    }
                    
                    mount(
                        Some(volume_path.as_path()),
                        target,
                        None::<&str>,
                        flags,
                        None::<&str>,
                    ).map_err(|e| ContainerError::Storage(format!("Failed to mount volume: {}", e)))?;
                }
            }
            
            log::debug!("Volume {} mounted successfully", volume_name);
            Ok(())
        }
        
        #[cfg(not(unix))]
        {
            Err(ContainerError::UnsupportedFeature("Volume mounting not supported on this platform".to_string()))
        }
    }
    
    /// 全てのボリュームをマウント
    pub fn mount_all_volumes(&self, container_rootfs: &Path) -> Result<()> {
        for (name, volume) in &self.volumes {
            let mount_point = container_rootfs.join(volume.target.strip_prefix("/").unwrap_or(&volume.target));
            self.mount_volume(name, &mount_point)?;
        }
        Ok(())
    }
    
    /// ボリュームをアンマウント
    pub fn umount_volume(&self, target: &Path) -> Result<()> {
        log::debug!("Unmounting volume at {}", target.display());
        
        #[cfg(unix)]
        {
            use nix::mount::umount;
            umount(target)
                .map_err(|e| ContainerError::Storage(format!("Failed to unmount volume: {}", e)))?;
            Ok(())
        }
        
        #[cfg(not(unix))]
        {
            Err(ContainerError::UnsupportedFeature("Volume unmounting not supported on this platform".to_string()))
        }
    }
    
    /// コンテナのストレージをクリーンアップ
    pub fn cleanup_container_storage(&self, container_id: &str) -> Result<()> {
        log::info!("Cleaning up storage for container {}", container_id);
        
        let container_dir = self.driver.root_dir.join("containers").join(container_id);
        
        if !container_dir.exists() {
            return Ok(());
        }
        
        // マージされたファイルシステムのアンマウント
        let merged_dir = container_dir.join("merged");
        if merged_dir.exists() {
            #[cfg(unix)]
            {
                if let Err(e) = umount(&merged_dir) {
                    log::warn!("Failed to unmount merged dir: {}", e);
                }
            }
        }
        
        // コンテナディレクトリの削除
        std::fs::remove_dir_all(&container_dir)
            .map_err(|e| ContainerError::Storage(format!("Failed to remove container directory: {}", e)))?;
        
        log::info!("Container storage cleaned up: {}", container_id);
        Ok(())
    }
    
    /// ボリュームを削除
    pub fn remove_volume(&mut self, volume_name: &str) -> Result<()> {
        if let Some(volume) = self.volumes.remove(volume_name) {
            log::info!("Removing volume: {}", volume_name);
            
            if let VolumeType::Volume = volume.volume_type {
                let volume_path = self.driver.root_dir.join("volumes").join(&volume.name);
                if volume_path.exists() {
                    std::fs::remove_dir_all(&volume_path)
                        .map_err(|e| ContainerError::Storage(format!("Failed to remove volume directory: {}", e)))?;
                }
            }
            
            log::info!("Volume removed: {}", volume_name);
        }
        
        Ok(())
    }
    
    /// ストレージ統計を取得
    pub fn get_storage_stats(&self) -> Result<StorageStats> {
        let mut stats = StorageStats::default();
        
        // ルートディレクトリのサイズ計算
        stats.total_size = self.calculate_directory_size(&self.driver.root_dir)?;
        
        // ボリューム数
        stats.volume_count = self.volumes.len();
        
        // ボリューム別のサイズ計算
        for (name, volume) in &self.volumes {
            if let VolumeType::Volume = volume.volume_type {
                let volume_path = self.driver.root_dir.join("volumes").join(&volume.name);
                if volume_path.exists() {
                    let size = self.calculate_directory_size(&volume_path)?;
                    stats.volume_sizes.insert(name.clone(), size);
                }
            }
        }
        
        Ok(stats)
    }
    
    /// ディレクトリサイズの計算
    fn calculate_directory_size(&self, path: &Path) -> Result<u64> {
        let mut total_size = 0;
        
        if !path.exists() {
            return Ok(0);
        }
        
        for entry in std::fs::read_dir(path)
            .map_err(|e| ContainerError::Storage(format!("Failed to read directory: {}", e)))? {
            
            let entry = entry
                .map_err(|e| ContainerError::Storage(format!("Failed to read directory entry: {}", e)))?;
            let metadata = entry.metadata()
                .map_err(|e| ContainerError::Storage(format!("Failed to get metadata: {}", e)))?;
            
            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += self.calculate_directory_size(&entry.path())?;
            }
        }
        
        Ok(total_size)
    }
    
    /// 読み取り専用バインドマウントの再マウント
    pub fn remount_readonly(&self, path: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            use nix::mount::{mount, MsFlags};
            mount(
                None::<&str>,
                path,
                None::<&str>,
                MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY | MsFlags::MS_BIND,
                None::<&str>,
            ).map_err(|e| ContainerError::Storage(format!("Failed to remount readonly: {}", e)))?;
            Ok(())
        }
        
        #[cfg(not(unix))]
        {
            Err(ContainerError::UnsupportedFeature("Remount operations not supported on this platform".to_string()))
        }
    }
}

#[derive(Debug, Default)]
pub struct StorageStats {
    pub total_size: u64,
    pub volume_count: usize,
    pub volume_sizes: HashMap<String, u64>,
}

impl VolumeConfig {
    pub fn new_bind_mount<P: AsRef<Path>>(name: String, source: P, target: P, readonly: bool) -> Self {
        Self {
            name,
            source: source.as_ref().to_path_buf(),
            target: target.as_ref().to_path_buf(),
            readonly,
            volume_type: VolumeType::Bind,
            mount_options: Vec::new(),
        }
    }
    
    pub fn new_tmpfs<P: AsRef<Path>>(name: String, target: P, size: Option<u64>) -> Self {
        Self {
            name,
            source: PathBuf::new(),
            target: target.as_ref().to_path_buf(),
            readonly: false,
            volume_type: VolumeType::Tmpfs { size },
            mount_options: Vec::new(),
        }
    }
    
    pub fn new_volume<P: AsRef<Path>>(name: String, target: P, readonly: bool) -> Self {
        Self {
            name: name.clone(),
            source: PathBuf::new(),
            target: target.as_ref().to_path_buf(),
            readonly,
            volume_type: VolumeType::Volume,
            mount_options: Vec::new(),
        }
    }
}

 