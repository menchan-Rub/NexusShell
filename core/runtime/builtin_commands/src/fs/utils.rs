/*!
# ファイルシステム操作コマンド用ユーティリティ

このモジュールは、ファイルシステム操作コマンドで共通して使用される
ユーティリティ関数やヘルパー機能を提供します。
*/

use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// ファイルの種類を表す列挙型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// 通常のファイル
    Regular,
    /// ディレクトリ
    Directory,
    /// シンボリックリンク
    SymbolicLink,
    /// ブロックデバイス
    BlockDevice,
    /// キャラクタデバイス
    CharDevice,
    /// FIFO（名前付きパイプ）
    Fifo,
    /// ソケット
    Socket,
    /// 不明なファイルタイプ
    Unknown,
}

/// ファイルパーミッションを表す構造体
#[derive(Debug, Clone)]
pub struct FilePermissions {
    /// 所有者の権限
    pub user: Permission,
    /// グループの権限
    pub group: Permission,
    /// その他のユーザーの権限
    pub other: Permission,
    /// セットユーザーID
    pub setuid: bool,
    /// セットグループID
    pub setgid: bool,
    /// スティッキービット
    pub sticky: bool,
}

/// 権限の種類
#[derive(Debug, Clone, Copy)]
pub struct Permission {
    /// 読み取り権限
    pub read: bool,
    /// 書き込み権限
    pub write: bool,
    /// 実行権限
    pub execute: bool,
}

/// ファイルの詳細情報
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// ファイル名
    pub name: String,
    /// ファイルパス
    pub path: PathBuf,
    /// ファイルのサイズ（バイト単位）
    pub size: u64,
    /// ファイルの種類
    pub file_type: FileType,
    /// ファイルの所有者ID
    pub uid: u32,
    /// ファイルのグループID
    pub gid: u32,
    /// ファイル作成時のタイムスタンプ（UNIXエポックからの秒数）
    pub created: u64,
    /// 最終アクセス時のタイムスタンプ（UNIXエポックからの秒数）
    pub accessed: u64,
    /// 最終変更時のタイムスタンプ（UNIXエポックからの秒数）
    pub modified: u64,
    /// ファイルのパーミッション
    pub permissions: FilePermissions,
    /// ハードリンクの数
    pub links: u64,
}

/// ファイルパスからファイルの詳細情報を取得
pub fn get_file_info(path: &Path) -> Result<FileInfo> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) => return Err(anyhow!("ファイル情報の取得に失敗しました: {}", err)),
    };
    
    // ファイル名を取得
    let name = path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            // ルートディレクトリなどの場合
            path.to_string_lossy().to_string()
        });
    
    // ファイルタイプを判定
    let file_type = if metadata.is_file() {
        FileType::Regular
    } else if metadata.is_dir() {
        FileType::Directory
    } else if metadata.file_type().is_symlink() {
        FileType::SymbolicLink
    } else {
        // UNIXプラットフォーム固有のファイルタイプ判定
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            let file_type = metadata.file_type();
            
            if file_type.is_block_device() {
                FileType::BlockDevice
            } else if file_type.is_char_device() {
                FileType::CharDevice
            } else if file_type.is_fifo() {
                FileType::Fifo
            } else if file_type.is_socket() {
                FileType::Socket
            } else {
                FileType::Unknown
            }
        }
        
        #[cfg(not(unix))]
        {
            FileType::Unknown
        }
    };
    
    // タイムスタンプを取得
    let created = metadata.created()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let accessed = metadata.accessed()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let modified = metadata.modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    // UNIXプラットフォーム固有の情報を取得
    #[cfg(unix)]
    let (uid, gid, links, permissions) = {
        use std::os::unix::fs::MetadataExt;
        
        let mode = metadata.mode();
        let permissions = FilePermissions {
            user: Permission {
                read: (mode & 0o400) != 0,
                write: (mode & 0o200) != 0,
                execute: (mode & 0o100) != 0,
            },
            group: Permission {
                read: (mode & 0o040) != 0,
                write: (mode & 0o020) != 0,
                execute: (mode & 0o010) != 0,
            },
            other: Permission {
                read: (mode & 0o004) != 0,
                write: (mode & 0o002) != 0,
                execute: (mode & 0o001) != 0,
            },
            setuid: (mode & 0o4000) != 0,
            setgid: (mode & 0o2000) != 0,
            sticky: (mode & 0o1000) != 0,
        };
        
        (metadata.uid(), metadata.gid(), metadata.nlink(), permissions)
    };
    
    // 非UNIXプラットフォーム用のダミー値
    #[cfg(not(unix))]
    let (uid, gid, links, permissions) = {
        let permissions = FilePermissions {
            user: Permission {
                read: metadata.permissions().readonly(),
                write: !metadata.permissions().readonly(),
                execute: false,
            },
            group: Permission {
                read: metadata.permissions().readonly(),
                write: !metadata.permissions().readonly(),
                execute: false,
            },
            other: Permission {
                read: metadata.permissions().readonly(),
                write: !metadata.permissions().readonly(),
                execute: false,
            },
            setuid: false,
            setgid: false,
            sticky: false,
        };
        
        (0, 0, 1, permissions)
    };
    
    Ok(FileInfo {
        name,
        path: path.to_path_buf(),
        size: metadata.len(),
        file_type,
        uid,
        gid,
        created,
        accessed,
        modified,
        permissions,
        links,
    })
}

/// ファイルサイズを人間が読みやすい形式で表示
pub fn format_file_size(size: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    
    if size == 0 {
        return "0B".to_string();
    }
    
    let base = 1024_f64;
    let exponent = (size as f64).log(base).floor() as usize;
    let exponent = exponent.min(UNITS.len() - 1);
    
    let size = size as f64 / base.powi(exponent as i32);
    
    if exponent == 0 {
        format!("{:.0}{}", size, UNITS[exponent])
    } else {
        format!("{:.1}{}", size, UNITS[exponent])
    }
}

/// ファイルのパーミッションを文字列形式（例: "rwxr-xr--"）で表示
pub fn format_permissions(permissions: &FilePermissions) -> String {
    let mut result = String::with_capacity(9);
    
    // ユーザー権限
    result.push(if permissions.user.read { 'r' } else { '-' });
    result.push(if permissions.user.write { 'w' } else { '-' });
    result.push(if permissions.setuid {
        if permissions.user.execute { 's' } else { 'S' }
    } else {
        if permissions.user.execute { 'x' } else { '-' }
    });
    
    // グループ権限
    result.push(if permissions.group.read { 'r' } else { '-' });
    result.push(if permissions.group.write { 'w' } else { '-' });
    result.push(if permissions.setgid {
        if permissions.group.execute { 's' } else { 'S' }
    } else {
        if permissions.group.execute { 'x' } else { '-' }
    });
    
    // その他のユーザー権限
    result.push(if permissions.other.read { 'r' } else { '-' });
    result.push(if permissions.other.write { 'w' } else { '-' });
    result.push(if permissions.sticky {
        if permissions.other.execute { 't' } else { 'T' }
    } else {
        if permissions.other.execute { 'x' } else { '-' }
    });
    
    result
}

/// ファイルタイプを表す文字を取得
pub fn get_file_type_char(file_type: FileType) -> char {
    match file_type {
        FileType::Regular => '-',
        FileType::Directory => 'd',
        FileType::SymbolicLink => 'l',
        FileType::BlockDevice => 'b',
        FileType::CharDevice => 'c',
        FileType::Fifo => 'p',
        FileType::Socket => 's',
        FileType::Unknown => '?',
    }
}

/// UnixタイムスタンプをYYYY-MM-DD HH:MM形式に変換
pub fn format_timestamp(timestamp: u64) -> String {
    let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
    
    // 現在時刻を取得
    let now = SystemTime::now();
    let six_months_ago = now - std::time::Duration::from_secs(6 * 30 * 24 * 60 * 60);
    
    // chrono を使用して書式を作成
    let datetime = chrono::DateTime::<chrono::Utc>::from(time);
    
    if time > six_months_ago && time < now {
        // 6ヶ月以内なら "月 日 時:分" の形式
        datetime.format("%b %e %H:%M").to_string()
    } else {
        // それより古いなら "月 日 年" の形式
        datetime.format("%b %e %Y").to_string()
    }
}

/// ユーザーID（UID）からユーザー名を取得
pub fn get_username(uid: u32) -> String {
    #[cfg(unix)]
    {
        use std::ffi::CStr;
        use libc::{getpwuid, passwd};
        
        unsafe {
            let passwd_ptr = getpwuid(uid);
            if passwd_ptr.is_null() {
                return uid.to_string();
            }
            
            let name_ptr = (*passwd_ptr).pw_name;
            if name_ptr.is_null() {
                return uid.to_string();
            }
            
            let name = CStr::from_ptr(name_ptr)
                .to_string_lossy()
                .to_string();
            
            if name.is_empty() {
                uid.to_string()
            } else {
                name
            }
        }
    }
    
    #[cfg(not(unix))]
    {
        uid.to_string()
    }
}

/// グループID（GID）からグループ名を取得
pub fn get_groupname(gid: u32) -> String {
    #[cfg(unix)]
    {
        use std::ffi::CStr;
        use libc::{getgrgid, group};
        
        unsafe {
            let group_ptr = getgrgid(gid);
            if group_ptr.is_null() {
                return gid.to_string();
            }
            
            let name_ptr = (*group_ptr).gr_name;
            if name_ptr.is_null() {
                return gid.to_string();
            }
            
            let name = CStr::from_ptr(name_ptr)
                .to_string_lossy()
                .to_string();
            
            if name.is_empty() {
                gid.to_string()
            } else {
                name
            }
        }
    }
    
    #[cfg(not(unix))]
    {
        gid.to_string()
    }
}

/// ファイルのパターンマッチング（ワイルドカード展開）
pub fn expand_glob_pattern(pattern: &str, current_dir: &Path) -> Result<Vec<PathBuf>> {
    use glob::glob_with;
    use glob::MatchOptions;
    
    // パターンが相対パスかどうかをチェック
    let pattern = if Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        // 相対パスの場合、現在のディレクトリからの相対パスに変換
        current_dir.join(pattern).to_string_lossy().to_string()
    };
    
    // ドットファイルもマッチさせるオプションを設定
    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    
    let mut paths = Vec::new();
    
    match glob_with(&pattern, options) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(path) => paths.push(path),
                    Err(e) => return Err(anyhow!("パスの展開に失敗しました: {}", e)),
                }
            }
        }
        Err(e) => return Err(anyhow!("パターンの解析に失敗しました: {}", e)),
    }
    
    // マッチするものが見つからない場合は、元のパターンをそのまま返す
    if paths.is_empty() {
        paths.push(PathBuf::from(&pattern));
    }
    
    Ok(paths)
} 