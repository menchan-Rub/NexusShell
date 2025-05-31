use crate::oci::{OCISpec, OCIRoot, OCIUser, OCIPlatform, OCILinux, OCINamespace};
use anyhow::Result;
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct SpecGenerator {
    spec: OCISpec,
}

impl SpecGenerator {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            spec: OCISpec::default(),
        }
    }
    
    #[allow(dead_code)]
    pub fn set_rootfs<P: AsRef<Path>>(mut self, path: P, readonly: bool) -> Self {
        self.spec.root = Some(OCIRoot {
            path: path.as_ref().to_string_lossy().to_string(),
            readonly,
        });
        self
    }
    
    #[allow(dead_code)]
    pub fn set_process(mut self, args: Vec<String>, env: Vec<String>, cwd: String) -> Self {
        if let Some(ref mut process) = self.spec.process {
            process.args = args;
            process.env = env;
            process.cwd = Some(cwd);
        }
        self
    }
    
    #[allow(dead_code)]
    pub fn set_user(mut self, uid: u32, gid: u32) -> Self {
        if let Some(ref mut process) = self.spec.process {
            process.user = Some(OCIUser {
                uid,
                gid,
                umask: None,
                additionalGids: None,
            });
        }
        self
    }
    
    #[allow(dead_code)]
    pub fn set_hostname(mut self, hostname: String) -> Self {
        self.spec.hostname = Some(hostname);
        self
    }
    
    #[allow(dead_code)]
    pub fn set_platform(mut self, os: String, arch: String) -> Self {
        self.spec.platform = Some(OCIPlatform {
            os,
            arch,
        });
        self
    }
    
    #[allow(dead_code)]
    pub fn set_namespaces(mut self, namespaces: Vec<String>) -> Self {
        if let Some(ref mut linux) = self.spec.linux {
            linux.namespaces = Some(namespaces.into_iter().map(|ns| OCINamespace {
                namespace_type: ns,
                path: None,
            }).collect());
        }
        self
    }
    
    #[allow(dead_code)]
    pub fn set_readonly_paths(mut self, paths: Vec<String>) -> Self {
        if let Some(ref mut linux) = self.spec.linux {
            linux.readonlyPaths = Some(paths);
        }
        self
    }
    
    #[allow(dead_code)]
    pub fn set_masked_paths(mut self, paths: Vec<String>) -> Self {
        if let Some(ref mut linux) = self.spec.linux {
            linux.maskedPaths = Some(paths);
        }
        self
    }
    
    #[allow(dead_code)]
    pub fn build(self) -> OCISpec {
        self.spec
    }
    
    #[allow(dead_code)]
    pub fn save_to_file<P: AsRef<Path>>(self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.spec)?;
        fs::write(path, json)?;
        Ok(())
    }
}

/// OCI仕様のバリデーター
#[allow(dead_code)]
pub struct SpecValidator;

impl SpecValidator {
    /// OCI仕様をバリデート
    #[allow(dead_code)]
    pub fn validate(spec: &OCISpec) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        
        // バージョンチェック
        if spec.ociVersion != "1.0.0" && !spec.ociVersion.starts_with("1.0.") {
            warnings.push(format!("Unsupported OCI version: {}", spec.ociVersion));
        }
        
        // ルートファイルシステムチェック
        if let Some(ref root) = spec.root {
            if root.path.is_empty() {
                warnings.push("Root path is empty".to_string());
            }
        } else {
            warnings.push("Root filesystem not specified".to_string());
        }
        
        // プロセス設定チェック
        if let Some(ref process) = spec.process {
            if process.args.is_empty() {
                warnings.push("Process args are empty".to_string());
            }
            
            if let Some(ref cwd) = process.cwd {
                if !cwd.starts_with('/') {
                    warnings.push("Working directory must be absolute path".to_string());
                }
            }
        } else {
            warnings.push("Process configuration not specified".to_string());
        }
        
        // Linux固有設定チェック
        if let Some(ref linux) = spec.linux {
            if let Some(ref namespaces) = linux.namespaces {
                let mut has_pid = false;
                let mut has_mount = false;
                
                for ns in namespaces {
                    match ns.namespace_type.as_str() {
                        "pid" => has_pid = true,
                        "mount" => has_mount = true,
                        "network" | "ipc" | "uts" | "user" | "cgroup" => {},
                        _ => warnings.push(format!("Unknown namespace type: {}", ns.namespace_type)),
                    }
                }
                
                if !has_pid {
                    warnings.push("PID namespace not specified".to_string());
                }
                if !has_mount {
                    warnings.push("Mount namespace not specified".to_string());
                }
            }
        }
        
        Ok(warnings)
    }
    
    /// ファイルからOCI仕様を読み込んでバリデート
    #[allow(dead_code)]
    pub fn validate_file<P: AsRef<Path>>(path: P) -> Result<(OCISpec, Vec<String>)> {
        let content = fs::read_to_string(path)?;
        let spec: OCISpec = serde_json::from_str(&content)?;
        let warnings = Self::validate(&spec)?;
        Ok((spec, warnings))
    }
}

/// バンドル管理
#[allow(dead_code)]
pub struct BundleManager {
    bundle_path: PathBuf,
}

impl BundleManager {
    #[allow(dead_code)]
    pub fn new<P: AsRef<Path>>(bundle_path: P) -> Self {
        Self {
            bundle_path: bundle_path.as_ref().to_path_buf(),
        }
    }
    
    /// バンドルディレクトリを作成
    #[allow(dead_code)]
    pub fn create_bundle(&self, spec: &OCISpec) -> Result<()> {
        // バンドルディレクトリを作成
        fs::create_dir_all(&self.bundle_path)?;
        
        // config.jsonを保存
        let config_path = self.bundle_path.join("config.json");
        let json = serde_json::to_string_pretty(spec)?;
        fs::write(config_path, json)?;
        
        // rootfsディレクトリを作成
        if let Some(ref root) = spec.root {
            let rootfs_path = self.bundle_path.join(&root.path);
            fs::create_dir_all(rootfs_path)?;
        }
        
        Ok(())
    }
    
    /// config.jsonを読み込み
    #[allow(dead_code)]
    pub fn load_config(&self) -> Result<OCISpec> {
        let config_path = self.bundle_path.join("config.json");
        
        if !config_path.exists() {
            return Err(anyhow::anyhow!("config.json not found in bundle"));
        }
        
        let content = fs::read_to_string(config_path)?;
        let spec: OCISpec = serde_json::from_str(&content)?;
        Ok(spec)
    }
    
    /// rootfsパスを取得
    #[allow(dead_code)]
    pub fn get_rootfs_path(&self) -> Result<PathBuf> {
        let spec = self.load_config()?;
        let root_path = spec.root
            .ok_or_else(|| anyhow::anyhow!("Root not specified in config"))?
            .path;
        Ok(self.bundle_path.join(root_path))
    }
    
    /// バンドルの整合性をチェック
    #[allow(dead_code)]
    pub fn verify_bundle(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        
        // config.jsonの存在チェック
        let config_path = self.bundle_path.join("config.json");
        if !config_path.exists() {
            issues.push("config.json not found".to_string());
            return Ok(issues);
        }
        
        // config.jsonの妥当性チェック
        match self.load_config() {
            Ok(spec) => {
                // rootfsの存在チェック
                if let Some(ref root) = spec.root {
                    let rootfs_path = self.bundle_path.join(&root.path);
                    if !rootfs_path.exists() {
                        issues.push(format!("Rootfs directory not found: {}", root.path));
                    }
                }
                
                // 仕様のバリデート
                match SpecValidator::validate(&spec) {
                    Ok(warnings) => issues.extend(warnings),
                    Err(e) => issues.push(format!("Validation error: {}", e)),
                }
            }
            Err(e) => {
                issues.push(format!("Failed to parse config.json: {}", e));
            }
        }
        
        Ok(issues)
    }
    
    /// バンドルを削除
    #[allow(dead_code)]
    pub fn remove_bundle(&self) -> Result<()> {
        if self.bundle_path.exists() {
            fs::remove_dir_all(&self.bundle_path)?;
        }
        Ok(())
    }
}

/// デフォルトのOCI仕様を生成
#[allow(dead_code)]
pub fn create_default_spec() -> SpecGenerator {
    let mut generator = SpecGenerator::new();
    
    // デフォルト設定
    generator.spec.ociVersion = "1.0.0".to_string();
    generator.spec.hostname = Some("container".to_string());
    
    // デフォルトプラットフォーム
    generator.spec.platform = Some(OCIPlatform {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    });
    
    // デフォルトLinux設定
    generator.spec.linux = Some(OCILinux::default());
    
    generator
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_spec_generator() {
        let spec = SpecGenerator::new()
            .set_rootfs("/rootfs", false)
            .set_process(
                vec!["sh".to_string()],
                vec!["PATH=/usr/bin".to_string()],
                "/".to_string()
            )
            .set_user(1000, 1000)
            .set_hostname("test-container".to_string())
            .build();
        
        assert_eq!(spec.hostname, Some("test-container".to_string()));
        assert!(spec.root.is_some());
        assert!(spec.process.is_some());
    }
    
    #[test]
    fn test_bundle_manager() {
        let temp_dir = TempDir::new().unwrap();
        let bundle_path = temp_dir.path().join("test-bundle");
        
        let manager = BundleManager::new(&bundle_path);
        let spec = create_default_spec().build();
        
        manager.create_bundle(&spec).unwrap();
        
        assert!(bundle_path.join("config.json").exists());
        
        let loaded_spec = manager.load_config().unwrap();
        assert_eq!(loaded_spec.ociVersion, spec.ociVersion);
    }
    
    #[test]
    fn test_spec_validator() {
        let spec = create_default_spec().build();
        let warnings = SpecValidator::validate(&spec).unwrap();
        
        // デフォルト仕様では警告があるかもしれない
        println!("Validation warnings: {:?}", warnings);
    }
} 