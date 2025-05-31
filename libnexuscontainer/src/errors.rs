use thiserror::Error;
use std::path::PathBuf;

// Unix固有のインポートを条件付きに
#[cfg(unix)]
use nix::sys::wait::WaitStatus;

/// NexusContainerのメインエラー型
#[derive(Error, Debug)]
pub enum ContainerError {
    #[error("Failed to clone process: {0}")]
    CloneError(String),
    
    // Unix固有のエラー
    #[cfg(unix)]
    #[error("Hostname setup failed: {0}")]
    HostnameError(String),
    #[cfg(unix)]
    #[error("Mount setup failed for {path:?}: {0}")]
    MountError{ path: PathBuf, message: String },
    #[cfg(unix)]
    #[error("Root filesystem setup failed (chroot): {0}")]
    ChrootError(String),
    #[cfg(unix)]
    #[error("Change directory failed for {path:?}: {0}")]
    ChdirError{ path: PathBuf, message: String },
    #[error("PivotRoot setup failed: {0}")]
    PivotRootError(String),
    #[cfg(unix)]
    #[error("Unshare failed: {0}")]
    UnshareError(String),
    #[cfg(unix)]
    #[error("Process execution failed for {command}: {0}")]
    ExecError{ command: String, message: String },
    #[error("CString conversion failed for {original}: {source}")]
    CStringError{ original: String, source: std::ffi::NulError },
    #[error("Capability setup failed: {0}")]
    CapabilityError(String),
    #[error("Namespace setup failed: {0}")]
    NamespaceError(String),
    #[error("User Namespace setup failed: {0}")]
    UserNamespaceSetupError(String),
    #[error("Cgroup setup failed: {0}")]
    CgroupError(String),
    #[error("Seccomp setup failed: {0}")]
    SeccompError(String),
    #[cfg(unix)]
    #[error("Child process wait failed: {0}")]
    WaitPidError(String),
    #[cfg(unix)]
    #[error("Child process exited with error: {0}")]
    ChildExited(String),
    #[cfg(unix)]
    #[error("Pipe creation failed: {0}")]
    PipeError(String),
    #[error("File operation error for {path:?}: {source}")]
    FileIOError{ path: PathBuf, source: std::io::Error },
    #[error("Path conversion error for {path:?}")]
    PathConversionError{ path: PathBuf },
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Sandbox error: {0}")]
    Sandbox(String),
    #[error("IPC error: {0}")]
    Ipc(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("System call error: {0}")]
    Syscall(String),
    #[error("Permission denied: {0}")]
    Permission(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Container lifecycle error: {0}")]
    InvalidState(String),
    #[error("Process spawn error: {0}")]
    ProcessSpawn(String),
    #[error("Process wait error: {0}")]
    ProcessWait(String),
    #[error("Signal error: {0}")]
    Signal(String),
    #[error("Namespace error: {0}")]
    Namespace(String),
    #[error("Cgroup error: {0}")]
    Cgroup(String),
    #[error("Security error: {0}")]
    Security(String),
    #[error("Image error: {0}")]
    Image(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Mount error: {0}")]
    Mount(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),
    #[error("Dependency error: {0}")]
    Dependency(String),
    #[error("Kernel feature not supported: {0}")]
    UnsupportedFeature(String),
    #[error("Insufficient privileges: {0}")]
    InsufficientPrivileges(String),
    #[error("Resource unavailable: {0}")]
    ResourceUnavailable(String),
    #[error("Operation interrupted: {0}")]
    Interrupted(String),
    #[error("Would block: {0}")]
    WouldBlock(String),
    #[error("Invalid digest: {0}")]
    InvalidDigest(String),
    #[error("Authentication error: {0}")]
    Authentication(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Result型のエイリアス
pub type Result<T> = std::result::Result<T, ContainerError>;

impl ContainerError {
    /// エラーが回復可能かどうかを判定
    pub fn is_recoverable(&self) -> bool {
        match self {
            ContainerError::Timeout(_)
            | ContainerError::WouldBlock(_)
            | ContainerError::Interrupted(_)
            | ContainerError::ResourceUnavailable(_) => true,
            
            ContainerError::InvalidArgument(_)
            | ContainerError::NotFound(_)
            | ContainerError::Permission(_)
            | ContainerError::UnsupportedFeature(_)
            | ContainerError::InsufficientPrivileges(_) => false,
            
            _ => false, // デフォルトは非回復可能
        }
    }
    
    /// エラーが一時的なものかどうかを判定
    pub fn is_temporary(&self) -> bool {
        matches!(self, ContainerError::Timeout(_)
            | ContainerError::ResourceUnavailable(_)
            | ContainerError::WouldBlock(_)
            | ContainerError::Interrupted(_))
    }
    
    /// エラーのカテゴリを取得
    pub fn category(&self) -> ErrorCategory {
        match self {
            ContainerError::Io(_) => ErrorCategory::Io,
            ContainerError::Sandbox(_) => ErrorCategory::Sandbox,
            ContainerError::Ipc(_) => ErrorCategory::Ipc,
            ContainerError::Config(_) => ErrorCategory::Configuration,
            ContainerError::Syscall(_) => ErrorCategory::System,
            ContainerError::Permission(_) | ContainerError::InsufficientPrivileges(_) => ErrorCategory::Permission,
            ContainerError::NotFound(_) => ErrorCategory::NotFound,
            ContainerError::AlreadyExists(_) => ErrorCategory::AlreadyExists,
            ContainerError::InvalidArgument(_) => ErrorCategory::InvalidArgument,
            ContainerError::Timeout(_) => ErrorCategory::Timeout,
            
            ContainerError::InvalidState(_) 
            | ContainerError::ProcessSpawn(_) 
            | ContainerError::ProcessWait(_) 
            | ContainerError::Signal(_) => ErrorCategory::Container,
            
            ContainerError::Namespace(_) 
            | ContainerError::Cgroup(_) 
            | ContainerError::Security(_) => ErrorCategory::Isolation,
            
            ContainerError::Image(_) 
            | ContainerError::Storage(_) 
            | ContainerError::Mount(_) => ErrorCategory::Storage,
            
            ContainerError::Network(_) => ErrorCategory::Network,
            ContainerError::Runtime(_) => ErrorCategory::Runtime,
            ContainerError::Api(_) => ErrorCategory::Api,
            ContainerError::Serialization(_) => ErrorCategory::Serialization,
            ContainerError::Validation(_) => ErrorCategory::Validation,
            ContainerError::ResourceLimit(_) => ErrorCategory::ResourceLimit,
            ContainerError::Dependency(_) => ErrorCategory::Dependency,
            ContainerError::InternalError(_) => ErrorCategory::Internal,
            ContainerError::UnsupportedFeature(_) => ErrorCategory::UnsupportedFeature,
            ContainerError::ResourceUnavailable(_) => ErrorCategory::ResourceUnavailable,
            ContainerError::Interrupted(_) => ErrorCategory::Interrupted,
            ContainerError::WouldBlock(_) => ErrorCategory::WouldBlock,
            // Unix固有のエラー
            #[cfg(unix)]
            ContainerError::CloneError(_) 
            | ContainerError::HostnameError(_)
            | ContainerError::MountError { .. } 
            | ContainerError::ChrootError(_)
            | ContainerError::ChdirError { .. }
            | ContainerError::UnshareError(_)
            | ContainerError::ExecError { .. }
            | ContainerError::WaitPidError(_)
            | ContainerError::ChildExited(_)
            | ContainerError::PipeError(_) => ErrorCategory::System,
            #[cfg(not(unix))]
            ContainerError::CloneError(_) => ErrorCategory::System,
            ContainerError::PivotRootError(_) => ErrorCategory::System,
            ContainerError::CStringError { .. } => ErrorCategory::InvalidArgument,
            ContainerError::CapabilityError(_) => ErrorCategory::Security,
            ContainerError::NamespaceError(_) => ErrorCategory::Isolation,
            ContainerError::UserNamespaceSetupError(_) => ErrorCategory::Isolation,
            ContainerError::CgroupError(_) => ErrorCategory::Isolation,
            ContainerError::SeccompError(_) => ErrorCategory::Security,
            ContainerError::FileIOError { .. } => ErrorCategory::Io,
            ContainerError::PathConversionError { .. } => ErrorCategory::InvalidArgument,
            ContainerError::InvalidDigest(_) => ErrorCategory::InvalidArgument,
            ContainerError::Authentication(_) => ErrorCategory::Security,
            ContainerError::Unknown(_) => ErrorCategory::Internal,
        }
    }
    
    /// エラーの重要度を取得
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ContainerError::InternalError(_) 
            | ContainerError::Security(_)
            | ContainerError::ProcessSpawn(_) => ErrorSeverity::Critical,
            
            ContainerError::InvalidState(_)
            | ContainerError::Namespace(_)
            | ContainerError::Cgroup(_)
            | ContainerError::Storage(_)
            | ContainerError::Network(_)
            | ContainerError::Image(_) => ErrorSeverity::High,
            
            ContainerError::Config(_)
            | ContainerError::Validation(_)
            | ContainerError::InvalidArgument(_)
            | ContainerError::NotFound(_)
            | ContainerError::AlreadyExists(_) => ErrorSeverity::Medium,
            
            ContainerError::Timeout(_)
            | ContainerError::WouldBlock(_)
            | ContainerError::Interrupted(_) => ErrorSeverity::Low,
            
            _ => ErrorSeverity::Medium,
        }
    }
}

/// エラーのカテゴリ
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCategory {
    Io,
    Sandbox,
    Ipc,
    Configuration,
    System,
    Permission,
    NotFound,
    AlreadyExists,
    InvalidArgument,
    Timeout,
    Container,
    Isolation,
    Storage,
    Network,
    Runtime,
    Api,
    Serialization,
    Validation,
    ResourceLimit,
    Dependency,
    Internal,
    UnsupportedFeature,
    ResourceUnavailable,
    Interrupted,
    WouldBlock,
    Security,
}

/// エラーの重要度
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

// 標準エラーからの変換実装
impl From<std::io::Error> for ContainerError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => ContainerError::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => ContainerError::Permission(err.to_string()),
            std::io::ErrorKind::AlreadyExists => ContainerError::AlreadyExists(err.to_string()),
            std::io::ErrorKind::InvalidInput => ContainerError::InvalidArgument(err.to_string()),
            std::io::ErrorKind::TimedOut => ContainerError::Timeout(err.to_string()),
            std::io::ErrorKind::WouldBlock => ContainerError::WouldBlock(err.to_string()),
            std::io::ErrorKind::Interrupted => ContainerError::Interrupted(err.to_string()),
            _ => ContainerError::Io(err.to_string()),
        }
    }
}

// Unix固有のnix::Errorからの変換実装
#[cfg(unix)]
impl From<nix::Error> for ContainerError {
    fn from(err: nix::Error) -> Self {
        match err {
            nix::Error::EPERM => ContainerError::Permission("Operation not permitted".to_string()),
            nix::Error::ENOENT => ContainerError::NotFound("No such file or directory".to_string()),
            nix::Error::EEXIST => ContainerError::AlreadyExists("File exists".to_string()),
            nix::Error::EINVAL => ContainerError::InvalidArgument("Invalid argument".to_string()),
            nix::Error::EACCES => ContainerError::Permission("Permission denied".to_string()),
            nix::Error::EAGAIN => ContainerError::WouldBlock("Resource temporarily unavailable".to_string()),
            nix::Error::EINTR => ContainerError::Interrupted("Interrupted system call".to_string()),
            nix::Error::ENOTSUP => ContainerError::UnsupportedFeature("Operation not supported".to_string()),
            _ => ContainerError::Syscall(err.to_string()),
        }
    }
}

impl From<serde_json::Error> for ContainerError {
    fn from(err: serde_json::Error) -> Self {
        ContainerError::Serialization(err.to_string())
    }
}

/// ヘルパー関数群
pub mod helpers {
    use super::*;
    
    /// エラーメッセージの詳細化
    pub fn enhance_error_message(err: &ContainerError, context: &str) -> String {
        format!("{}: {}", context, err)
    }
    
    /// エラーチェインの構築
    pub fn error_chain(errors: Vec<ContainerError>) -> ContainerError {
        if errors.is_empty() {
            return ContainerError::InternalError("Empty error chain".to_string());
        }
        
        if errors.len() == 1 {
            return errors.into_iter().next().unwrap();
        }
        
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        ContainerError::InternalError(format!("Multiple errors: {}", messages.join("; ")))
    }
    
    /// エラーのフィルタリング
    pub fn filter_errors_by_category(errors: &[ContainerError], category: ErrorCategory) -> Vec<&ContainerError> {
        errors.iter().filter(|e| e.category() == category).collect()
    }
    
    /// 最も重要なエラーを取得
    pub fn most_severe_error(errors: &[ContainerError]) -> Option<&ContainerError> {
        errors.iter().max_by_key(|e| e.severity())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_categories() {
        let io_err = ContainerError::Io("test".to_string());
        assert_eq!(io_err.category(), ErrorCategory::Io);
        
        let namespace_err = ContainerError::Namespace("test".to_string());
        assert_eq!(namespace_err.category(), ErrorCategory::Isolation);
    }
    
    #[test]
    fn test_error_severity() {
        let security_err = ContainerError::Security("test".to_string());
        assert_eq!(security_err.severity(), ErrorSeverity::Critical);
        
        let timeout_err = ContainerError::Timeout("test".to_string());
        assert_eq!(timeout_err.severity(), ErrorSeverity::Low);
    }
    
    #[test]
    fn test_error_recoverability() {
        let timeout_err = ContainerError::Timeout("test".to_string());
        assert!(timeout_err.is_recoverable());
        
        let permission_err = ContainerError::Permission("test".to_string());
        assert!(!permission_err.is_recoverable());
    }
} 