#![allow(dead_code)] // 初期開発段階では未使用コードを許可
#![allow(unused_variables)] // TODO: remove later

// Module declarations
pub mod child_process;
pub mod errors;
pub mod ipc;
pub mod sandbox;
pub mod utils;

// フェーズ0: 基盤技術の拡張
pub mod container;
pub mod image;
pub mod network;
pub mod storage;
pub mod security;
pub mod cgroup;
pub mod namespace;
pub mod oci;
pub mod registry;

// Re-export key components for users of the library
pub use errors::{ContainerError, Result};
pub use sandbox::{run_container, SandboxConfig, Sandbox};
pub use container::{Container, ContainerState, ContainerStatus};
pub use image::{ImageManager, ImageInfo, ImageRegistry};
pub use network::{NetworkConfig, NetworkMode};
pub use storage::{StorageDriver, VolumeConfig};
pub use security::{SecurityPolicy, SeccompProfile, CapabilitySet};
pub use cgroup::{CgroupConfig, ResourceLimits};
pub use namespace::{NamespaceConfig, UserMapping};
pub use oci::{OCISpec, OCIImage, OCILayer, OCIManifest};
pub use registry::{RegistryClient, RegistryAuth};

// All the specific container logic (structs, functions, old tests)
// has been moved to errors.rs or sandbox.rs.
// lib.rs is now primarily for module organization and re-exports.

#[cfg(test)]
mod tests {
    use super::*; // Imports items from lib.rs (e.g., re-exported items)
    use std::path::PathBuf; // Ensure PathBuf is in scope for tests

    #[test]
    fn lib_level_config_creation_test() {
        // Test that a simple ContainerConfig can be created via re-exported types
        let rootfs_path = PathBuf::from("/tmp/lib_test_rootfs");
        let config = ContainerConfig::new_simple(
            "lib-test-container".to_string(),
            "/bin/true".to_string(),
            Vec::new(),
            rootfs_path.clone(),
        );
        assert_eq!(config.hostname, "lib-test-container");
        assert_eq!(config.command, "/bin/true");
        assert_eq!(config.rootfs, rootfs_path);
    }

    // Add other high-level library integration tests here if necessary.
    // For example, testing that `run_container` can be called, though
    // its detailed functional tests are in `sandbox.rs`.
}

pub mod config;
pub mod volume;
pub mod runtime;

// プラットフォーム固有のモジュール
#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows; 