use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;

/// イメージ名のパースヘルパー
#[derive(Debug, Clone)]
pub struct ImageName {
    pub registry: Option<String>,
    pub repository: String,
    pub tag: String,
}

impl ImageName {
    pub fn parse(name: &str) -> Result<Self> {
        let mut parts = name.split('/');
        let mut registry = None;
        let mut repository_and_tag = name;

        // レジストリが含まれているかチェック
        if name.contains('.') || name.contains(':') {
            if let Some(first_part) = parts.next() {
                if first_part.contains('.') || first_part.contains(':') {
                    registry = Some(first_part.to_string());
                    repository_and_tag = &name[first_part.len() + 1..];
                }
            }
        }

        // リポジトリとタグを分離
        let (repository, tag) = if let Some(colon_pos) = repository_and_tag.rfind(':') {
            let repo = &repository_and_tag[..colon_pos];
            let tag = &repository_and_tag[colon_pos + 1..];
            (repo.to_string(), tag.to_string())
        } else {
            (repository_and_tag.to_string(), "latest".to_string())
        };

        Ok(Self {
            registry,
            repository,
            tag,
        })
    }
    
    pub fn to_string(&self) -> String {
        let mut result = String::new();
        
        if let Some(ref registry) = self.registry {
            result.push_str(registry);
            result.push('/');
        }
        
        result.push_str(&self.repository);
        result.push(':');
        result.push_str(&self.tag);
        
        result
    }
    
    pub fn get_full_name(&self) -> String {
        self.to_string()
    }
    
    pub fn get_short_name(&self) -> String {
        format!("{}:{}", self.repository, self.tag)
    }
}

impl fmt::Display for ImageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::new();

        if let Some(ref registry) = self.registry {
            result.push_str(registry);
            result.push('/');
        }

        result.push_str(&self.repository);
        result.push(':');
        result.push_str(&self.tag);

        write!(f, "{}", result)
    }
}

/// イメージレイヤー情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImageLayer {
    pub digest: String,
    pub size: u64,
    pub media_type: String,
}

/// イメージマニフェスト情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImageManifest {
    pub schema_version: u32,
    pub media_type: String,
    pub config: ImageLayer,
    pub layers: Vec<ImageLayer>,
}

/// イメージ情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImageInfo {
    pub id: String,
    pub name: String,
    pub tag: String,
    pub digest: String,
    pub size: u64,
    pub created: std::time::SystemTime,
    pub manifest: Option<ImageManifest>,
}

/// イメージヘルパー
#[allow(dead_code)]
pub struct ImageHelper;

impl ImageHelper {
    /// イメージ名の正規化
    #[allow(dead_code)]
    pub fn normalize_image_name(image: &str) -> String {
        let parsed = ImageName::parse(image).unwrap();
        
        // デフォルトレジストリを追加
        let registry = parsed.registry.unwrap_or_else(|| "docker.io".to_string());
        
        // デフォルト名前空間を追加（docker.ioの場合）
        let namespace = if registry == "docker.io" && parsed.registry.is_none() {
            Some("library".to_string())
        } else {
            parsed.registry
        };
        
        ImageName {
            registry,
            repository: parsed.repository,
            tag: parsed.tag,
        }.to_string()
    }
    
    /// イメージタグの検証
    #[allow(dead_code)]
    pub fn validate_tag(tag: &str) -> Result<()> {
        if tag.is_empty() {
            return Err(anyhow::anyhow!("Tag cannot be empty"));
        }
        
        if tag.len() > 128 {
            return Err(anyhow::anyhow!("Tag too long (max 128 characters)"));
        }
        
        // 有効な文字のみ許可
        if !tag.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_') {
            return Err(anyhow::anyhow!("Tag contains invalid characters"));
        }
        
        // 先頭と末尾は英数字のみ
        if let (Some(first), Some(last)) = (tag.chars().next(), tag.chars().last()) {
            if !first.is_alphanumeric() || !last.is_alphanumeric() {
                return Err(anyhow::anyhow!("Tag must start and end with alphanumeric character"));
            }
        }
        
        Ok(())
    }
    
    /// レジストリURLの検証
    #[allow(dead_code)]
    pub fn validate_registry_url(url: &str) -> Result<()> {
        if url.is_empty() {
            return Err(anyhow::anyhow!("Registry URL cannot be empty"));
        }
        
        // 基本的なURL形式チェック
        if !url.contains('.') {
            return Err(anyhow::anyhow!("Invalid registry URL format"));
        }
        
        Ok(())
    }
    
    /// イメージサイズをフォーマット
    #[allow(dead_code)]
    pub fn format_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_index = 0;
        
        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }
        
        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_image_name_parsing() {
        let name = ImageName::parse("nginx:latest").unwrap();
        assert_eq!(name.repository, "nginx");
        assert_eq!(name.tag, "latest");
        assert!(name.registry.is_none());
        
        let name = ImageName::parse("library/nginx:1.20").unwrap();
        assert_eq!(name.repository, "nginx");
        assert_eq!(name.tag, "1.20");
        
        let name = ImageName::parse("docker.io/library/nginx:alpine").unwrap();
        assert_eq!(name.registry, Some("docker.io".to_string()));
        assert_eq!(name.repository, "nginx");
        assert_eq!(name.tag, "alpine");
    }
    
    #[test]
    fn test_image_name_normalization() {
        assert_eq!(
            ImageHelper::normalize_image_name("nginx"),
            "docker.io/library/nginx:latest"
        );
        
        assert_eq!(
            ImageHelper::normalize_image_name("nginx:1.20"),
            "docker.io/library/nginx:1.20"
        );
        
        assert_eq!(
            ImageHelper::normalize_image_name("myregistry.com/myapp:v1.0"),
            "myregistry.com/myapp:v1.0"
        );
    }
    
    #[test]
    fn test_tag_validation() {
        assert!(ImageHelper::validate_tag("latest").is_ok());
        assert!(ImageHelper::validate_tag("v1.0.0").is_ok());
        assert!(ImageHelper::validate_tag("alpine-3.14").is_ok());
        
        assert!(ImageHelper::validate_tag("").is_err());
        assert!(ImageHelper::validate_tag("-invalid").is_err());
        assert!(ImageHelper::validate_tag("invalid-").is_err());
        assert!(ImageHelper::validate_tag("with spaces").is_err());
    }
    
    #[test]
    fn test_size_formatting() {
        assert_eq!(ImageHelper::format_size(512), "512 B");
        assert_eq!(ImageHelper::format_size(1536), "1.5 KB");
        assert_eq!(ImageHelper::format_size(2097152), "2.0 MB");
        assert_eq!(ImageHelper::format_size(1073741824), "1.0 GB");
    }
} 