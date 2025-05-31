use std::collections::HashMap;
use std::fmt;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// ジョブコントロール用のコマンド
pub struct JobsCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl JobsCommand {
    /// 新しいJobsCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "jobs".to_string(),
            description: "実行中のジョブを一覧表示します".to_string(),
            usage: "jobs [-lnprs] [jobID ...]".to_string(),
        }
    }
    
    /// オプションパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("jobs")
            .about("実行中のジョブを一覧表示します")
            .arg(
                Arg::new("list")
                    .short('l')
                    .help("プロセスIDに関する情報も表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("pid")
                    .short('p')
                    .help("プロセスグループリーダーのプロセスIDのみを表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("running")
                    .short('r')
                    .help("実行中のジョブのみを表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("stopped")
                    .short('s')
                    .help("停止しているジョブのみを表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("no_status")
                    .short('n')
                    .help("ジョブのステータスを表示しません")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("job_ids")
                    .help("表示するジョブID")
                    .action(ArgAction::Append)
            )
    }
    
    /// ジョブの状態を表す列挙型
    #[derive(Debug, Clone, PartialEq)]
    enum JobState {
        Running,
        Stopped,
        Done,
    }
    
    /// ジョブ情報を表す構造体
    #[derive(Debug, Clone)]
    struct JobInfo {
        id: usize,
        pid: u32,
        state: JobState,
        command: String,
        is_current: bool,
        is_previous: bool,
    }
    
    /// ジョブリストをモック
    fn mock_jobs(&self) -> Vec<JobInfo> {
        vec![
            JobInfo {
                id: 1,
                pid: 1234,
                state: JobState::Running,
                command: "sleep 100".to_string(),
                is_current: true,
                is_previous: false,
            },
            JobInfo {
                id: 2,
                pid: 1235,
                state: JobState::Stopped,
                command: "vim file.txt".to_string(),
                is_current: false,
                is_previous: true,
            },
            JobInfo {
                id: 3,
                pid: 1236,
                state: JobState::Running,
                command: "find / -name \"*.rs\"".to_string(),
                is_current: false,
                is_previous: false,
            },
        ]
    }
    
    /// ジョブリストを表示
    fn display_jobs(&self, show_pids: bool, running_only: bool, stopped_only: bool, no_status: bool) -> String {
        let jobs = self.mock_jobs();
        let mut result = String::new();
        
        for job in jobs {
            // フィルタリング
            if running_only && job.state != JobState::Running {
                continue;
            }
            if stopped_only && job.state != JobState::Stopped {
                continue;
            }
            
            // 現在/前回のジョブマーク
            let marker = if job.is_current {
                '+'
            } else if job.is_previous {
                '-'
            } else {
                ' '
            };
            
            // ステータス文字列
            let status = if no_status {
                "".to_string()
            } else {
                match job.state {
                    JobState::Running => "実行中".to_string(),
                    JobState::Stopped => "停止".to_string(),
                    JobState::Done => "完了".to_string(),
                }
            };
            
            // PID表示
            if show_pids {
                result.push_str(&format!("[{}]{} {} {} {}\n", job.id, marker, job.pid, status, job.command));
            } else {
                result.push_str(&format!("[{}]{} {} {}\n", job.id, marker, status, job.command));
            }
        }
        
        if result.is_empty() {
            "現在実行中のジョブはありません\n".to_string()
        } else {
            result
        }
    }
    
    /// PIDのみ表示
    fn display_pids(&self, running_only: bool, stopped_only: bool) -> String {
        let jobs = self.mock_jobs();
        let mut result = String::new();
        
        for job in jobs {
            // フィルタリング
            if running_only && job.state != JobState::Running {
                continue;
            }
            if stopped_only && job.state != JobState::Stopped {
                continue;
            }
            
            result.push_str(&format!("{}\n", job.pid));
        }
        
        result
    }
    
    /// 現在のジョブID（%+または%%に対応）を見つける
    fn find_current_job_id(&self) -> usize {
        let jobs = self.mock_jobs();
        jobs.iter()
            .find(|job| job.is_current)
            .map(|job| job.id)
            .unwrap_or(0)
    }
    
    /// 前のジョブID（%-に対応）を見つける
    fn find_previous_job_id(&self) -> usize {
        let jobs = self.mock_jobs();
        jobs.iter()
            .find(|job| job.is_previous)
            .map(|job| job.id)
            .unwrap_or(0)
    }
    
    /// 特定のジョブIDのみを表示
    fn display_specific_jobs(&self, job_ids: &[usize], show_pids: bool, no_status: bool) -> String {
        let jobs = self.mock_jobs();
        let filtered_jobs: Vec<_> = jobs.iter()
            .filter(|job| job_ids.contains(&job.id))
            .collect();
            
        if filtered_jobs.is_empty() {
            return "指定されたジョブは存在しません。".to_string();
        }
        
        let mut result = String::new();
        
        for job in filtered_jobs {
            let status = match job.state {
                JobState::Running => "実行中",
                JobState::Stopped => "停止",
                JobState::Done => "完了",
            };
            
            let current_marker = if job.is_current {
                "+"
            } else if job.is_previous {
                "-"
            } else {
                " "
            };
            
            if show_pids {
                if no_status {
                    result.push_str(&format!("[{}]{} {}\n", job.id, current_marker, job.pid));
                } else {
                    result.push_str(&format!("[{}]{} {} {}\n", job.id, current_marker, status, job.pid));
                }
            } else {
                if no_status {
                    result.push_str(&format!("[{}]{} {}\n", job.id, current_marker, job.command));
                } else {
                    result.push_str(&format!("[{}]{} {} {}\n", job.id, current_marker, status, job.command));
                }
            }
        }
        
        result
    }
}

impl BuiltinCommand for JobsCommand {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn usage(&self) -> &str {
        &self.usage
    }
    
    fn execute(&self, args: Vec<String>, _env: &mut HashMap<String, String>) -> Result<String> {
        // 引数解析
        let matches = match self.build_parser().try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) => return Err(anyhow!("引数解析エラー: {}", e)),
        };
        
        // オプション取得
        let show_pids = matches.get_flag("list");
        let pid_only = matches.get_flag("pid");
        let running_only = matches.get_flag("running");
        let stopped_only = matches.get_flag("stopped");
        let no_status = matches.get_flag("no_status");
        
        // ジョブIDでフィルタリング
        let job_ids: Vec<usize> = matches.get_many::<String>("job_ids").map(|ids| {
            ids.map(|id_str| {
                // %数字 形式か、単なる数字か、%+, %-形式をサポート
                let id_str = id_str.trim();
                if id_str.starts_with('%') {
                    match &id_str[1..] {
                        "+" | "%" => self.find_current_job_id(),  // 現在のジョブ
                        "-" => self.find_previous_job_id(),      // 一つ前のジョブ
                        num => num.parse::<usize>().unwrap_or(0),
                    }
                } else {
                    // 数字だけの場合もジョブID
                    id_str.parse::<usize>().unwrap_or(0)
                }
            }).filter(|&id| id > 0).collect::<Vec<_>>()
        }).unwrap_or_default();

        // 指定されたジョブIDのみを表示
        if !job_ids.is_empty() {
            return self.display_specific_jobs(&job_ids, show_pids, no_status);
        }
        
        // ジョブ情報の表示
        if pid_only {
            Ok(self.display_pids(running_only, stopped_only))
        } else {
            Ok(self.display_jobs(show_pids, running_only, stopped_only, no_status))
        }
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\nオプション:\n  -l        プロセスIDも表示\n  -p        プロセスIDのみ表示\n  -r        実行中のジョブのみ表示\n  -s        停止中のジョブのみ表示\n  -n        ステータスを表示しない\n\n引数:\n  jobID     表示するジョブID\n",
            self.description,
            self.usage
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_display_jobs() {
        let cmd = JobsCommand::new();
        let result = cmd.display_jobs(false, false, false, false);
        
        // ジョブリストが表示されていることを確認
        assert!(result.contains("[1]+ 実行中 sleep 100"));
        assert!(result.contains("[2]- 停止 vim file.txt"));
    }
    
    #[test]
    fn test_display_pids() {
        let cmd = JobsCommand::new();
        let result = cmd.display_pids(false, false);
        
        // PIDのみが表示されていることを確認
        assert!(result.contains("1234"));
        assert!(result.contains("1235"));
        assert!(result.contains("1236"));
    }
} 