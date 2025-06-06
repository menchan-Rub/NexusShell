use std::collections::HashMap;
use std::path::Path;
use std::env;
use std::fs;
use anyhow::{Result, anyhow};
use clap::{Arg, ArgAction, Command};

use crate::BuiltinCommand;

/// whichコマンド
pub struct WhichCommand {
    /// コマンド名
    name: String,
    /// コマンドの説明
    description: String,
    /// 使用方法
    usage: String,
}

impl WhichCommand {
    /// 新しいWhichCommandインスタンスを作成
    pub fn new() -> Self {
        Self {
            name: "which".to_string(),
            description: "指定されたコマンドのパスを表示します".to_string(),
            usage: "which [オプション] コマンド名...".to_string(),
        }
    }
    
    /// コマンドパーサーを作成
    fn build_parser(&self) -> Command {
        Command::new("which")
            .about("指定されたコマンドのパスを表示します")
            .arg(
                Arg::new("all")
                    .short('a')
                    .long("all")
                    .help("一致するすべてのパスを表示します")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("silent")
                    .short('s')
                    .long("silent")
                    .help("エラーメッセージを表示しません")
                    .action(ArgAction::SetTrue)
            )
            .arg(
                Arg::new("commands")
                    .help("パスを検索するコマンド名のリスト")
                    .required(true)
                    .action(ArgAction::Append)
            )
    }
    
    /// PATHからコマンドを検索
    fn search_command(&self, command: &str, show_all: bool, env: &HashMap<String, String>) -> Vec<String> {
        let mut result = Vec::new();
        
        // シェルから組み込みコマンドのリストを取得
        let builtin_commands = self.get_builtin_commands_from_shell(env);
        
        // 組み込みコマンドのチェック
        if builtin_commands.contains(&command.to_string()) {
            result.push(format!("{}: シェル組み込みコマンド", command));
            if !show_all {
                return result;
            }
        }
        
        // 環境変数からPATHを取得
        let path_var = match env.get("PATH") {
            Some(path) => path.clone(),
            None => {
                // 環境からPATHを直接取得（フォールバック）
                match env::var("PATH") {
                    Ok(p) => p,
                    Err(_) => return result,
                }
            }
        };
        
        // パスセパレータ
        #[cfg(windows)]
        let separator = ";";
        #[cfg(not(windows))]
        let separator = ":";
        
        // PATHの各ディレクトリを検索
        for dir in path_var.split(separator) {
            let path = Path::new(dir).join(
                // Windows環境では.exeを自動追加
                #[cfg(windows)]
                if command.ends_with(".exe") {
                    command.to_string()
                } else {
                    format!("{}.exe", command)
                }
                #[cfg(not(windows))]
                command
            );
            
            if path.exists() && is_executable(&path) {
                result.push(path.to_string_lossy().to_string());
                if !show_all {
                    break;
                }
            }
        }
        
        result
    }
    
    /// シェルから組み込みコマンドのリストを取得
    fn get_builtin_commands_from_shell(&self, env: &HashMap<String, String>) -> Vec<String> {
        // シェルコンテキストから組み込みコマンドリストを取得
        if let Some(builtin_list) = env.get("SHELL_BUILTINS") {
            // SHELL_BUILTINS環境変数が設定されている場合はそこから取得
            return builtin_list.split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
        
        // シェルのタイプを判断
        let shell_type = if let Some(shell_path) = env.get("SHELL") {
            if shell_path.contains("bash") {
                "bash"
            } else if shell_path.contains("zsh") {
                "zsh"
            } else if shell_path.contains("fish") {
                "fish"
            } else if shell_path.contains("powershell") || shell_path.contains("pwsh") {
                "powershell"
            } else if shell_path.contains("cmd") {
                "cmd"
            } else {
                "unknown"
            }
        } else {
            "unknown"
        };
        
        // シェルごとの組み込みコマンドリストを返す
        match shell_type {
            "bash" => vec![
                "alias", "bg", "bind", "break", "builtin", "caller", "cd", "command", 
                "compgen", "complete", "compopt", "continue", "declare", "dirs", "disown", 
                "echo", "enable", "eval", "exec", "exit", "export", "false", "fc", "fg", 
                "getopts", "hash", "help", "history", "jobs", "kill", "let", "local", 
                "logout", "mapfile", "popd", "printf", "pushd", "pwd", "read", "readarray", 
                "readonly", "return", "set", "shift", "shopt", "source", "suspend", "test", 
                "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias", 
                "unset", "wait", "which"
            ].iter().map(|&s| s.to_string()).collect(),
            
            "zsh" => vec![
                "alias", "autoload", "bg", "bindkey", "builtin", "cd", "command", "compdef", 
                "compdescribe", "compfiles", "compgroups", "compquote", "comptags", "comptry", 
                "compvalues", "declare", "dirs", "disable", "disown", "echo", "echotc", 
                "emulate", "enable", "eval", "exec", "exit", "export", "false", "fc", "fg", 
                "functions", "getcap", "getln", "getopts", "hash", "history", "jobs", "kill", 
                "let", "limit", "local", "log", "logout", "popd", "print", "printf", "pushd", 
                "pushln", "pwd", "r", "read", "readonly", "rehash", "return", "sched", "set", 
                "setcap", "setopt", "shift", "source", "suspend", "test", "times", "trap", 
                "true", "ttyctl", "type", "typeset", "ulimit", "umask", "unalias", "unfunction", 
                "unhash", "unlimit", "unset", "unsetopt", "vared", "wait", "whence", "where", 
                "which", "zcompile", "zformat", "zftp", "zle", "zmodload", "zparseopts", "zprof", 
                "zpty", "zregexparse", "zsocket", "zstyle", "ztcp"
            ].iter().map(|&s| s.to_string()).collect(),
            
            "fish" => vec![
                "alias", "and", "begin", "bg", "bind", "block", "breakpoint", "builtin", 
                "case", "cd", "command", "commandline", "complete", "contains", "continue", 
                "count", "disown", "echo", "else", "emit", "end", "eval", "exec", "exit", 
                "fg", "for", "function", "functions", "history", "if", "jobs", "math", 
                "not", "or", "pwd", "random", "read", "return", "set", "status", "switch", 
                "test", "time", "type", "ulimit", "umask", "while"
            ].iter().map(|&s| s.to_string()).collect(),
            
            "powershell" => vec![
                "Break", "Clear-Host", "Continue", "Exit", "ForEach-Object", 
                "Get-Command", "Get-Help", "Get-History", "Get-Job", "Get-Location", 
                "Get-Member", "Get-Module", "Get-Variable", "Group-Object", 
                "Invoke-Command", "Invoke-Expression", "Invoke-History", "Measure-Object", 
                "New-Alias", "New-Item", "New-Module", "New-Object", "New-Variable", 
                "Out-Default", "Out-Host", "Out-Null", "Read-Host", "Remove-Item", 
                "Remove-Variable", "Select-Object", "Set-Alias", "Set-Location", 
                "Set-Variable", "Sort-Object", "Start-Job", "Wait-Job", "Where-Object", 
                "Write-Host", "Write-Output"
            ].iter().map(|&s| s.to_string()).collect(),
            
            "cmd" => vec![
                "assoc", "call", "cd", "chdir", "cls", "color", "copy", "date", "del", 
                "dir", "echo", "endlocal", "erase", "exit", "for", "goto", "if", "md", 
                "mkdir", "mklink", "move", "path", "pause", "popd", "prompt", "pushd", 
                "rd", "rem", "ren", "rename", "rmdir", "set", "setlocal", "shift", 
                "start", "time", "title", "type", "ver", "verify", "vol"
            ].iter().map(|&s| s.to_string()).collect(),
            
            // デフォルトの組み込みコマンド（NexusShell固有）
            _ => vec![
                "alias", "cd", "echo", "exit", "export", "help", "history", "jobs", 
                "pwd", "set", "source", "type", "unset", "which", "nexus", "pipeline",
                "execute", "transform", "deploy", "analyze"
            ].iter().map(|&s| s.to_string()).collect(),
        }
    }
}

/// ファイルが実行可能かチェック
fn is_executable(path: &Path) -> bool {
    // Unix環境では実行権限をチェック
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(path) {
            let permissions = metadata.permissions();
            return permissions.mode() & 0o111 != 0;
        }
        false
    }
    
    // Windows環境では.exe, .bat, .cmd, .ps1などの拡張子をチェック
    #[cfg(windows)]
    {
        if let Some(extension) = path.extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            return ext == "exe" || ext == "bat" || ext == "cmd" || ext == "ps1";
        }
        false
    }
}

impl BuiltinCommand for WhichCommand {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn usage(&self) -> &str {
        &self.usage
    }
    
    fn execute(&self, args: Vec<String>, env: &mut HashMap<String, String>) -> Result<String> {
        // 引数解析
        let matches = match self.build_parser().try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) => return Err(anyhow!("引数解析エラー: {}", e)),
        };
        
        // オプション取得
        let show_all = matches.get_flag("all");
        let silent = matches.get_flag("silent");
        
        // コマンドリストを取得
        let commands = matches.get_many::<String>("commands")
            .ok_or_else(|| anyhow!("コマンド名が指定されていません"))?
            .cloned()
            .collect::<Vec<_>>();
        
        let mut result = String::new();
        let mut exit_code = 0;
        
        // 各コマンドに対して検索
        for command in commands {
            let paths = self.search_command(&command, show_all, env);
            
            if paths.is_empty() {
                if !silent {
                    result.push_str(&format!("{}: 見つかりません\n", command));
                }
                exit_code = 1;
            } else {
                for path in paths {
                    result.push_str(&format!("{}\n", path));
                }
            }
        }
        
        // 終了コードを環境変数に設定
        env.insert("?".to_string(), exit_code.to_string());
        
        Ok(result)
    }
    
    fn help(&self) -> String {
        format!(
            "{}\n\n使用法: {}\n\nオプション:\n  -a, --all       一致するすべてのパスを表示します\n  -s, --silent    エラーメッセージを表示しません\n\n引数:\n  commands...    パスを検索するコマンド名のリスト\n\n例:\n  which ls          lsコマンドのパスを表示\n  which -a python   pythonコマンドのすべてのパスを表示\n",
            self.description,
            self.usage
        )
    }
} 