use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use byte_unit::{Byte, ByteUnit};
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use tabular::{Row, Table};
use crate::utils::format::{format_bytes, format_table};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};
use crate::command::{Command, CommandContext, CommandOutput};
use std::fmt;

/// メモリ使用状況を表示するコマンド
///
/// メモリの合計サイズ、使用量、空き容量、キャッシュなどの情報を表示します。
/// Linux、macOS、Windowsに対応しています。
///
/// # 使用例
///
/// ```bash
/// free            # 基本的なメモリ情報を表示
/// free -h         # 人間が読みやすい形式で表示
/// free -b         # バイト単位で表示
/// free -k         # キロバイト単位で表示
/// free -m         # メガバイト単位で表示
/// free -g         # ギガバイト単位で表示
/// free --si       # 1000進法の単位を使用
/// ```
pub struct FreeCommand;

/// メモリ情報を格納する構造体
struct MemoryInfo {
    total: u64,
    used: u64,
    free: u64,
    shared: u64,
    buffers: u64,
    cached: u64,
    available: u64,
}

/// スワップ情報を格納する構造体
struct SwapInfo {
    total: u64,
    used: u64,
    free: u64,
}

/// メモリサイズの単位を指定するオプション
enum SizeUnit {
    Bytes,
    Kilobytes,
    Megabytes,
    Gigabytes,
    Human,
    Si,
}

/// メモリ情報を格納する構造体
struct MemInfo {
    total: u64,
    used: u64,
    free: u64,
    shared: Option<u64>,
    buffers: Option<u64>,
    cached: Option<u64>,
    available: Option<u64>,
}

/// メモリサイズのフォーマット用ユニット
#[derive(Debug, Clone, Copy)]
enum MemoryUnit {
    Bytes,
    Kilobytes,
    Megabytes,
    Gigabytes,
}

impl MemoryUnit {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "b" => Some(MemoryUnit::Bytes),
            "k" | "kb" => Some(MemoryUnit::Kilobytes),
            "m" | "mb" => Some(MemoryUnit::Megabytes),
            "g" | "gb" => Some(MemoryUnit::Gigabytes),
            _ => None,
        }
    }

    fn format_size(&self, size: u64) -> String {
        match self {
            MemoryUnit::Bytes => format!("{} B", size),
            MemoryUnit::Kilobytes => format!("{} KB", size / 1024),
            MemoryUnit::Megabytes => format!("{} MB", size / (1024 * 1024)),
            MemoryUnit::Gigabytes => format!("{} GB", size / (1024 * 1024 * 1024)),
        }
    }
}

impl Default for MemoryUnit {
    fn default() -> Self {
        MemoryUnit::Kilobytes
    }
}

/// メモリ情報を保持する構造体
#[derive(Debug)]
struct MemoryInfo {
    total: u64,
    used: u64,
    free: u64,
    shared: u64,
    buffers: u64,
    cached: u64,
    available: u64,
    swap_total: u64,
    swap_used: u64,
    swap_free: u64,
}

/// freeコマンドの出力オプション
#[derive(Debug, Default)]
struct FreeOptions {
    unit: MemoryUnit,
    human_readable: bool,
    total_line: bool,
    wide_output: bool,
}

impl FreeOptions {
    fn new() -> Self {
        Self::default()
    }

    fn parse_args(&mut self, args: &[String]) -> Result<(), String> {
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-b" => self.unit = MemoryUnit::Bytes,
                "-k" => self.unit = MemoryUnit::Kilobytes,
                "-m" => self.unit = MemoryUnit::Megabytes,
                "-g" => self.unit = MemoryUnit::Gigabytes,
                "--kilo" => self.unit = MemoryUnit::Kilobytes,
                "--mega" => self.unit = MemoryUnit::Megabytes,
                "--giga" => self.unit = MemoryUnit::Gigabytes,
                "-h" | "--human" => self.human_readable = true,
                "-t" | "--total" => self.total_line = true,
                "-w" | "--wide" => self.wide_output = true,
                "--help" => return Err("help".to_string()),
                "--version" => return Err("version".to_string()),
                arg if arg.starts_with("--") => {
                    if let Some(unit_str) = arg.strip_prefix("--") {
                        if let Some(unit) = MemoryUnit::from_str(unit_str) {
                            self.unit = unit;
                        } else {
                            return Err(format!("不明なオプション: {}", arg));
                        }
                    }
                }
                _ => {
                    if args[i].starts_with('-') {
                        return Err(format!("不明なオプション: {}", args[i]));
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }

    fn format_memory(&self, size: u64) -> String {
        if self.human_readable {
            if size < 1024 {
                format!("{}B", size)
            } else if size < 1024 * 1024 {
                format!("{:.1}K", size as f64 / 1024.0)
            } else if size < 1024 * 1024 * 1024 {
                format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
            } else {
                format!("{:.1}G", size as f64 / (1024.0 * 1024.0 * 1024.0))
            }
        } else {
            self.unit.format_size(size)
        }
    }
}

/// メモリ情報の取得
fn get_memory_info() -> MemoryInfo {
    let mut system = System::new_all();
    system.refresh_all();
    
    // メモリ情報の取得
    let total = system.total_memory();
    let used = total - system.available_memory();
    let free = system.free_memory();
    let available = system.available_memory();
    
    // OSに応じて適切な方法でキャッシュされたメモリ量を取得
    let cached = if cfg!(target_os = "linux") { 
        // Linuxの場合はprocファイルシステムから情報を取得
        match std::fs::read_to_string("/proc/meminfo") {
            Ok(meminfo) => {
                // 各種キャッシュメモリの情報を取得
                let mut cached_kb = 0;
                
                for line in meminfo.lines() {
                    if line.starts_with("Cached:") {
                        if let Some(value) = parse_meminfo_line(line) {
                            cached_kb += value;
                        }
                    } else if line.starts_with("Buffers:") {
                        if let Some(value) = parse_meminfo_line(line) {
                            cached_kb += value;
                        }
                    } else if line.starts_with("SReclaimable:") {
                        if let Some(value) = parse_meminfo_line(line) {
                            cached_kb += value;
                        }
                    }
                }
                
                // KBからMBに変換
                cached_kb / 1024
            }
            Err(_) => 0,
        }
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        // macOSの場合はsysctlを使用
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            use std::process::Command;
            
            // vm_stats コマンドを実行して結果を取得
            let output = Command::new("vm_stat")
                .output()
                .ok();
                
            if let Some(output) = output {
                if let Ok(output_str) = String::from_utf8(output.stdout) {
                    // ページサイズを取得（通常4KB）
                    let page_size = 4096;
                    
                    // 各種キャッシュメモリの情報を取得
                    let mut cached_pages = 0;
                    
                    for line in output_str.lines() {
                        if line.contains("Pages inactive:") || 
                           line.contains("Pages purgeable:") {
                            if let Some(value) = parse_vm_stat_line(line) {
                                cached_pages += value;
                            }
                        }
                    }
                    
                    // ページ数をMBに変換
                    return (cached_pages as u64 * page_size as u64) / (1024 * 1024);
                }
            }
            0
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        0
    } else if cfg!(target_os = "windows") {
        // Windowsの場合はPerfomance Counterを使用
        #[cfg(target_os = "windows")]
        {
            use std::mem::MaybeUninit;
            use windows_sys::Win32::System::Performance::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
            let mut perf_info = MaybeUninit::<PERFORMANCE_INFORMATION>::uninit();
            let ok = unsafe { GetPerformanceInfo(perf_info.as_mut_ptr(), std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32) };
            if ok == 0 {
                return 0;
            }
            let perf_info = unsafe { perf_info.assume_init() };
            perf_info.SystemCache as u64 * perf_info.PageSize as u64
        }
        #[cfg(not(target_os = "windows"))]
        0
    } else {
        // その他のOSの場合は0を返す
        0
    };
    
    let buffers = get_system_buffers()?;
    
    // Swap情報
    let swap_total = system.total_swap();
    let swap_used = system.used_swap();
    let swap_free = swap_total - swap_used;
    
    // 共有メモリ（shmem）情報
    // sysinfo クレートでは直接取得できないため、0とする
    let shared = 0;
    
    MemoryInfo {
        total,
        used,
        free,
        shared,
        buffers,
        cached,
        available,
        swap_total,
        swap_used,
        swap_free,
    }
}

/// freeコマンドのヘルプメッセージ
fn print_help() -> String {
    r#"使用法: free [オプション]

メモリの使用状況を表示します。

オプション:
  -b, --bytes         バイト単位で表示
  -k, --kilo          キロバイト単位で表示 (デフォルト)
  -m, --mega          メガバイト単位で表示
  -g, --giga          ギガバイト単位で表示
  -h, --human         人間が読みやすい形式で表示
  -t, --total         合計の行を表示
  -w, --wide          ワイド出力モード
      --help          このヘルプメッセージを表示して終了
      --version       バージョン情報を表示して終了
"#.to_string()
}

/// freeコマンドのバージョン情報
fn print_version() -> String {
    "free (NexusShell builtin) 1.0.0\n".to_string()
}

#[async_trait]
impl Command for FreeCommand {
    fn name(&self) -> &'static str {
        "free"
    }

    fn description(&self) -> &'static str {
        "システムのメモリ使用状況を表示します"
    }

    async fn execute(&self, ctx: &mut CommandContext) -> Result<CommandOutput> {
        let args = ctx.args.clone();
        
        let mut options = FreeOptions::new();
        match options.parse_args(&args) {
            Ok(_) => {},
            Err(error) => {
                if error == "help" {
                    return Ok(CommandOutput::text(print_help()));
                } else if error == "version" {
                    return Ok(CommandOutput::text(print_version()));
                } else {
                    return Ok(CommandOutput::error(format!("エラー: {}\n{}", error, print_help())));
                }
            }
        }

        let memory_info = get_memory_info();
        let output = format_memory_info(&memory_info, &options);
        
        Ok(CommandOutput::text(output))
    }
}

/// メモリ情報を整形して出力
fn format_memory_info(info: &MemoryInfo, options: &FreeOptions) -> String {
    let mut result = String::new();
    
    // ヘッダーの作成
    if options.wide_output {
        result.push_str(&format!("{:10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
            "", "total", "used", "free", "shared", "buffers", "cached", "available"));
    } else {
        result.push_str(&format!("{:10} {:>10} {:>10} {:>10}\n",
            "", "total", "used", "free"));
    }
    
    // メモリ情報の出力
    if options.wide_output {
        result.push_str(&format!("{:10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
            "Mem:",
            options.format_memory(info.total),
            options.format_memory(info.used),
            options.format_memory(info.free),
            options.format_memory(info.shared),
            options.format_memory(info.buffers),
            options.format_memory(info.cached),
            options.format_memory(info.available)
        ));
    } else {
        result.push_str(&format!("{:10} {:>10} {:>10} {:>10}\n",
            "Mem:",
            options.format_memory(info.total),
            options.format_memory(info.used),
            options.format_memory(info.free)
        ));
    }
    
    // Swap情報の出力
    if options.wide_output {
        result.push_str(&format!("{:10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
            "Swap:",
            options.format_memory(info.swap_total),
            options.format_memory(info.swap_used),
            options.format_memory(info.swap_free),
            options.format_memory(0),
            options.format_memory(0),
            options.format_memory(0),
            options.format_memory(0)
        ));
    } else {
        result.push_str(&format!("{:10} {:>10} {:>10} {:>10}\n",
            "Swap:",
            options.format_memory(info.swap_total),
            options.format_memory(info.swap_used),
            options.format_memory(info.swap_free)
        ));
    }
    
    // 合計行の出力（オプションが指定されている場合）
    if options.total_line {
        if options.wide_output {
            let total_used = info.used + info.swap_used;
            let total_free = info.free + info.swap_free;
            let total_total = info.total + info.swap_total;
            
            result.push_str(&format!("{:10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
                "Total:",
                options.format_memory(total_total),
                options.format_memory(total_used),
                options.format_memory(total_free),
                options.format_memory(info.shared),
                options.format_memory(info.buffers),
                options.format_memory(info.cached),
                options.format_memory(info.available)
            ));
        } else {
            let total_used = info.used + info.swap_used;
            let total_free = info.free + info.swap_free;
            let total_total = info.total + info.swap_total;
            
            result.push_str(&format!("{:10} {:>10} {:>10} {:>10}\n",
                "Total:",
                options.format_memory(total_total),
                options.format_memory(total_used),
                options.format_memory(total_free)
            ));
        }
    }
    
    result
}

/// /proc/meminfo の行からメモリ値（KB単位）を解析する
fn parse_meminfo_line(line: &str) -> Option<u64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let value_str = parts[1];
        if let Ok(value) = value_str.parse::<u64>() {
            return Some(value);
        }
    }
    None
}

/// vm_stat の行からページ数を解析する
fn parse_vm_stat_line(line: &str) -> Option<u64> {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() >= 2 {
        let value_str = parts[1].trim().trim_end_matches('.');
        if let Ok(value) = value_str.parse::<u64>() {
            return Some(value);
        }
    }
    None
}

// OSごとのバッファ取得関数
#[cfg(target_os = "linux")]
fn get_system_buffers() -> Result<u64, FreeError> {
    // /proc/meminfoからBuffersを取得
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    let file = File::open("/proc/meminfo")?;
    for line in BufReader::new(file).lines() {
        let l = line?;
        if l.starts_with("Buffers:") {
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 2 {
                return Ok(parts[1].parse::<u64>().unwrap_or(0) * 1024);
            }
        }
    }
    Ok(0)
}
#[cfg(target_os = "windows")]
fn get_system_buffers() -> Result<u64, FreeError> {
    use std::mem::MaybeUninit;
    use windows_sys::Win32::System::Performance::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    let mut perf_info = MaybeUninit::<PERFORMANCE_INFORMATION>::uninit();
    let ok = unsafe { GetPerformanceInfo(perf_info.as_mut_ptr(), std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32) };
    if ok == 0 {
        return Err(FreeError::Other("GetPerformanceInfo failed".into()));
    }
    let perf_info = unsafe { perf_info.assume_init() };
    Ok(perf_info.SystemCache as u64 * perf_info.PageSize as u64)
}
#[cfg(target_os = "macos")]
fn get_system_buffers() -> Result<u64, FreeError> {
    use std::process::Command;
    // vm_statでバッファページ数を取得
    let output = Command::new("vm_stat").output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut buffer_pages = 0u64;
    let mut page_size = 4096u64;
    for line in output_str.lines() {
        if line.contains("page size of") {
            if let Some(size_str) = line.split_whitespace().last() {
                page_size = size_str.parse::<u64>().unwrap_or(4096);
            }
        } else if line.starts_with("Pages speculative:") {
            if let Some(num_str) = line.split(':').nth(1) {
                buffer_pages = num_str.trim().trim_end_matches('.').parse::<u64>().unwrap_or(0);
            }
        }
    }
    Ok(buffer_pages * page_size)
} 