use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use sysinfo::{ProcessExt, System, SystemExt};
use tabular::{Row, Table};
use std::convert::TryFrom;

/// プロセス一覧を表示するコマンド
///
/// システム上で実行中のプロセスの一覧と、それぞれのプロセスに関する
/// 情報（プロセスID、ユーザー名、CPU使用率、メモリ使用量、実行ファイル名など）を表示します。
///
/// # 使用例
///
/// ```bash
/// ps                   # 基本的なプロセス情報を表示
/// ps --all             # すべてのプロセスを表示
/// ps --sort cpu        # CPU使用率でソート
/// ps --sort mem        # メモリ使用量でソート
/// ps --filter firefox  # 特定のプロセスでフィルタリング
/// ```
pub struct PsCommand;

/// プロセス情報を格納する構造体
#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    user: String,
    cpu_usage: f32,
    memory_usage: u64,
    memory_percent: f32,
    start_time: DateTime<Local>,
    command: String,
    status: String,
}

#[async_trait]
impl BuiltinCommand for PsCommand {
    fn name(&self) -> &'static str {
        "ps"
    }

    fn description(&self) -> &'static str {
        "プロセス一覧を表示します"
    }

    fn usage(&self) -> &'static str {
        "ps [オプション]\n\n\
        オプション:\n\
        --all, -a             すべてのプロセスを表示\n\
        --sort <フィールド>   指定したフィールドでソート（pid, cpu, mem, time, command）\n\
        --filter <パターン>   指定したパターンでプロセスをフィルタリング\n\
        --long, -l            詳細なプロセス情報を表示\n\
        --tree, -t            プロセスツリー形式で表示\n\
        --headers             ヘッダー行を表示（デフォルト：表示する）\n\
        --no-headers          ヘッダー行を表示しない\n\
        --help                このヘルプメッセージを表示"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        // コマンドライン引数の解析
        let mut show_all = false;
        let mut sort_by = "pid";
        let mut filter_pattern = String::new();
        let mut show_long = false;
        let mut show_tree = false;
        let mut show_headers = true;
        
        let mut i = 1; // 最初の引数はコマンド名なのでスキップ
        while i < context.args.len() {
            match context.args[i].as_str() {
                "--all" | "-a" => {
                    show_all = true;
                }
                "--sort" => {
                    if i + 1 < context.args.len() {
                        sort_by = &context.args[i + 1];
                        i += 1;
                    } else {
                        return Err(anyhow!("--sort オプションには値が必要です"));
                    }
                }
                "--filter" => {
                    if i + 1 < context.args.len() {
                        filter_pattern = context.args[i + 1].clone();
                        i += 1;
                    } else {
                        return Err(anyhow!("--filter オプションには値が必要です"));
                    }
                }
                "--long" | "-l" => {
                    show_long = true;
                }
                "--tree" | "-t" => {
                    show_tree = true;
                }
                "--headers" => {
                    show_headers = true;
                }
                "--no-headers" => {
                    show_headers = false;
                }
                "--help" => {
                    return Ok(CommandResult::success().with_stdout(self.usage().as_bytes().to_vec()));
                }
                arg if arg.starts_with("-") => {
                    return Err(anyhow!("不明なオプション: {}", arg));
                }
                _ => {}
            }
            i += 1;
        }
        
        // プロセス情報の取得
        let processes = get_process_info(show_all)?;
        
        // フィルタリング
        let mut filtered_processes = if !filter_pattern.is_empty() {
            processes.into_iter()
                .filter(|p| p.command.to_lowercase().contains(&filter_pattern.to_lowercase()))
                .collect()
        } else {
            processes
        };
        
        // ソート
        match sort_by {
            "pid" => filtered_processes.sort_by_key(|p| p.pid),
            "cpu" => filtered_processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal)),
            "mem" => filtered_processes.sort_by(|a, b| b.memory_usage.cmp(&a.memory_usage)),
            "time" => filtered_processes.sort_by(|a, b| a.start_time.cmp(&b.start_time)),
            "command" => filtered_processes.sort_by(|a, b| a.command.cmp(&b.command)),
            _ => return Err(anyhow!("不明なソートフィールド: {}", sort_by)),
        }
        
        // 結果の整形と出力
        let output = if show_tree {
            format_process_tree(&filtered_processes, show_headers, show_long)
        } else {
            format_process_list(&filtered_processes, show_headers, show_long)
        };
        
        Ok(CommandResult::success().with_stdout(output.as_bytes().to_vec()))
    }
}

/// プロセス情報を取得する関数
fn get_process_info(show_all: bool) -> Result<Vec<ProcessInfo>> {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let mut processes = Vec::new();
    
    for (pid, process) in sys.processes() {
        // 現在のユーザーIDを取得
        let current_uid = match users::get_current_uid() {
            Some(uid) => uid,
            None => 0,
        };
        
        // 自分のプロセスのみ表示するかどうか
        if !show_all && process.user_id() != Some(current_uid) {
            continue;
        }
        
        // ユーザー名を取得
        let user = if let Some(uid) = process.user_id() {
            match users::get_user_by_uid(uid) {
                Some(user) => user.name().to_string_lossy().into_owned(),
                None => uid.to_string(),
            }
        } else {
            "unknown".to_string()
        };
        
        // プロセス開始時間を取得
        let start_time = match process.start_time() {
            secs if secs > 0 => {
                let now = chrono::Local::now();
                let seconds_ago = now.timestamp() as u64 - secs;
                now - chrono::Duration::seconds(seconds_ago as i64)
            },
            _ => chrono::Local::now(), // フォールバック
        };
        
        let status = match process.status() {
            sysinfo::ProcessStatus::Run => "running",
            sysinfo::ProcessStatus::Sleep => "sleeping",
            sysinfo::ProcessStatus::Stop => "stopped",
            sysinfo::ProcessStatus::Zombie => "zombie",
            sysinfo::ProcessStatus::Idle => "idle",
            _ => "unknown",
        };
        
        // 親プロセスIDを取得
        let ppid = process.parent().unwrap_or(0);
        
        processes.push(ProcessInfo {
            pid: *pid,
            ppid,
            user,
            cpu_usage: process.cpu_usage(),
            memory_usage: process.memory(),
            memory_percent: (process.memory() as f32 / sys.total_memory() as f32) * 100.0,
            start_time,
            command: process.name().to_string(),
            status: status.to_string(),
        });
    }
    
    Ok(processes)
}

/// プロセス一覧を整形する関数
fn format_process_list(processes: &[ProcessInfo], show_headers: bool, show_long: bool) -> String {
    let mut table = Table::new("{:<8} {:<12} {:<8} {:<8} {:<12} {:<20}");
    
    if show_headers {
        table.add_row(
            Row::new()
                .with_cell("PID")
                .with_cell("USER")
                .with_cell("CPU%")
                .with_cell("MEM%")
                .with_cell("TIME")
                .with_cell("COMMAND")
        );
    }
    
    for process in processes {
        let time_str = if show_long {
            process.start_time.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            process.start_time.format("%H:%M:%S").to_string()
        };
        
        table.add_row(
            Row::new()
                .with_cell(process.pid.to_string())
                .with_cell(&process.user)
                .with_cell(format!("{:.1}", process.cpu_usage))
                .with_cell(format!("{:.1}", process.memory_percent))
                .with_cell(time_str)
                .with_cell(&process.command)
        );
    }
    
    table.to_string()
}

/// プロセスツリーを整形する関数
fn format_process_tree(processes: &[ProcessInfo], show_headers: bool, show_long: bool) -> String {
    // プロセスをPIDでマップ化
    let process_map: HashMap<u32, &ProcessInfo> = processes.iter()
        .map(|p| (p.pid, p))
        .collect();
    
    // プロセスを親子関係でマップ化
    let mut child_map: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        child_map.entry(process.ppid).or_default().push(process.pid);
    }
    
    // ルートプロセス（親がいないか、親が表示対象外）を特定
    let root_pids: Vec<u32> = processes.iter()
        .filter(|p| !process_map.contains_key(&p.ppid) || p.ppid == 0)
        .map(|p| p.pid)
        .collect();
    
    let mut result = String::new();
    
    if show_headers {
        result.push_str(&format!("{:<8} {:<12} {:<8} {:<8} {:<12} {:<20}\n", 
            "PID", "USER", "CPU%", "MEM%", "TIME", "COMMAND"));
    }
    
    // ツリーを再帰的に構築
    for pid in root_pids {
        build_process_tree(&mut result, pid, &process_map, &child_map, show_long, 0);
    }
    
    result
}

/// プロセスツリーを再帰的に構築する関数
fn build_process_tree(
    result: &mut String,
    pid: u32,
    process_map: &HashMap<u32, &ProcessInfo>,
    child_map: &HashMap<u32, Vec<u32>>,
    show_long: bool,
    depth: usize
) {
    if let Some(process) = process_map.get(&pid) {
        let time_str = if show_long {
            process.start_time.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            process.start_time.format("%H:%M:%S").to_string()
        };
        
        let prefix = if depth > 0 {
            format!("{:width$}\\_ ", "", width = depth * 2)
        } else {
            "".to_string()
        };
        
        result.push_str(&format!("{:<8} {:<12} {:<8} {:<8} {:<12} {}{}\n",
            process.pid,
            &process.user,
            format!("{:.1}", process.cpu_usage),
            format!("{:.1}", process.memory_percent),
            time_str,
            prefix,
            &process.command
        ));
        
        // 子プロセスを再帰的に処理
        if let Some(children) = child_map.get(&pid) {
            for &child_pid in children {
                build_process_tree(result, child_pid, process_map, child_map, show_long, depth + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_ps_basic_execution() {
        let cmd = PsCommand;
        let context = CommandContext {
            args: vec!["ps".to_string()],
            env_vars: HashMap::new(),
            current_dir: std::path::PathBuf::from("/"),
            stdin_connected: false,
            stdout_connected: true,
            stderr_connected: true,
        };
        
        let result = cmd.execute(context).await;
        assert!(result.is_ok());
    }
} 