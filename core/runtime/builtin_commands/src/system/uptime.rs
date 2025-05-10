use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Local, Duration};
use std::fs::File;
use std::io::{self, Read};
use std::process::Command;
use std::time::SystemTime;

/// システムの稼働時間を表示するコマンド
///
/// 現在の時刻、システムが起動してからの経過時間、ログインユーザー数、
/// システム負荷（ロードアベレージ）を表示します。
///
/// # 使用例
///
/// ```bash
/// uptime              # 現在の稼働時間情報を表示
/// uptime -p           # 稼働時間のみをわかりやすい形式で表示
/// uptime -s           # システム起動時刻を表示
/// ```
pub struct UptimeCommand;

#[async_trait]
impl BuiltinCommand for UptimeCommand {
    fn name(&self) -> &'static str {
        "uptime"
    }

    fn description(&self) -> &'static str {
        "システムの稼働時間を表示します"
    }

    fn usage(&self) -> &'static str {
        "uptime [オプション]\n\n\
        オプション:\n\
        -p, --pretty    人間が読みやすい形式で稼働時間のみを表示\n\
        -s, --since     システムが起動した時刻を表示\n\
        -h, --help      このヘルプを表示して終了"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        let pretty_format = context.args.iter().any(|arg| *arg == "-p" || *arg == "--pretty");
        let show_since = context.args.iter().any(|arg| *arg == "-s" || *arg == "--since");
        let show_help = context.args.iter().any(|arg| *arg == "-h" || *arg == "--help");
        
        if show_help {
            return Ok(CommandResult::success().with_stdout(self.usage().as_bytes().to_vec()));
        }
        
        let mut output = Vec::new();
        
        // システム稼働時間情報を取得
        match get_uptime_info() {
            Ok((uptime, load_avg, user_count)) => {
                if show_since {
                    // 起動時刻を計算
                    let now = Local::now();
                    let boot_time = now - Duration::seconds(uptime as i64);
                    output.extend_from_slice(format!("{}\n", boot_time.format("%Y-%m-%d %H:%M:%S")).as_bytes());
                } else if pretty_format {
                    // 人間が読みやすい形式で表示
                    output.extend_from_slice(format_uptime_pretty(uptime).as_bytes());
                } else {
                    // 標準形式で表示
                    let now = Local::now();
                    output.extend_from_slice(
                        format!("{} up {}, {} users, load average: {:.2}, {:.2}, {:.2}\n",
                            now.format("%H:%M:%S"),
                            format_uptime(uptime),
                            user_count,
                            load_avg[0],
                            load_avg[1],
                            load_avg[2]
                        ).as_bytes()
                    );
                }
            },
            Err(e) => {
                return Err(anyhow!("稼働時間情報の取得に失敗しました: {}", e));
            }
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// システムの稼働時間情報を取得
fn get_uptime_info() -> io::Result<(u64, [f64; 3], u32)> {
    #[cfg(target_os = "linux")]
    {
        // /proc/uptimeからシステム稼働時間を読み取る
        let mut uptime_file = File::open("/proc/uptime")?;
        let mut uptime_str = String::new();
        uptime_file.read_to_string(&mut uptime_str)?;
        
        let uptime_parts: Vec<&str> = uptime_str.split_whitespace().collect();
        let uptime = uptime_parts[0].parse::<f64>().unwrap_or(0.0) as u64;
        
        // /proc/loadavgからロードアベレージを読み取る
        let mut loadavg_file = File::open("/proc/loadavg")?;
        let mut loadavg_str = String::new();
        loadavg_file.read_to_string(&mut loadavg_str)?;
        
        let loadavg_parts: Vec<&str> = loadavg_str.split_whitespace().collect();
        let load_avg = [
            loadavg_parts[0].parse::<f64>().unwrap_or(0.0),
            loadavg_parts[1].parse::<f64>().unwrap_or(0.0),
            loadavg_parts[2].parse::<f64>().unwrap_or(0.0),
        ];
        
        // ログインユーザー数の取得（who -qコマンドを使用）
        let user_count = match Command::new("who").arg("-q").output() {
            Ok(output) => {
                if output.status.success() {
                    // 出力の最終行に "# users=X" の形式で表示される
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = output_str.lines().collect();
                    if let Some(last_line) = lines.last() {
                        if last_line.starts_with("# users=") {
                            last_line[8..].parse::<u32>().unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    0
                }
            },
            Err(_) => 0
        };
        
        Ok((uptime, load_avg, user_count))
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOSの場合はsysctlコマンドを使用
        let uptime = match Command::new("sysctl").arg("-n").arg("kern.boottime").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    // 出力形式: { sec = 1234567890, usec = 123456 } Sat Jan 1 00:00:00 2000
                    if let Some(sec_str) = output_str.split("sec = ").nth(1) {
                        if let Some(sec_end) = sec_str.find(',') {
                            let boot_time = sec_str[..sec_end].trim().parse::<u64>().unwrap_or(0);
                            let now = SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            now.saturating_sub(boot_time)
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    0
                }
            },
            Err(_) => 0
        };
        
        // ロードアベレージを取得
        let load_avg = match Command::new("sysctl").arg("-n").arg("vm.loadavg").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    // 出力形式: { 0.12 0.34 0.56 }
                    let parts: Vec<&str> = output_str.split_whitespace()
                        .filter(|s| !s.contains('{') && !s.contains('}'))
                        .collect();
                    
                    [
                        parts.get(0).unwrap_or(&"0.0").parse::<f64>().unwrap_or(0.0),
                        parts.get(1).unwrap_or(&"0.0").parse::<f64>().unwrap_or(0.0),
                        parts.get(2).unwrap_or(&"0.0").parse::<f64>().unwrap_or(0.0),
                    ]
                } else {
                    [0.0, 0.0, 0.0]
                }
            },
            Err(_) => [0.0, 0.0, 0.0]
        };
        
        // ログインユーザー数を取得
        let user_count = match Command::new("who").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    output_str.lines().count() as u32
                } else {
                    0
                }
            },
            Err(_) => 0
        };
        
        Ok((uptime, load_avg, user_count))
    }
    
    #[cfg(target_os = "windows")]
    {
        // Windowsの場合はSystemTimeとWMICを使用
        let uptime = match Command::new("powershell")
            .args(&["-Command", "(Get-Date) - (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime | Select-Object -ExpandProperty TotalSeconds"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    output_str.parse::<f64>().unwrap_or(0.0) as u64
                } else {
                    0
                }
            },
            Err(_) => 0
        };
        
        // Windowsにはロードアベレージに相当する概念がないため、CPU使用率で代用
        let load_avg = match Command::new("powershell")
            .args(&["-Command", "Get-Counter -Counter \"\\Processor(_Total)\\% Processor Time\" | Select-Object -ExpandProperty CounterSamples | Select-Object -ExpandProperty CookedValue"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let cpu_usage = String::from_utf8_lossy(&output.stdout).trim().parse::<f64>().unwrap_or(0.0) / 100.0;
                    [cpu_usage, cpu_usage, cpu_usage]
                } else {
                    [0.0, 0.0, 0.0]
                }
            },
            Err(_) => [0.0, 0.0, 0.0]
        };
        
        // ログインユーザー数を取得
        let user_count = match Command::new("query").arg("user").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    // ヘッダー行を除く
                    output_str.lines().skip(1).count() as u32
                } else {
                    0
                }
            },
            Err(_) => 0
        };
        
        Ok((uptime, load_avg, user_count))
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(io::Error::new(io::ErrorKind::Unsupported, "このプラットフォームはサポートされていません"))
    }
}

/// 秒単位の稼働時間を人間が読みやすい形式に変換
fn format_uptime(seconds: u64) -> String {
    let days = seconds / (24 * 60 * 60);
    let hours = (seconds % (24 * 60 * 60)) / (60 * 60);
    let minutes = (seconds % (60 * 60)) / 60;
    
    if days > 0 {
        format!("{} 日, {:02}:{:02}", days, hours, minutes)
    } else {
        format!("{:02}:{:02}", hours, minutes)
    }
}

/// 秒単位の稼働時間を人間が読みやすい形式に変換（-p オプション用）
fn format_uptime_pretty(seconds: u64) -> String {
    let days = seconds / (24 * 60 * 60);
    let hours = (seconds % (24 * 60 * 60)) / (60 * 60);
    let minutes = (seconds % (60 * 60)) / 60;
    
    let mut result = String::new();
    
    if days > 0 {
        result.push_str(&format!("{} 日", days));
        if hours > 0 || minutes > 0 {
            result.push_str("、");
        }
    }
    
    if hours > 0 {
        result.push_str(&format!("{} 時間", hours));
        if minutes > 0 {
            result.push_str("、");
        }
    }
    
    if minutes > 0 || (days == 0 && hours == 0) {
        result.push_str(&format!("{} 分", minutes));
    }
    
    result.push_str("\n");
    result
} 