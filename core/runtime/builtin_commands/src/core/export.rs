/*!
 * export コマンド実装
 * 
 * このモジュールは環境変数の設定と表示を行う `export` コマンドを実装します。
 * 引数なしで呼び出された場合は現在の環境変数をすべて表示し、
 * NAME=VALUE の形式で呼び出された場合は環境変数を設定します。
 */

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::{BuiltinCommand, CommandContext, CommandResult};

/// export コマンド実装
///
/// 環境変数の設定と表示を行います。
/// 
/// 使用例:
/// - `export` - 現在の環境変数をすべて表示
/// - `export NAME=VALUE` - 環境変数 NAME に VALUE を設定
/// - `export NAME` - 環境変数 NAME の値を表示
pub struct ExportCommand;

#[async_trait]
impl BuiltinCommand for ExportCommand {
    fn name(&self) -> &'static str {
        "export"
    }

    fn description(&self) -> &'static str {
        "環境変数を設定または表示します"
    }

    fn usage(&self) -> &'static str {
        "export [NAME=VALUE]..."
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        debug!("export コマンドを実行しています: {:?}", context.args);

        // 引数がない場合は全ての環境変数を表示
        if context.args.len() <= 1 {
            return self.display_all_env_vars(&context);
        }

        // 各引数を処理
        let mut result = CommandResult::success();
        let mut updated_env = context.env_vars.clone();
        let mut error_occurred = false;

        // 最初の引数はコマンド名なのでスキップ
        for arg in context.args.iter().skip(1) {
            if let Err(e) = self.process_arg(arg, &mut updated_env, &mut result, &context) {
                // エラーメッセージを標準エラーに追加
                let mut error_msg = format!("export: {}\n", e);
                result.stderr.append(&mut error_msg.into_bytes());
                error_occurred = true;
            }
        }

        // エラーが発生した場合は終了コードを設定
        if error_occurred {
            result.exit_code = 1;
        }
        
        // 環境変数の変更をシェルに反映させるためのメタデータを設定
        let mut env_changes = HashMap::new();
        
        // 元の環境と比較して変更された変数を検出
        for (key, value) in &updated_env {
            if let Some(old_value) = context.env_vars.get(key) {
                if value != old_value {
                    // 変更された変数
                    env_changes.insert(key.clone(), value.clone());
                }
            } else {
                // 新しく追加された変数
                env_changes.insert(key.clone(), value.clone());
            }
        }
        
        // 環境変数の変更があれば、結果にメタデータを追加
        if !env_changes.is_empty() {
            // ShellControlを通じてシェルに環境変数変更を通知
            let shell_control = ShellControl {
                action: ShellAction::UpdateEnv,
                env_changes: Some(env_changes),
                ..ShellControl::default()
            };
            
            result.metadata = Some(CommandMetadata {
                shell_control: Some(shell_control),
                ..CommandMetadata::default()
            });
        }

        Ok(result)
    }
}

impl ExportCommand {
    /// 全ての環境変数を表示
    fn display_all_env_vars(&self, context: &CommandContext) -> Result<CommandResult> {
        let mut output = Vec::new();
        
        // 環境変数をソートして表示
        let mut env_vars: Vec<(&String, &String)> = context.env_vars.iter().collect();
        env_vars.sort_by(|a, b| a.0.cmp(b.0));
        
        for (name, value) in env_vars {
            let line = format!("declare -x {}=\"{}\"\n", name, value);
            output.append(&mut line.into_bytes());
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }

    /// 個々の引数を処理
    fn process_arg(&self, arg: &str, env_snapshot: &mut HashMap<String, String>, result: &mut CommandResult, context: &CommandContext) -> Result<()> {
        if let Some(pos) = arg.find('=') {
            let name = &arg[0..pos];
            let value = &arg[pos+1..];

            if !self.is_valid_variable_name(name) {
                return Err(anyhow!("無効な変数名: {}", name));
            }

            // 1. 現在のシェルセッションの環境に即時反映
            // context.shell_env は Arc<RwLock<HashMap<String, String>>> 型と仮定
            {
                let mut shell_env_guard = context.shell_env.write().map_err(|_| anyhow!("シェル環境の書き込みロック取得に失敗"))?;
                shell_env_guard.insert(name.to_string(), value.to_string());
                debug!("シェルセッションの環境変数 {} を {} に設定しました。", name, value);
            } // ロックを早期に解放

            // 2. コマンド実行時のスナップショットにも反映 (executeメソッドでの差分検出のため)
            env_snapshot.insert(name.to_string(), value.to_string());

            // 3. 親シェルへの伝播 (ベストエフォート)
            if context.config.propagate_env_to_system { // config を参照して伝播を制御
                if let Err(e) = self.propagate_to_parent_shell(name, value, context) { // context を渡す
                    warn!("親シェルへの環境変数伝播中にエラーが発生しました: {}", e);
                }
            } else {
                debug!("propagate_env_to_system が false のため、親シェルへの伝播はスキップされました。");
            }
            
            info!("環境変数 {}={} をエクスポートしました。", name, value);

        } else {
            if !self.is_valid_variable_name(arg) {
                return Err(anyhow!("無効な変数名: {}", arg));
            }

            let mut shell_env_guard = context.shell_env.read().map_err(|_| anyhow!("シェル環境の読み取りロック取得に失敗"))?;
            if !shell_env_guard.contains_key(arg) {
                let mut warning_msg = format!("export: {}: 現在のシェルセッションで定義されていません。値なしでエクスポートマークされます。(スナップショットには空文字列で記録)\n", arg);
                result.stderr.append(&mut warning_msg.into_bytes());
            }
            let value_to_set = shell_env_guard.get(arg).cloned().unwrap_or_default();
            drop(shell_env_guard); 

            env_snapshot.insert(arg.to_string(), value_to_set.clone()); // スナップショットには現在のシェル環境の値を(なければ空で)反映
            
            {
                let mut shell_env_write_guard = context.shell_env.write().map_err(|_| anyhow!("シェル環境の書き込みロック取得に失敗"))?;
                shell_env_write_guard.entry(arg.to_string()).or_insert_with(String::new); // シェル環境にもなければ空で設定
            }

            if context.config.propagate_env_to_system { // NAMEのみの場合も伝播を試みる (空の値で)
                 if let Err(e) = self.propagate_to_parent_shell(arg, &value_to_set, context) {
                    warn!("親シェルへの環境変数伝播中にエラーが発生しました (NAME のみ指定時): {}", e);
                }
            }
            info!("環境変数 {} をエクスポートマークしました。", arg);
        }
        Ok(())
    }
    
    /// 変数名が有効かチェック
    fn is_valid_variable_name(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        
        let mut chars = name.chars();
        
        // 最初の文字は英字またはアンダースコアであること
        match chars.next() {
            Some(c) if c.is_alphabetic() || c == '_' => {}
            _ => return false,
        }
        
        // 残りの文字は英数字またはアンダースコアであること
        for c in chars {
            if !c.is_alphanumeric() && c != '_' {
                return false;
            }
        }
        
        true
    }

    /// 親シェルプロセスに環境変数を伝播する
    fn propagate_to_parent_shell(&self, name: &str, value: &str, context: &CommandContext) -> Result<()> {
        // --- 1. イベントバス/送信機による通知 ---
        if let Some(event_bus) = &context.event_bus {
            match context.shell_event_sender.try_send(crate::ShellEvent::EnvVarChanged {
                name: name.to_string(),
                value: value.to_string()
            }) {
                Ok(_) => debug!("環境変数変更イベント '{}={}' を送信しました。", name, value),
                Err(e) => warn!("環境変数変更イベントの送信に失敗: {}", e),
            }
        }
        // --- 2. 共有メモリ/高速IPC ---
        if let Some(shared_mem_path) = &context.config.shared_memory_path {
            #[cfg(feature = "shared-memory")]
            {
                use shared_memory::{ShmemConf, ShmemError};
                let event_data = format!("ENV_CHANGED:{}={}", name, value);
                match ShmemConf::new().size(4096).flink(shared_mem_path).create() {
                    Ok(shmem) => {
                        let bytes = event_data.as_bytes();
                        if bytes.len() + 4 <= shmem.len() {
                            let mut data = shmem.as_slice_mut();
                            data[0..4].copy_from_slice(&(bytes.len() as u32).to_ne_bytes());
                            data[4..4+bytes.len()].copy_from_slice(bytes);
                            debug!("共有メモリ経由で '{}={}' の変更を通知しました", name, value);
                        } else {
                            warn!("共有メモリのサイズが不足しています");
                        }
                    },
                    Err(e) => warn!("共有メモリの作成に失敗: {:?}", e),
                }
            }
        }
        // --- 3. Unix: ドメインソケット通知 ---
        #[cfg(unix)]
        {
            if let Some(socket_path) = std::env::var_os("NEXUS_SHELL_CONTROL_SOCKET") {
                use std::os::unix::net::UnixStream;
                use std::io::Write;
                debug!("Unixドメインソケット '{}' を使用して親シェルへの通知を試みます。", socket_path.to_string_lossy());
                match UnixStream::connect(&socket_path) {
                    Ok(mut socket) => {
                        let message = format!("EXPORT:{}={}", name, value);
                        if let Err(e) = socket.write_all(message.as_bytes()) {
                            warn!("親シェルへの環境変数通知 (Unixソケット書き込み) に失敗: {}", e);
                        } else {
                            debug!("親シェルに '{}={}' をUnixソケット経由で通知しました。", name, value);
                        }
                    }
                    Err(e) => {
                        warn!("親シェル制御ソケット '{}' への接続に失敗: {}", socket_path.to_string_lossy(), e);
                    }
                }
            }
        }
        // --- 4. Windows: 名前付きパイプ/WM_COPYDATA/レジストリ ---
        #[cfg(windows)]
        {
            if let Some(pipe_name_os) = std::env::var_os("NEXUS_SHELL_CONTROL_PIPE") {
                use std::fs::OpenOptions;
                use std::io::Write;
                let pipe_path = format!(r"\\.\pipe\{}", pipe_name_os.to_string_lossy());
                debug!("名前付きパイプ '{}' を使用して親シェルへの通知を試みます。", pipe_path);
                match OpenOptions::new().write(true).open(&pipe_path) {
                    Ok(mut pipe) => {
                        let message = format!("EXPORT:{}={}", name, value);
                        if let Err(e) = pipe.write_all(message.as_bytes()) {
                            warn!("親シェルへの環境変数通知 (名前付きパイプ書き込み) に失敗: {}", e);
                        } else {
                            debug!("親シェルに '{}={}' を名前付きパイプ経由で通知しました。", name, value);
                        }
                    }
                    Err(e) => {
                        warn!("親シェル制御パイプ '{}' への接続に失敗: {}", pipe_path, e);
                    }
                }
            }
            // WM_COPYDATA通知（GUI/ターミナル連携）
            #[cfg(feature = "win-ipc")]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_COPYDATA, COPYDATASTRUCT};
                use std::ptr;
                if let Some(window_name) = &context.config.parent_window_name {
                    let window_name_wide: Vec<u16> = window_name.encode_utf16().chain(Some(0)).collect();
                    let hwnd = unsafe { FindWindowW(ptr::null(), window_name_wide.as_ptr()) };
                    if hwnd != 0 {
                        let message = format!("EXPORT:{}={}\0", name, value);
                        let message_bytes = message.as_bytes();
                        let mut copydata = COPYDATASTRUCT {
                            dwData: 1,
                            cbData: message_bytes.len() as u32,
                            lpData: message_bytes.as_ptr() as *const core::ffi::c_void,
                        };
                        let result = unsafe {
                            SendMessageW(
                                hwnd,
                                WM_COPYDATA,
                                0,
                                &mut copydata as *mut _ as isize
                            )
                        };
                        if result == 0 {
                            warn!("WM_COPYDATAメッセージの送信に失敗しました");
                        } else {
                            debug!("WM_COPYDATAメッセージ経由で親ウィンドウに通知しました");
                        }
                    } else {
                        warn!("親ウィンドウ '{}' が見つかりません", window_name);
                    }
                }
            }
            // レジストリ永続化
            if let Err(e) = self.update_windows_environment(name, value) {
                warn!("Windowsレジストリへの環境変数永続化に失敗: {}", e);
            }
        }
        // --- 5. Unix: プロファイル永続化 ---
        #[cfg(unix)]
        {
            if let Err(e) = self.update_unix_environment(name, value, context) {
                warn!("Unixプロファイルへの環境変数永続化に失敗: {}", e);
            }
        }
        // --- 6. 高度なIPC（オプション） ---
        #[cfg(feature = "advanced-ipc")]
        {
            use interprocess::local_socket::LocalSocketStream;
            if let Some(socket_name) = &context.config.ipc_socket_name {
                match LocalSocketStream::connect(socket_name) {
                    Ok(mut stream) => {
                        use std::io::Write;
                        let message = format!("ENV:{}={}\n", name, value);
                        if let Err(e) = stream.write_all(message.as_bytes()) {
                            warn!("IPC経由の環境変数通知に失敗: {}", e);
                        } else {
                            debug!("IPC経由で '{}={}' の変更を通知しました", name, value);
                        }
                    },
                    Err(e) => warn!("IPCソケット接続に失敗: {}", e),
                }
            }
        }
        // --- 7. プロセス環境変数の即時反映 ---
        std::env::set_var(name, value);
        Ok(())
    }
    
    /// Windows環境変数をレジストリに設定する (Windows専用)
    #[cfg(windows)]
    fn update_windows_environment(&self, name: &str, value: &str) -> Result<()> {
        // winregクレートを使用してレジストリに環境変数を永続化
        use winreg::enums::*;
        use winreg::RegKey;
        
        // ユーザー環境変数（HKEY_CURRENT_USER\Environment）
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env_key = hkcu.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .map_err(|e| anyhow!("ユーザー環境変数レジストリキーを開けませんでした: {}", e))?;
            
        // 既存の値をチェック（変更がある場合のみ更新）
        let existing_value: Result<String, _> = env_key.get_value(name);
        
        if existing_value.as_ref().map(|v| v != value).unwrap_or(true) {
            // 値が異なるか存在しない場合は設定
            env_key.set_value(name, &value)
                .map_err(|e| anyhow!("レジストリへの環境変数設定に失敗: {}", e))?;
                
            // 環境変数変更を通知（他のアプリケーションに反映させる）
            use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, WM_SETTINGCHANGE};
            use std::ptr;
            
            let env_str: Vec<u16> = "Environment\0".encode_utf16().collect();
            
            unsafe {
                SendMessageTimeoutW(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    0,
                    env_str.as_ptr() as isize,
                    0,
                    1000,
                    ptr::null_mut(),
                );
            }
            
            info!("環境変数 '{}' をレジストリに永続化しました", name);
        } else {
            debug!("環境変数 '{}' は既に同じ値でレジストリに設定されています", name);
        }
        
        Ok(())
    }
    
    /// Unix環境での環境変数の永続化
    #[cfg(unix)]
    fn update_unix_environment(&self, name: &str, value: &str, context: &CommandContext) -> Result<()> {
        use std::fs::{File, OpenOptions};
        use std::io::{self, BufRead, BufReader, Write};
        use std::path::PathBuf;
        
        // ホームディレクトリの確認
        let home_dir = dirs::home_dir()
            .ok_or_else(|| anyhow!("ホームディレクトリが見つかりませんでした"))?;
        
        // 現在のシェルを特定
        let shell = std::env::var("SHELL")
            .unwrap_or_else(|_| "/bin/sh".to_string());
        
        // シェルタイプに基づいて適切な設定ファイルを選択
        let config_file = if shell.contains("bash") {
            // Bashの場合
            if context.config.persistent_env_global {
                home_dir.join(".bash_profile")
            } else {
                home_dir.join(".bashrc")
            }
        } else if shell.contains("zsh") {
            // Zshの場合
            home_dir.join(".zshrc")
        } else {
            // その他のシェルやデフォルト
            home_dir.join(".profile")
        };
        
        debug!("環境変数 '{}={}' を {} に保存します", name, value, config_file.display());
        
        // 変数宣言行のパターン
        let export_pattern = format!("export {}=", name);
        
        // ファイルが存在するかチェック
        if config_file.exists() {
            // 一時ファイルを作成
            let temp_file_path = config_file.with_extension("temp");
            let mut temp_file = File::create(&temp_file_path)
                .map_err(|e| anyhow!("一時ファイルの作成に失敗: {}", e))?;
            
            // 現在のファイルを読み込み
            let file = File::open(&config_file)
                .map_err(|e| anyhow!("設定ファイルのオープンに失敗: {}", e))?;
            let reader = BufReader::new(file);
            
            let mut updated = false;
            
            // 行ごとに処理
            for line in reader.lines() {
                let line = line?;
                if line.trim().starts_with(&export_pattern) {
                    // 既存のエクスポート行を更新
                    if context.config.use_quotes_in_env {
                        writeln!(temp_file, "export {}=\"{}\"", name, value.replace("\"", "\\\""))?;
                    } else {
                        writeln!(temp_file, "export {}={}", name, value)?;
                    }
                    updated = true;
                } else {
                    // その他の行はそのまま
                    writeln!(temp_file, "{}", line)?;
                }
            }
            
            // 変数が見つからなかった場合は追加
            if !updated {
                if context.config.use_quotes_in_env {
                    writeln!(temp_file, "\n# Added by NexusShell export command")?;
                    writeln!(temp_file, "export {}=\"{}\"", name, value.replace("\"", "\\\""))?;
                } else {
                    writeln!(temp_file, "\n# Added by NexusShell export command")?;
                    writeln!(temp_file, "export {}={}", name, value)?;
                }
            }
            
            // 一時ファイルを元のファイルに置き換え
            std::fs::rename(&temp_file_path, &config_file)
                .map_err(|e| anyhow!("設定ファイルの更新に失敗: {}", e))?;
                
        } else {
            // ファイルが存在しない場合は新規作成
            let mut file = File::create(&config_file)
                .map_err(|e| anyhow!("設定ファイルの作成に失敗: {}", e))?;
                
            writeln!(file, "#!/bin/sh")?;
            writeln!(file, "# NexusShell環境変数 - {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
            if context.config.use_quotes_in_env {
                writeln!(file, "export {}=\"{}\"", name, value.replace("\"", "\\\""))?;
            } else {
                writeln!(file, "export {}={}", name, value)?;
            }
        }
        
        info!("環境変数 '{}={}' を {} に永続化しました", name, value, config_file.display());
        Ok(())
    }
    
    // Windows以外の環境ではダミー実装を提供
    #[cfg(not(windows))]
    fn update_windows_environment(&self, _name: &str, _value: &str) -> Result<()> {
        Ok(())
    }
    
    // Unix以外の環境ではダミー実装を提供
    #[cfg(not(unix))]
    fn update_unix_environment(&self, _name: &str, _value: &str, _context: &CommandContext) -> Result<()> {
        Ok(())
    }
}

/// 現在のプロセスの環境変数を設定（実行中のプロセス内）
fn set_process_environment(name: &str, value: &str) -> Result<()> {
    // プロセス環境変数に設定
    std::env::set_var(name, value);
    debug!("プロセス内環境変数を設定: {}={}", name, value);
    Ok(())
}

/// シェルの種類を判定
fn get_shell_type(&self, context: &CommandContext) -> ShellType {
    let shell_path = std::env::var("SHELL").unwrap_or_default();
    let shell_name = std::path::Path::new(&shell_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    
    // Windowsの場合、コマンドプロンプトかPowerShellかを判定
    #[cfg(target_os = "windows")]
    {
        if let Some(command_prompt) = &context.env_vars.get("ComSpec") {
            if command_prompt.to_lowercase().contains("cmd.exe") {
                return ShellType::Cmd;
            }
        }
        
        if let Some(ps_version) = &context.env_vars.get("PSModulePath") {
            if !ps_version.is_empty() {
                return ShellType::PowerShell;
            }
        }
    }
    
    // Unix系の場合、シェルの種類を判定
    if shell_name.contains("bash") {
        ShellType::Bash
    } else if shell_name.contains("zsh") {
        ShellType::Zsh
    } else if shell_name.contains("sh") {
        ShellType::Posix
    } else {
        ShellType::Unknown
    }
}

/// シェルの種類を表す列挙型
#[derive(Debug, PartialEq)]
enum ShellType {
    Bash,
    Zsh,
    Posix,
    Cmd,
    PowerShell,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_export_display_all() {
        let command = ExportCommand;
        let mut env_vars = HashMap::new();
        env_vars.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        env_vars.insert("HOME".to_string(), "/home/user".to_string());
        
        let context = CommandContext {
            current_dir: std::path::PathBuf::from("/"),
            env_vars,
            args: vec!["export".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        let output = String::from_utf8(result.stdout).unwrap();
        assert!(output.contains("export HOME=\"/home/user\""));
        assert!(output.contains("export PATH=\"/usr/bin:/bin\""));
    }
    
    #[tokio::test]
    async fn test_export_set_variable() {
        let command = ExportCommand;
        let mut env_vars = HashMap::new();
        
        let context = CommandContext {
            current_dir: std::path::PathBuf::from("/"),
            env_vars,
            args: vec!["export".to_string(), "TEST=value".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 0);
        
        // 新しい環境変数が設定されていることを確認
        let test_value = std::env::var("TEST").unwrap_or_default();
        assert_eq!(test_value, "value", "環境変数がプロセス内に正しく設定されていません");
    }
    
    #[tokio::test]
    async fn test_export_invalid_name() {
        let command = ExportCommand;
        let mut env_vars = HashMap::new();
        
        let context = CommandContext {
            current_dir: std::path::PathBuf::from("/"),
            env_vars,
            args: vec!["export".to_string(), "1INVALID=value".to_string()],
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let result = command.execute(context).await.unwrap();
        assert_eq!(result.exit_code, 1);
        
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains("無効な変数名"));
    }
} 