use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::env;
use std::process::Command;

/// システム情報を表示するコマンド
///
/// カーネル名、ホスト名、OS情報などのシステム識別情報を表示します。
///
/// # 使用例
///
/// ```bash
/// uname               # カーネル名を表示
/// uname -a            # すべての情報を表示
/// uname -s -r         # カーネル名とリリースを表示
/// ```
pub struct UnameCommand;

#[async_trait]
impl BuiltinCommand for UnameCommand {
    fn name(&self) -> &'static str {
        "uname"
    }

    fn description(&self) -> &'static str {
        "システム情報を表示します"
    }

    fn usage(&self) -> &'static str {
        "uname [オプション]...\n\n\
        オプション:\n\
        -a, --all       すべての情報を表示\n\
        -s, --kernel-name      カーネル名を表示\n\
        -n, --nodename         ネットワークノードのホスト名を表示\n\
        -r, --kernel-release   カーネルリリースを表示\n\
        -v, --kernel-version   カーネルバージョンを表示\n\
        -m, --machine          マシンのハードウェア名を表示\n\
        -p, --processor        プロセッサタイプを表示\n\
        -i, --hardware-platform        ハードウェアプラットフォームを表示\n\
        -o, --operating-system         オペレーティングシステムを表示"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let show_kernel_name = context.args.len() == 1 || 
            context.args.iter().any(|arg| *arg == "-s" || *arg == "--kernel-name") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        let show_nodename = context.args.iter().any(|arg| *arg == "-n" || *arg == "--nodename") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        let show_kernel_release = context.args.iter().any(|arg| *arg == "-r" || *arg == "--kernel-release") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        let show_kernel_version = context.args.iter().any(|arg| *arg == "-v" || *arg == "--kernel-version") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        let show_machine = context.args.iter().any(|arg| *arg == "-m" || *arg == "--machine") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        let show_processor = context.args.iter().any(|arg| *arg == "-p" || *arg == "--processor") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        let show_hardware_platform = context.args.iter().any(|arg| *arg == "-i" || *arg == "--hardware-platform") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        let show_os = context.args.iter().any(|arg| *arg == "-o" || *arg == "--operating-system") ||
            context.args.iter().any(|arg| *arg == "-a" || *arg == "--all");
        
        // 結果を格納するバッファ
        let mut output = Vec::new();
        let mut parts = Vec::new();
        
        // システム情報を取得
        #[cfg(target_os = "linux")]
        {
            if show_kernel_name {
                if let Ok(output) = Command::new("uname").arg("-s").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_nodename {
                if let Ok(output) = Command::new("uname").arg("-n").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_kernel_release {
                if let Ok(output) = Command::new("uname").arg("-r").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_kernel_version {
                if let Ok(output) = Command::new("uname").arg("-v").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_machine {
                if let Ok(output) = Command::new("uname").arg("-m").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_processor {
                if let Ok(output) = Command::new("uname").arg("-p").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_hardware_platform {
                if let Ok(output) = Command::new("uname").arg("-i").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_os {
                if let Ok(output) = Command::new("uname").arg("-o").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
        }
        
        #[cfg(target_os = "macos")]
        {
            if show_kernel_name {
                parts.push("Darwin".to_string());
            }
            
            if show_nodename {
                if let Ok(output) = Command::new("hostname").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_kernel_release {
                if let Ok(output) = Command::new("uname").arg("-r").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_kernel_version {
                if let Ok(output) = Command::new("uname").arg("-v").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_machine {
                if let Ok(output) = Command::new("uname").arg("-m").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_processor {
                if let Ok(output) = Command::new("uname").arg("-p").output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_hardware_platform {
                parts.push("unknown".to_string());
            }
            
            if show_os {
                parts.push("Darwin".to_string());
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            if show_kernel_name {
                parts.push("Windows_NT".to_string());
            }
            
            if show_nodename {
                if let Ok(hostname) = env::var("COMPUTERNAME") {
                    parts.push(hostname);
                }
            }
            
            if show_kernel_release {
                // Windowsのバージョン情報を取得
                // レジストリからの取得は複雑なため簡易化
                parts.push(env::var("OS").unwrap_or_else(|_| "Unknown".to_string()));
            }
            
            if show_kernel_version {
                if let Ok(output) = Command::new("cmd").args(&["/c", "ver"]).output() {
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        parts.push(s);
                    }
                }
            }
            
            if show_machine {
                if let Ok(arch) = env::var("PROCESSOR_ARCHITECTURE") {
                    parts.push(arch);
                }
            }
            
            if show_processor {
                if let Ok(proc) = env::var("PROCESSOR_IDENTIFIER") {
                    parts.push(proc);
                }
            }
            
            if show_hardware_platform {
                parts.push("unknown".to_string());
            }
            
            if show_os {
                parts.push("Windows_NT".to_string());
            }
        }
        
        // 結果を出力
        output.extend_from_slice(format!("{}\n", parts.join(" ")).as_bytes());
        
        Ok(CommandResult::success().with_stdout(output))
    }
} 