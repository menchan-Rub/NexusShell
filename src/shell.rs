use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use anyhow::{Result, anyhow, Context};
use colored::Colorize;

use crate::config::ShellConfig;
use crate::prompt::PromptManager;
use crate::history::HistoryManager;
use crate::plugins::PluginManager;
use crate::themes::ThemeManager;
use crate::builtins::BuiltinManager;

/// シェルオプション
#[derive(Debug, Clone)]
pub struct ShellOptions {
    /// 対話モードかどうか
    pub is_interactive: bool,
    /// ログインシェルかどうか
    pub is_login: bool,
    /// プロファイルを読み込まないかどうか
    pub no_profile: bool,
    /// RCファイルを読み込まないかどうか
    pub no_rc: bool,
    /// デバッグモードかどうか
    pub debug_mode: bool,
}

impl Default for ShellOptions {
    fn default() -> Self {
        Self {
            is_interactive: true,
            is_login: false,
            no_profile: false,
            no_rc: false,
            debug_mode: false,
        }
    }
}

/// シェル環境
pub struct Shell {
    /// 設定
    config: ShellConfig,
    /// オプション
    options: ShellOptions,
    /// 環境変数
    env_vars: HashMap<String, String>,
    /// 現在のディレクトリ
    current_dir: PathBuf,
    /// プロンプトマネージャー
    prompt_manager: PromptManager,
    /// 履歴マネージャー
    history_manager: HistoryManager,
    /// プラグインマネージャー
    plugin_manager: PluginManager,
    /// テーママネージャー
    theme_manager: ThemeManager,
    /// ビルトインコマンドマネージャー
    builtin_manager: BuiltinManager,
    /// エイリアス
    aliases: HashMap<String, String>,
    /// 関数
    functions: HashMap<String, String>,
    /// 終了コード
    last_exit_code: i32,
}

impl Shell {
    /// 新しいシェルを作成します
    pub fn new(config: ShellConfig, options: ShellOptions) -> Result<Self> {
        // 現在のディレクトリを取得
        let current_dir = env::current_dir().context("カレントディレクトリの取得に失敗しました")?;
        
        // 環境変数を取得
        let env_vars: HashMap<String, String> = env::vars().collect();
        
        // マネージャーを初期化
        let prompt_manager = PromptManager::new(&config.prompt);
        let history_manager = HistoryManager::new(&config.history);
        let plugin_manager = PluginManager::new(&config.plugins);
        let theme_manager = ThemeManager::new(&config.theme);
        let builtin_manager = BuiltinManager::new();
        
        let mut shell = Self {
            config,
            options,
            env_vars,
            current_dir,
            prompt_manager,
            history_manager,
            plugin_manager,
            theme_manager,
            builtin_manager,
            aliases: HashMap::new(),
            functions: HashMap::new(),
            last_exit_code: 0,
        };
        
        // 基本的な環境を設定
        shell.setup_environment()?;
        
        Ok(shell)
    }
    
    /// 基本的な環境を設定します
    fn setup_environment(&mut self) -> Result<()> {
        // 基本的な環境変数を設定
        self.set_env_var("SHELL", env::current_exe()?.to_string_lossy().to_string());
        self.set_env_var("PWD", self.current_dir.to_string_lossy().to_string());
        
        // PATHを確保
        if !self.env_vars.contains_key("PATH") {
            // デフォルトのPATHを設定
            #[cfg(unix)]
            self.set_env_var("PATH", "/usr/local/bin:/usr/bin:/bin");
            #[cfg(windows)]
            self.set_env_var("PATH", "C:\\Windows\\System32;C:\\Windows;C:\\Windows\\System32\\Wbem");
        }
        
        // デフォルトのエイリアスを設定
        self.setup_default_aliases();
        
        Ok(())
    }
    
    /// デフォルトのエイリアスを設定します
    fn setup_default_aliases(&mut self) {
        // 基本的なエイリアス
        self.aliases.insert("ls".to_string(), "ls --color=auto".to_string());
        self.aliases.insert("ll".to_string(), "ls -l".to_string());
        self.aliases.insert("la".to_string(), "ls -a".to_string());
        
        // プラットフォーム固有のエイリアス
        #[cfg(unix)]
        {
            self.aliases.insert("grep".to_string(), "grep --color=auto".to_string());
            self.aliases.insert("egrep".to_string(), "egrep --color=auto".to_string());
            self.aliases.insert("fgrep".to_string(), "fgrep --color=auto".to_string());
        }
        
        #[cfg(windows)]
        {
            self.aliases.insert("cls".to_string(), "clear".to_string());
            self.aliases.insert("dir".to_string(), "ls".to_string());
        }
    }
    
    /// ログインプロファイルを読み込みます
    pub fn load_login_profile(&mut self) -> Result<()> {
        if self.options.debug_mode {
            println!("{} ログインプロファイルを読み込みます...", "[DEBUG]".bright_yellow());
        }
        
        // プロファイルファイルのパスを取得
        let profile_path = self.get_profile_path();
        
        if let Some(path) = profile_path {
            if path.exists() {
                self.execute_script(path)?;
            } else if self.options.debug_mode {
                println!("{} プロファイルファイルが見つかりません: {:?}", "[DEBUG]".bright_yellow(), path);
            }
        }
        
        Ok(())
    }
    
    /// RCファイルを読み込みます
    pub fn load_rc_file(&mut self) -> Result<()> {
        if self.options.debug_mode {
            println!("{} RCファイルを読み込みます...", "[DEBUG]".bright_yellow());
        }
        
        // RCファイルのパスを取得
        let rc_path = self.get_rc_path();
        
        if let Some(path) = rc_path {
            if path.exists() {
                self.execute_script(path)?;
            } else if self.options.debug_mode {
                println!("{} RCファイルが見つかりません: {:?}", "[DEBUG]".bright_yellow(), path);
            }
        }
        
        Ok(())
    }
    
    /// プロファイルファイルのパスを取得します
    fn get_profile_path(&self) -> Option<PathBuf> {
        if let Some(home) = dirs::home_dir() {
            #[cfg(unix)]
            return Some(home.join(".profile"));
            
            #[cfg(windows)]
            return Some(home.join("profile.ps1"));
        }
        None
    }
    
    /// RCファイルのパスを取得します
    fn get_rc_path(&self) -> Option<PathBuf> {
        if let Some(home) = dirs::home_dir() {
            #[cfg(unix)]
            return Some(home.join(".nexusshellrc"));
            
            #[cfg(windows)]
            return Some(home.join(".nexusshellrc.ps1"));
        }
        None
    }
    
    /// 履歴ファイルのパスを取得します
    pub fn get_history_path(&self) -> Option<PathBuf> {
        self.history_manager.get_history_path()
    }
    
    /// コマンドを実行します
    pub fn execute_command(&mut self, command: &str) -> Result<i32> {
        if command.trim().is_empty() {
            return Ok(0);
        }
        
        if self.options.debug_mode {
            println!("{} コマンドを実行します: {}", "[DEBUG]".bright_yellow(), command);
        }
        
        // コマンドを展開
        let expanded_command = self.expand_aliases(command);
        
        // 変数展開
        let expanded_command = self.expand_variables(&expanded_command);
        
        // 複数のコマンドを処理（セミコロンで区切る）
        if expanded_command.contains(';') {
            let commands: Vec<&str> = expanded_command.split(';').collect();
            let mut last_exit_code = 0;
            
            for cmd in commands {
                last_exit_code = self.execute_single_command(cmd.trim())?;
                self.last_exit_code = last_exit_code;
                
                // 失敗したらエラーコードを返す
                if self.config.stop_on_error && last_exit_code != 0 {
                    return Ok(last_exit_code);
                }
            }
            
            return Ok(last_exit_code);
        }
        
        // 単一コマンドの実行
        let exit_code = self.execute_single_command(&expanded_command)?;
        self.last_exit_code = exit_code;
        
        Ok(exit_code)
    }
    
    /// 単一のコマンドを実行します
    fn execute_single_command(&mut self, command: &str) -> Result<i32> {
        let command = command.trim();
        
        if command.is_empty() {
            return Ok(0);
        }
        
        // ディレクトリ変更のショートカット
        if command.starts_with("cd ") {
            let args: Vec<&str> = command.split_whitespace().collect();
            return self.change_directory(args.get(1).copied());
        }
        
        // ビルトインコマンドかどうかをチェック
        if let Some((cmd, args)) = command.split_once(char::is_whitespace) {
            if self.builtin_manager.has_builtin(cmd) {
                let args_vec: Vec<&str> = args.split_whitespace().collect();
                return self.builtin_manager.execute(cmd, &args_vec, self);
            }
        } else if self.builtin_manager.has_builtin(command) {
            return self.builtin_manager.execute(command, &[], self);
        }
        
        // 外部コマンドの実行
        self.execute_external_command(command)
    }
    
    /// 外部コマンドを実行します
    fn execute_external_command(&self, command: &str) -> Result<i32> {
        // コマンドとパラメータを分割
        let mut parts = command.split_whitespace();
        let command_name = parts.next().ok_or_else(|| anyhow!("無効なコマンドです"))?;
        let args: Vec<&str> = parts.collect();
        
        // コマンドを実行
        let mut cmd = Command::new(command_name);
        cmd.args(args)
            .current_dir(&self.current_dir)
            .envs(&self.env_vars);
        
        // コマンドを実行
        match cmd.status() {
            Ok(status) => {
                let exit_code = status.code().unwrap_or(if status.success() { 0 } else { 1 });
                Ok(exit_code)
            },
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    eprintln!("{}: コマンドが見つかりません: {}", "エラー".bright_red(), command_name);
                    Ok(127) // コマンドが見つからない場合の標準的な終了コード
                } else {
                    Err(anyhow!("コマンドの実行に失敗しました: {}", err))
                }
            }
        }
    }
    
    /// スクリプトを実行します
    pub fn execute_script<P: AsRef<Path>>(&mut self, script_path: P) -> Result<i32> {
        let path = script_path.as_ref();
        
        if self.options.debug_mode {
            println!("{} スクリプトを実行します: {:?}", "[DEBUG]".bright_yellow(), path);
        }
        
        // ファイルを読み込む
        let content = fs::read_to_string(path)
            .with_context(|| format!("スクリプトファイルの読み込みに失敗しました: {:?}", path))?;
        
        // 各行を実行
        let lines: Vec<&str> = content.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        
        let mut last_exit_code = 0;
        
        for line in lines {
            last_exit_code = self.execute_command(line)?;
            
            // 失敗したらエラーコードを返す
            if self.config.stop_on_error && last_exit_code != 0 {
                return Ok(last_exit_code);
            }
        }
        
        Ok(last_exit_code)
    }
    
    /// エイリアスを展開します
    fn expand_aliases(&self, command: &str) -> String {
        // シンプルな実装: 最初の単語がエイリアスかどうかをチェック
        let mut parts = command.splitn(2, char::is_whitespace);
        let first_word = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("");
        
        if let Some(alias) = self.aliases.get(first_word) {
            if rest.is_empty() {
                alias.clone()
            } else {
                format!("{} {}", alias, rest)
            }
        } else {
            command.to_string()
        }
    }
    
    /// 変数を展開します
    fn expand_variables(&self, command: &str) -> String {
        let mut result = command.to_string();
        
        // 変数展開（シンプルな実装）
        for (name, value) in &self.env_vars {
            let var_pattern = format!("${{{}}}", name);
            result = result.replace(&var_pattern, value);
            
            let var_pattern = format!("${}", name);
            result = result.replace(&var_pattern, value);
        }
        
        // 特殊変数
        result = result.replace("$?", &self.last_exit_code.to_string());
        result = result.replace("$$", &std::process::id().to_string());
        
        // ホームディレクトリの展開
        if let Some(home) = dirs::home_dir() {
            result = result.replace("~", &home.to_string_lossy());
        }
        
        result
    }
    
    /// 環境変数を設定します
    pub fn set_env_var(&mut self, name: &str, value: String) {
        self.env_vars.insert(name.to_string(), value.clone());
        env::set_var(name, value);
    }
    
    /// 環境変数を取得します
    pub fn get_env_var(&self, name: &str) -> Option<&String> {
        self.env_vars.get(name)
    }
    
    /// ディレクトリを変更します
    pub fn change_directory(&mut self, dir: Option<&str>) -> Result<i32> {
        let target_dir = match dir {
            Some("-") => {
                // 前のディレクトリに戻る
                if let Some(prev) = self.get_env_var("OLDPWD") {
                    PathBuf::from(prev)
                } else {
                    eprintln!("{}: OLDPWD が設定されていません", "エラー".bright_red());
                    return Ok(1);
                }
            },
            Some("~") | None => {
                // ホームディレクトリに移動
                dirs::home_dir().ok_or_else(|| anyhow!("ホームディレクトリが見つかりません"))?
            },
            Some(path) if path.starts_with('~') => {
                // ~ユーザー展開
                let home = dirs::home_dir().ok_or_else(|| anyhow!("ホームディレクトリが見つかりません"))?;
                
                if path == "~" {
                    home
                } else if path.starts_with("~/") {
                    home.join(&path[2..])
                } else {
                    // ~user形式はまだサポートしていません
                    PathBuf::from(path)
                }
            },
            Some(path) => {
                // 通常のパス
                if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    self.current_dir.join(path)
                }
            }
        };
        
        // ディレクトリの存在チェック
        if !target_dir.is_dir() {
            eprintln!("{}: ディレクトリではありません: {}", "エラー".bright_red(), target_dir.display());
            return Ok(1);
        }
        
        // 古いディレクトリを保存
        let old_dir = self.current_dir.clone();
        
        // ディレクトリを変更
        env::set_current_dir(&target_dir)
            .with_context(|| format!("ディレクトリの変更に失敗しました: {}", target_dir.display()))?;
        
        // 環境変数を更新
        self.set_env_var("OLDPWD", old_dir.to_string_lossy().to_string());
        self.set_env_var("PWD", target_dir.to_string_lossy().to_string());
        
        // 現在のディレクトリを更新
        self.current_dir = target_dir;
        
        Ok(0)
    }
    
    /// クリーンアップ処理を行います
    pub fn cleanup(&mut self) -> Result<()> {
        // 履歴の保存
        self.history_manager.save_history()?;
        
        // プラグインのクリーンアップ
        self.plugin_manager.cleanup()?;
        
        Ok(())
    }
    
    /// プロンプトマネージャーを取得します
    pub fn get_prompt_manager(&self) -> &PromptManager {
        &self.prompt_manager
    }
    
    /// ビルトインコマンドの一覧を取得します
    pub fn get_builtins(&self) -> Vec<String> {
        self.builtin_manager.get_builtin_names()
    }
    
    /// エイリアスの一覧を取得します
    pub fn get_aliases(&self) -> HashMap<String, String> {
        self.aliases.clone()
    }
    
    /// デバッグモードかどうかを取得します
    pub fn is_debug_mode(&self) -> bool {
        self.options.debug_mode
    }
    
    /// 現在のディレクトリを取得します
    pub fn get_current_dir(&self) -> &Path {
        &self.current_dir
    }
    
    /// 最後の終了コードを取得します
    pub fn get_last_exit_code(&self) -> i32 {
        self.last_exit_code
    }
} 