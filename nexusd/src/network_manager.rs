use anyhow::Result;
use crate::daemon::NetworkStats;

#[derive(Debug)]
pub struct NetworkManager {
    // 実装は後で追加
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn initialize(&self) -> Result<()> {
        Ok(())
    }
    
    #[allow(dead_code)]
    pub async fn get_stats(&self) -> Result<NetworkStats> {
        Ok(NetworkStats {
            total: 0,
            active: 0,
        })
    }
} 