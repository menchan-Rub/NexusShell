use std::path::PathBuf;
use anyhow::Result;
use crate::daemon::ImageStats;

#[derive(Debug)]
pub struct ImageManager {
    data_root: PathBuf,
}

impl ImageManager {
    pub fn new(data_root: PathBuf) -> Self {
        Self { data_root }
    }

    pub async fn initialize(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.data_root).await?;
        Ok(())
    }
    
    #[allow(dead_code)]
    pub async fn get_stats(&self) -> Result<ImageStats> {
        Ok(ImageStats {
            total: 0,
            size: 0,
        })
    }
    
    pub async fn cleanup_unused_images(&self) -> Result<()> {
        Ok(())
    }
} 