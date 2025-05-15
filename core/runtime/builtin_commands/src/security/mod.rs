/*!
# セキュリティコマンドモジュール

このモジュールは、ファイルのパーミッションや所有者の管理など、
セキュリティ関連の組み込みコマンドを提供します。

## 含まれるコマンド

- `chmod`: ファイルの権限を変更
- `chown`: ファイルの所有者を変更
- `umask`: ファイル作成時のデフォルトパーミッションマスクを設定
- `su`: ユーザーを切り替え
- `passwd`: パスワードを変更
*/

// 各コマンドをサブモジュールとしてエクスポート
pub mod chmod;
pub mod chown;
pub mod umask;
pub mod su;
pub mod passwd;

// パブリックエクスポート
pub use chmod::ChmodCommand;
pub use chown::ChownCommand;

use crate::BuiltinCommand;
use crate::registry::CommandRegistry;
use anyhow::Result;

/// セキュリティコマンドを登録
pub fn register_security_commands(registry: &mut CommandRegistry) -> Result<()> {
    registry.register(Box::new(chmod::ChmodCommand::new()));
    registry.register(Box::new(chown::ChownCommand::new()));
    // registry.register(Box::new(umask::UmaskCommand::new()));
    // registry.register(Box::new(su::SuCommand::new()));
    // registry.register(Box::new(passwd::PasswdCommand::new()));
    
    Ok(())
}

/// セキュリティ関連のユーティリティ関数

/// 権限文字列を数値に変換（例: "rwxr-xr--" → 0754）
pub fn permission_string_to_mode(perm_str: &str) -> u32 {
    let mut mode = 0;
    let chars: Vec<char> = perm_str.chars().collect();
    
    // ユーザー権限
    if chars.len() > 0 && chars[0] == 'r' { mode |= 0o400; }
    if chars.len() > 1 && chars[1] == 'w' { mode |= 0o200; }
    if chars.len() > 2 && chars[2] == 'x' { mode |= 0o100; }
    
    // グループ権限
    if chars.len() > 3 && chars[3] == 'r' { mode |= 0o040; }
    if chars.len() > 4 && chars[4] == 'w' { mode |= 0o020; }
    if chars.len() > 5 && chars[5] == 'x' { mode |= 0o010; }
    
    // その他の権限
    if chars.len() > 6 && chars[6] == 'r' { mode |= 0o004; }
    if chars.len() > 7 && chars[7] == 'w' { mode |= 0o002; }
    if chars.len() > 8 && chars[8] == 'x' { mode |= 0o001; }
    
    mode
}

/// 数値モードを権限文字列に変換（例: 0754 → "rwxr-xr--"）
pub fn mode_to_permission_string(mode: u32) -> String {
    let mut result = String::with_capacity(9);
    
    // ユーザー権限
    result.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    result.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    result.push(if mode & 0o100 != 0 { 'x' } else { '-' });
    
    // グループ権限
    result.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    result.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    result.push(if mode & 0o010 != 0 { 'x' } else { '-' });
    
    // その他の権限
    result.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    result.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    result.push(if mode & 0o001 != 0 { 'x' } else { '-' });
    
    result
} 