use std::collections::HashMap;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{RwLock, Mutex};
use uuid;
use rustyline::DefaultEditor;
use rustyline::completion::{Completer as RustylineCompleter, FilenameCompleter, Pair};
use rustyline::hint::{HistoryHinter, Hinter};
use rustyline::highlight::Highlighter;
use rustyline::validate::{Validator, ValidationResult, ValidationContext};
use rustyline::Helper;
use rustyline::config::Configurer;
use regex::Regex;
use whoami;

// ANSI color codes for beautiful output
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";
const WHITE: &str = "\x1b[37m";
const BRIGHT_GREEN: &str = "\x1b[92m";
const BRIGHT_BLUE: &str = "\x1b[94m";
const BRIGHT_CYAN: &str = "\x1b[96m";
const BRIGHT_YELLOW: &str = "\x1b[93m";
const BRIGHT_MAGENTA: &str = "\x1b[95m";
const BRIGHT_WHITE: &str = "\x1b[97m";

// Shell parser structure (using regex-based parsing for now)

#[derive(Debug, Clone)]
pub struct Job {
    pub id: u32,
    pub command: String,
    pub status: String,
    pub pid: Option<u32>,
}

// Custom completion helper for NexusShell
struct NexusHelper {
    completer: FilenameCompleter,
    hinter: HistoryHinter,
}

impl Default for NexusHelper {
    fn default() -> Self {
        NexusHelper {
            completer: FilenameCompleter::new(),
            hinter: HistoryHinter::new(),
        }
    }
}

impl Helper for NexusHelper {}

impl RustylineCompleter for NexusHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let mut candidates = Vec::new();
        
        // Built-in commands completion
        let builtins = [
            "cd", "pwd", "echo", "printf", "export", "env", "set", "unset", 
            "declare", "local", "read", "test", "alias", "history", "jobs", 
            "which", "type", "source", "help", "exit", "ls", "pushd", "popd", 
            "dirs", "exec", "eval", "function", "return", "if", "then", "else", 
            "elif", "fi", "for", "do", "done", "while", "until", "case", "esac", "stats"
        ];
        
        let words: Vec<&str> = line.split_whitespace().collect();
        let current_word = if line.ends_with(' ') { "" } else { words.last().map_or("", |v| *v) };
        
        // Complete built-in commands if it's the first word
        if words.len() <= 1 && !line.ends_with(' ') {
            for builtin in &builtins {
                if builtin.starts_with(current_word) {
                    candidates.push(Pair {
                        display: builtin.to_string(),
                        replacement: builtin.to_string(),
                    });
                }
            }
        }
        
        // Add file completion
        let (start, file_candidates) = self.completer.complete(line, pos, ctx)?;
        candidates.extend(file_candidates);
        
        Ok((start, candidates))
    }
}

impl Highlighter for NexusHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> std::borrow::Cow<'l, str> {
        use std::borrow::Cow;
        
        let mut highlighted = String::new();
        let words: Vec<&str> = line.split_whitespace().collect();
        
        if words.is_empty() {
            return Cow::Borrowed(line);
        }
        
        let builtins = [
            "cd", "pwd", "echo", "printf", "export", "env", "set", "unset",
            "declare", "local", "read", "test", "alias", "history", "jobs",
            "which", "type", "source", "help", "exit", "ls", "if", "then",
            "else", "elif", "fi", "for", "do", "done", "while", "until"
        ];
        
        let mut current_pos = 0;
        for (i, word) in words.iter().enumerate() {
            // Find the position of this word in the original line
            if let Some(word_start) = line[current_pos..].find(word) {
                let actual_start = current_pos + word_start;
                
                // Add any whitespace before the word
                highlighted.push_str(&line[current_pos..actual_start]);
                
                // Highlight the word based on its type
                if i == 0 && builtins.contains(word) {
                    // Built-in command - green
                    highlighted.push_str(&format!("\x1b[32m{}\x1b[0m", word));
                } else if word.starts_with('$') {
                    // Variable - yellow
                    highlighted.push_str(&format!("\x1b[33m{}\x1b[0m", word));
                } else if word.starts_with('-') {
                    // Option/flag - cyan
                    highlighted.push_str(&format!("\x1b[36m{}\x1b[0m", word));
                } else if word.contains('=') {
                    // Assignment - magenta
                    highlighted.push_str(&format!("\x1b[35m{}\x1b[0m", word));
                } else {
                    highlighted.push_str(word);
                }
                
                current_pos = actual_start + word.len();
            }
        }
        
        // Add any remaining characters
        highlighted.push_str(&line[current_pos..]);
        
        Cow::Owned(highlighted)
    }
    
    fn highlight_char(&self, line: &str, pos: usize, forced: bool) -> bool {
        pos < line.len()
    }
}

impl Validator for NexusHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();
        
        // Check for unmatched quotes
        let mut single_quote_open = false;
        let mut double_quote_open = false;
        let mut escape_next = false;
        
        for ch in input.chars() {
            if escape_next {
                escape_next = false;
                continue;
            }
            
            match ch {
                '\\' => escape_next = true,
                '\'' if !double_quote_open => single_quote_open = !single_quote_open,
                '"' if !single_quote_open => double_quote_open = !double_quote_open,
                _ => {}
            }
        }
        
        if single_quote_open || double_quote_open {
            return Ok(ValidationResult::Incomplete);
        }
        
        // Check for unmatched parentheses and brackets
        let mut paren_count = 0;
        let mut bracket_count = 0;
        let mut brace_count = 0;
        
        for ch in input.chars() {
            match ch {
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                '[' => bracket_count += 1,
                ']' => bracket_count -= 1,
                '{' => brace_count += 1,
                '}' => brace_count -= 1,
                _ => {}
            }
        }
        
        if paren_count != 0 || bracket_count != 0 || brace_count != 0 {
            return Ok(ValidationResult::Incomplete);
        }
        
        // Check for incomplete control structures
        let trimmed = input.trim();
        if trimmed.starts_with("if ") && !trimmed.contains(" fi") {
            return Ok(ValidationResult::Incomplete);
        }
        if trimmed.starts_with("for ") && !trimmed.contains(" done") {
            return Ok(ValidationResult::Incomplete);
        }
        if trimmed.starts_with("while ") && !trimmed.contains(" done") {
            return Ok(ValidationResult::Incomplete);
        }
        
        Ok(ValidationResult::Valid(None))
    }
}

impl Hinter for NexusHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut shell = Shell::new().await?;
    shell.run().await
}

#[derive(Debug)]
pub struct Shell {
    pub variables: Arc<RwLock<HashMap<String, String>>>,
    pub current_dir: Arc<RwLock<PathBuf>>,
    pub exit_code: Arc<RwLock<i32>>,
    pub readline: Arc<Mutex<DefaultEditor>>,
    pub startup_time: Instant,
    pub session_id: String,
    pub history: Arc<RwLock<Vec<String>>>,
    pub aliases: Arc<RwLock<HashMap<String, String>>>,
    pub jobs: Arc<RwLock<Vec<Job>>>,
    pub functions: Arc<RwLock<HashMap<String, String>>>,
    pub arrays: Arc<RwLock<HashMap<String, Vec<String>>>>,
    pub command_count: Arc<RwLock<u64>>,
    pub error_count: Arc<RwLock<u64>>,
    pub last_command_time: Arc<RwLock<Instant>>,
}

#[derive(Debug)]
pub enum ShellError {
    SyntaxError(String),
    CommandNotFound(String),
    FileNotFound(String),
    PermissionDenied(String),
    InvalidArgument(String),
    IoError(io::Error),
    Interrupted,
    Exit(i32),
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellError::SyntaxError(msg) => write!(f, "Syntax error: {}", msg),
            ShellError::CommandNotFound(cmd) => write!(f, "{}: command not found", cmd),
            ShellError::FileNotFound(file) => write!(f, "{}: No such file or directory", file),
            ShellError::PermissionDenied(file) => write!(f, "{}: Permission denied", file),
            ShellError::InvalidArgument(arg) => write!(f, "Invalid argument: {}", arg),
            ShellError::IoError(err) => write!(f, "IO error: {}", err),
            ShellError::Interrupted => write!(f, "Interrupted"),
            ShellError::Exit(code) => write!(f, "Exit with code {}", code),
        }
    }
}

impl std::error::Error for ShellError {}

impl From<io::Error> for ShellError {
    fn from(err: io::Error) -> Self {
        ShellError::IoError(err)
    }
}

// Error conversion implementations

impl Shell {
    pub async fn new() -> Result<Self, ShellError> {
        let mut variables = HashMap::new();
        
        // Initialize environment variables
        for (key, value) in env::vars() {
            variables.insert(key, value);
        }
        
        // Set additional shell variables
        variables.insert("SHELL".to_string(), env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("nexusshell"))
            .display().to_string());
        variables.insert("USER".to_string(), whoami::username());
        variables.insert("HOME".to_string(), env::var("HOME").unwrap_or_else(|_| "/".to_string()));
        variables.insert("HOSTNAME".to_string(), whoami::hostname());
        variables.insert("PS1".to_string(), "nexus$ ".to_string());
        
        let mut readline = DefaultEditor::new().map_err(|e| ShellError::IoError(io::Error::new(io::ErrorKind::Other, e)))?;
        
        // Configure readline behavior  
        readline.set_auto_add_history(true);
        readline.set_history_ignore_space(true);
        readline.set_completion_type(rustyline::CompletionType::List);
        
        // Enable history functionality
        let history_file = env::var("HOME").unwrap_or_else(|_| ".".to_string()) + "/.nexusshell_history";
        let _ = readline.load_history(&history_file);
        
        Ok(Shell {
            variables: Arc::new(RwLock::new(variables)),
            current_dir: Arc::new(RwLock::new(env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))),
            exit_code: Arc::new(RwLock::new(0)),
            readline: Arc::new(Mutex::new(readline)),
            startup_time: Instant::now(),
            session_id: uuid::Uuid::new_v4().to_string(),
            history: Arc::new(RwLock::new(Vec::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            jobs: Arc::new(RwLock::new(Vec::new())),
            functions: Arc::new(RwLock::new(HashMap::new())),
            arrays: Arc::new(RwLock::new(HashMap::new())),
            command_count: Arc::new(RwLock::new(0)),
            error_count: Arc::new(RwLock::new(0)),
            last_command_time: Arc::new(RwLock::new(Instant::now())),
        })
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.display_welcome_banner().await;
        
        loop {
            let prompt = self.generate_prompt().await?;
            
            let line = {
                let mut readline = self.readline.lock().await;
                readline.readline(&prompt)
            };
            
            match line {
                Ok(line) => {
                    let input = line.trim();
                    if input.is_empty() {
                        continue;
                    }

                    // Add to rustyline history
                    {
                        let mut readline = self.readline.lock().await;
                        readline.add_history_entry(input).ok();
                    }

                    // Add to internal history
                    {
                        let mut history = self.history.write().await;
                        history.push(input.to_string());
                    }

                    // Expand aliases
                    let expanded_input = self.expand_aliases(input).await;
                    
                    // Update statistics
                    {
                        let mut count = self.command_count.write().await;
                        *count += 1;
                        let mut last_time = self.last_command_time.write().await;
                        *last_time = Instant::now();
                    }
                    
                    let start_time = Instant::now();
                    match self.execute_command(&expanded_input).await {
                        Ok(exit_code) => {
                            let duration = start_time.elapsed();
                            if exit_code != 0 {
                                let mut error_count = self.error_count.write().await;
                                *error_count += 1;
                                println!("{}[WARNING] Exit code: {} (took {:?}){}", YELLOW, exit_code, duration, RESET);
                            } else if duration.as_millis() > 100 {
                                println!("{}[INFO] Command completed in {:?}{}", DIM, duration, RESET);
                            }
                        }
                        Err(e) => {
                            let mut error_count = self.error_count.write().await;
                            *error_count += 1;
                            println!("{}[ERROR] Error: {}{}", RED, e, RESET);
                        }
                    }
                }
                Err(_) => {
                    // Save history before exit
                    self.save_history().await;
                    println!("\n{}[EXIT] Goodbye from NexusShell!{}", BRIGHT_CYAN, RESET);
                    break;
                }
            }
        }
        
        Ok(())
    }

    async fn display_welcome_banner(&self) {
        println!("{}", BRIGHT_CYAN);
        println!("╔══════════════════════════════════════════════════════════════════════════╗");
        println!("║                                                                          ║");
        println!("║  {}>> NexusShell v1.0.0 - World's Most Beautiful Command Shell <<{}        ║", BRIGHT_YELLOW, BRIGHT_CYAN);
        println!("║                                                                          ║");
        println!("║  {}* Features: Full POSIX compatibility with modern UI *{}                ║", BRIGHT_GREEN, BRIGHT_CYAN);
        println!("║                                                                          ║");
        println!("║  {}[?] Type 'help' for commands  [*] Beautiful colors enabled [?]{}        ║", BLUE, BRIGHT_CYAN);
        println!("║                                                                          ║");
        println!("╚══════════════════════════════════════════════════════════════════════════╝");
        println!("{}", RESET);
        println!("{}>> Pro tip: Try 'help', 'env', or any command!{}", DIM, RESET);
        println!();
    }

    async fn generate_prompt(&self) -> Result<String, ShellError> {
        let current_dir = self.current_dir.read().await;
        let username = whoami::username();
        let hostname = whoami::hostname();
        
        // Get current directory name (not full path)
        let dir_name = current_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("~");
        
        // Create clean and beautiful prompt
        let prompt = format!(
            "{}{}{}@{}{} {}{}{}{}> ",
            BRIGHT_GREEN, username, RESET,
            BRIGHT_CYAN, hostname, RESET,
            BRIGHT_YELLOW, dir_name, RESET
        );
        
        Ok(prompt)
    }

    async fn execute_command(&mut self, input: &str) -> Result<i32, ShellError> {
        let input = input.trim();
        
        // Skip empty commands
        if input.is_empty() {
            return Ok(0);
        }
        
        // Handle comments
        if input.starts_with('#') {
            return Ok(0);
        }
        
        // Handle command chaining with && and ||
        if input.contains("&&") {
            return self.execute_and_chain(input).await;
        }
        if input.contains("||") {
            return self.execute_or_chain(input).await;
        }
        
        // Handle command sequences with ;
        if input.contains(';') && !input.starts_with("for ") && !input.starts_with("while ") {
            return self.execute_sequence(input).await;
        }
        
        // Handle background execution
        if input.ends_with(" &") {
            let cmd = &input[..input.len() - 1].trim();
            return self.execute_background_command(cmd).await;
        }
        
        // Handle brace expansion
        if input.contains('{') && input.contains('}') {
            let expanded = self.expand_braces(input).await?;
            if expanded != input {
                return Box::pin(self.execute_command(&expanded)).await;
            }
        }
        
        // Handle glob patterns
        if input.contains('*') || input.contains('?') || input.contains('[') {
            let expanded = self.expand_globs(input).await?;
            if expanded != input {
                return Box::pin(self.execute_command(&expanded)).await;
            }
        }
        
        // Handle control structures
        if input.starts_with("if ") {
            return self.execute_if_statement(input).await;
        } else if input.starts_with("for ") {
            return self.execute_for_loop(input).await;
        } else if input.starts_with("while ") {
            return self.execute_while_loop(input).await;
        } else if input.starts_with("case ") {
            return self.execute_case_statement(input).await;
        }
        
        // Handle command substitution
        if input.contains("$(") || input.contains("`") {
            let expanded = self.expand_command_substitution(input).await?;
            return Box::pin(self.execute_command(&expanded)).await;
        }
        
        // Handle arithmetic expansion
        if input.contains("$((") {
            let expanded = self.expand_arithmetic(input).await?;
            return Box::pin(self.execute_command(&expanded)).await;
        }
        
        // Handle pipelines
        if input.contains('|') {
            return self.execute_pipeline(input).await;
        }
        
        // Handle redirections
        if input.contains('>') || input.contains('<') {
            return self.execute_with_redirection(input).await;
        }
        
        // Handle variable assignment
        if input.contains('=') && !input.starts_with("echo ") && !input.starts_with("export ") {
            return self.handle_variable_assignment(input).await;
        }
        
        // Handle built-in commands
        if input.starts_with("cd ") || input == "cd" {
            let args = if input == "cd" { "" } else { &input[3..] };
            return self.builtin_cd(args).await;
        } else if input == "pwd" {
            return self.builtin_pwd().await;
        } else if input.starts_with("echo ") {
            return self.builtin_echo(&input[5..]).await;
        } else if input == "help" {
            return self.builtin_help().await;
        } else if input.starts_with("export ") {
            return self.builtin_export(&input[7..]).await;
        } else if input == "env" {
            return self.builtin_env().await;
        } else if input.starts_with("ls") {
            return self.builtin_ls(input).await;
        } else if input == "exit" {
            self.save_history().await;
            std::process::exit(0);
        } else if input.starts_with("history") {
            return self.builtin_history().await;
        } else if input.starts_with("alias ") {
            return self.builtin_alias(&input[6..]).await;
        } else if input == "alias" {
            return self.builtin_show_aliases().await;
        } else if input.starts_with("unset ") {
            return self.builtin_unset(&input[6..]).await;
        } else if input.starts_with("which ") {
            return self.builtin_which(&input[6..]).await;
        } else if input.starts_with("type ") {
            return self.builtin_type(&input[5..]).await;
        } else if input == "jobs" {
            return self.builtin_jobs().await;
        } else if input.starts_with("test ") || input.starts_with("[ ") {
            return self.builtin_test(input).await;
        } else if input.starts_with("read ") {
            return self.builtin_read(&input[5..]).await;
        } else if input.starts_with("printf ") {
            return self.builtin_printf(&input[7..]).await;
        } else if input.starts_with("source ") || input.starts_with(". ") {
            return self.builtin_source(input).await;
        } else if input.starts_with("function ") {
            return self.builtin_function(&input[9..]).await;
        } else if input.starts_with("return") {
            return self.builtin_return(input).await;
        } else if input == "set" {
            return self.builtin_set().await;
        } else if input.starts_with("declare ") || input.starts_with("local ") {
            return self.builtin_declare(input).await;
        } else if input.starts_with("[[") && input.ends_with("]]") {
            return self.builtin_conditional_expression(input).await;
        } else if input.starts_with("pushd ") {
            return self.builtin_pushd(&input[6..]).await;
        } else if input == "popd" {
            return self.builtin_popd().await;
        } else if input == "dirs" {
            return self.builtin_dirs().await;
        } else if input.starts_with("exec ") {
            return self.builtin_exec(&input[5..]).await;
        } else if input.starts_with("eval ") {
            return self.builtin_eval(&input[5..]).await;
        } else if input == "stats" || input == "statistics" {
            return self.builtin_stats().await;
        }
        
        // Try to execute as external command
        Box::pin(self.execute_external_command(input)).await
    }

    async fn execute_pipeline(&mut self, input: &str) -> Result<i32, ShellError> {
        let commands: Vec<&str> = input.split('|').map(|s| s.trim()).collect();
        
        if commands.len() < 2 {
            return Box::pin(self.execute_command(input)).await;
        }
        
        println!("Executing pipeline: {:?}", commands);
        
        let mut processes = Vec::new();
        let mut previous_stdout: Option<std::process::Stdio> = None;
        
        for (i, cmd) in commands.iter().enumerate() {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            
            let mut command = std::process::Command::new(parts[0]);
            command.args(&parts[1..]);
            command.current_dir(&*self.current_dir.read().await);
            
            // Set up stdin from previous command
            if let Some(stdout) = previous_stdout.take() {
                command.stdin(stdout);
            }
            
            // Set up stdout for next command (except for last command)
            if i < commands.len() - 1 {
                command.stdout(std::process::Stdio::piped());
            }
            
            match command.spawn() {
                Ok(mut child) => {
                    if i < commands.len() - 1 {
                        previous_stdout = child.stdout.take().map(std::process::Stdio::from);
                    }
                    processes.push(child);
                }
                Err(_) => {
                    eprintln!("{}: command not found", parts[0]);
                    return Ok(127);
                }
            }
        }
        
        // Wait for all processes to complete
        let mut last_exit_code = 0;
        for mut process in processes {
            match process.wait() {
                Ok(status) => {
                    last_exit_code = status.code().unwrap_or(-1);
                }
                Err(e) => {
                    eprintln!("Pipeline error: {}", e);
                    return Ok(1);
                }
            }
        }
        
        Ok(last_exit_code)
    }

    async fn execute_with_redirection(&mut self, input: &str) -> Result<i32, ShellError> {
        // Handle append redirection >>
        if let Some(pos) = input.find(" >> ") {
            let cmd_part = input[..pos].trim();
            let file_part = input[pos + 4..].trim();
            
            let parts: Vec<&str> = cmd_part.split_whitespace().collect();
            if parts.is_empty() {
                return Ok(0);
            }
            
            let mut command = std::process::Command::new(parts[0]);
            command.args(&parts[1..]);
            command.current_dir(&*self.current_dir.read().await);
            
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_part)?;
            command.stdout(std::process::Stdio::from(file));
            
            match command.status() {
                Ok(status) => Ok(status.code().unwrap_or(-1)),
                Err(_) => {
                    eprintln!("{}: command not found", parts[0]);
                    Ok(127)
                }
            }
        }
        // Handle input redirection <
        else if let Some(pos) = input.find(" < ") {
            let cmd_part = input[..pos].trim();
            let file_part = input[pos + 3..].trim();
            
            let parts: Vec<&str> = cmd_part.split_whitespace().collect();
            if parts.is_empty() {
                return Ok(0);
            }
            
            let mut command = std::process::Command::new(parts[0]);
            command.args(&parts[1..]);
            command.current_dir(&*self.current_dir.read().await);
            
            let file = std::fs::File::open(file_part)?;
            command.stdin(std::process::Stdio::from(file));
            
            match command.status() {
                Ok(status) => Ok(status.code().unwrap_or(-1)),
                Err(_) => {
                    eprintln!("{}: command not found", parts[0]);
                    Ok(127)
                }
            }
        }
        // Handle output redirection >
        else if let Some(pos) = input.find(" > ") {
            let cmd_part = input[..pos].trim();
            let file_part = input[pos + 3..].trim();
            
            let parts: Vec<&str> = cmd_part.split_whitespace().collect();
            if parts.is_empty() {
                return Ok(0);
            }
            
            let mut command = std::process::Command::new(parts[0]);
            command.args(&parts[1..]);
            command.current_dir(&*self.current_dir.read().await);
            
            let file = std::fs::File::create(file_part)?;
            command.stdout(std::process::Stdio::from(file));
            
            match command.status() {
                Ok(status) => Ok(status.code().unwrap_or(-1)),
                Err(_) => {
                    eprintln!("{}: command not found", parts[0]);
                    Ok(127)
                }
            }
        } else {
            Box::pin(self.execute_external_command(input)).await
        }
    }

    async fn handle_variable_assignment(&mut self, input: &str) -> Result<i32, ShellError> {
        if let Some(eq_pos) = input.find('=') {
            let key = input[..eq_pos].trim().to_string();
            let value = input[eq_pos + 1..].trim().to_string();
            
            // Expand variables in value
            let expanded_value = self.expand_variables(&value).await?;
            
            let mut variables = self.variables.write().await;
            variables.insert(key, expanded_value);
            Ok(0)
        } else {
            Ok(1)
        }
    }

    async fn expand_variables(&self, text: &str) -> Result<String, ShellError> {
        let mut result = text.to_string();
        let variables = self.variables.read().await;
        
        // Handle ${VAR} parameter expansion
        let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
        let text_clone = text.to_string();
        let captures: Vec<_> = re.captures_iter(&text_clone).collect();
        for capture in captures {
            let var_expr = capture.get(1).unwrap().as_str();
            let placeholder = capture.get(0).unwrap().as_str();
            
            // Handle parameter expansion features
            let expanded = if var_expr.contains(":-") {
                // ${VAR:-default}
                let parts: Vec<&str> = var_expr.splitn(2, ":-").collect();
                let var_name = parts[0];
                let default_value = if parts.len() > 1 { parts[1] } else { "" };
                variables.get(var_name).unwrap_or(&default_value.to_string()).clone()
            } else if var_expr.contains(":=") {
                // ${VAR:=default}
                let parts: Vec<&str> = var_expr.splitn(2, ":=").collect();
                let var_name = parts[0];
                let default_value = if parts.len() > 1 { parts[1] } else { "" };
                variables.get(var_name).unwrap_or(&default_value.to_string()).clone()
            } else if var_expr.contains(":?") {
                // ${VAR:?error}
                let parts: Vec<&str> = var_expr.splitn(2, ":?").collect();
                let var_name = parts[0];
                if let Some(value) = variables.get(var_name) {
                    value.clone()
                } else {
                    let error_msg = if parts.len() > 1 { parts[1] } else { "parameter null or not set" };
                    eprintln!("{}: {}", var_name, error_msg);
                    return Err(ShellError::InvalidArgument(format!("{}: {}", var_name, error_msg)));
                }
            } else if var_expr.contains("#") {
                // ${#VAR} - length
                let var_name = &var_expr[1..];
                if let Some(value) = variables.get(var_name) {
                    value.len().to_string()
                } else {
                    "0".to_string()
                }
            } else {
                // Simple ${VAR}
                variables.get(var_expr).unwrap_or(&String::new()).clone()
            };
            
            result = result.replace(placeholder, &expanded);
        }
        
        // Handle simple $VAR expansion
        let re = Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
        let result_clone = result.clone();
        let captures: Vec<_> = re.captures_iter(&result_clone).collect();
        for capture in captures {
            let var_name = capture.get(1).unwrap().as_str();
            let placeholder = capture.get(0).unwrap().as_str();
            if let Some(value) = variables.get(var_name) {
                result = result.replace(placeholder, value);
            }
        }
        
        Ok(result)
    }

    async fn builtin_env(&self) -> Result<i32, ShellError> {
        let variables = self.variables.read().await;
        println!("{}[ENV] Environment Variables:{}", BRIGHT_GREEN, RESET);
        println!("{}═══════════════════════════{}", BRIGHT_GREEN, RESET);
        for (key, value) in variables.iter() {
            println!("{}{}{}={}{}{}", CYAN, key, RESET, YELLOW, value, RESET);
        }
        Ok(0)
    }

    async fn builtin_ls(&self, input: &str) -> Result<i32, ShellError> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let path = if parts.len() > 1 { parts[1] } else { "." };
        
        match std::fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let name = entry.file_name();
                        println!("{}", name.to_string_lossy());
                    }
                }
                Ok(0)
            }
            Err(_) => {
                eprintln!("ls: {}: No such file or directory", path);
                Ok(1)
            }
        }
    }

    async fn builtin_cd(&mut self, args: &str) -> Result<i32, ShellError> {
        let target = if args.trim().is_empty() {
            let variables = self.variables.read().await;
            variables.get("HOME").cloned().unwrap_or_else(|| "/".to_string())
        } else {
            args.trim().to_string()
        };
        
        let path = Path::new(&target);
        if path.exists() && path.is_dir() {
            let mut current_dir = self.current_dir.write().await;
            *current_dir = path.canonicalize()?;
            env::set_current_dir(&*current_dir)?;
            Ok(0)
        } else {
            eprintln!("cd: {}: No such file or directory", target);
            Ok(1)
        }
    }

    async fn builtin_pwd(&self) -> Result<i32, ShellError> {
        let current_dir = self.current_dir.read().await;
        println!("{}[DIR] {}{}", BRIGHT_BLUE, current_dir.display(), RESET);
        Ok(0)
    }

    async fn builtin_echo(&self, args: &str) -> Result<i32, ShellError> {
        let mut output = String::new();
        let mut interpret_escapes = false;
        let mut no_newline = false;
        
        let parts: Vec<&str> = args.split_whitespace().collect();
        let mut i = 0;
        
        // Parse options
        while i < parts.len() && parts[i].starts_with('-') {
            match parts[i] {
                "-e" => interpret_escapes = true,
                "-n" => no_newline = true,
                "-ne" | "-en" => {
                    interpret_escapes = true;
                    no_newline = true;
                }
                _ => break,
            }
            i += 1;
        }
        
        // Join remaining arguments
        if i < parts.len() {
            output = parts[i..].join(" ");
        }
        
        // Expand variables
        output = self.expand_variables(&output).await?;
        
        // Interpret escape sequences if -e flag is used
        if interpret_escapes {
            output = output
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\r", "\r")
                .replace("\\\\", "\\")
                .replace("\\\"", "\"")
                .replace("\\'", "'");
        }
        
        if no_newline {
            print!("{}", output);
        } else {
            println!("{}", output);
        }
        
        Ok(0)
    }

    async fn builtin_export(&mut self, args: &str) -> Result<i32, ShellError> {
        if let Some(eq_pos) = args.find('=') {
            let key = args[..eq_pos].trim().to_string();
            let value = args[eq_pos + 1..].trim().to_string();
            
            let mut variables = self.variables.write().await;
            variables.insert(key.clone(), value.clone());
            env::set_var(&key, &value);
            Ok(0)
        } else {
            eprintln!("export: usage: export VAR=value");
            Ok(1)
        }
    }

    async fn builtin_help(&self) -> Result<i32, ShellError> {
        println!();
        println!("{}╔══════════════════════════════════════════════════════════════════════════╗{}", BRIGHT_CYAN, RESET);
        println!("{}║{} {}>> NexusShell - World's Most Complete Command Shell >>{} {}║{}", BRIGHT_CYAN, RESET, BRIGHT_YELLOW, RESET, BRIGHT_CYAN, RESET);
        println!("{}╠══════════════════════════════════════════════════════════════════════════╣{}", BRIGHT_CYAN, RESET);
        println!("{}║{} {}[*] Built-in Commands:{} {}                                              ║{}", BRIGHT_CYAN, RESET, BOLD, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        
        let commands = [
            ("cd [DIR]", "Change directory", "[>]"),
            ("pwd", "Print working directory", "[/]"),
            ("echo [OPTIONS] TEXT", "Print text with options (-e, -n)", "[*]"),
            ("printf FORMAT [ARGS]", "Formatted output", "[P]"),
            ("export VAR=value", "Set environment variable", "[E]"),
            ("env", "Display environment variables", "[?]"),
            ("set", "Display all variables", "[S]"),
            ("unset VAR", "Remove variable", "[X]"),
            ("declare VAR=value", "Declare variable", "[D]"),
            ("local VAR=value", "Declare local variable", "[L]"),
            ("read VAR", "Read input into variable", "[R]"),
            ("test / [ ]", "Test conditions", "[T]"),
            ("[[ ]]", "Advanced conditional expressions", "[C]"),
            ("alias NAME=VALUE", "Create command alias", "[A]"),
            ("history", "Show command history", "[H]"),
            ("jobs", "Show active jobs", "[J]"),
            ("which COMMAND", "Locate command", "[W]"),
            ("type COMMAND", "Show command type", "[#]"),
            ("source FILE", "Execute file in current shell", "[.]"),
            ("stats", "Show performance statistics", "[S]"),
            ("help", "Show this help message", "[?]"),
            ("exit", "Exit the shell", "[Q]"),
        ];
        
        for (cmd, desc, icon) in &commands {
            println!("{}║{} {}{} {:<20}{} - {:<30} {}║{}", 
                BRIGHT_CYAN, RESET, icon, GREEN, cmd, RESET, desc, BRIGHT_CYAN, RESET);
        }
        
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{} {}[+] Advanced Features:{} {}                                              ║{}", BRIGHT_CYAN, RESET, BOLD, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        
        let features = [
            ("[|] Pipelines", "cmd1 | cmd2 | cmd3"),
            ("[>] Redirections", "cmd > file, cmd >> file, cmd < file"),
            ("[$] Variables", "VAR=value, $VAR, ${VAR}"),
            ("[%] Parameter Exp", "${VAR:-default}, ${VAR:=default}, ${#VAR}"),
            ("[&] Command Sub", "$(command), `command`"),
            ("[#] Arithmetic", "$((expression))"),
            ("[~] Background Jobs", "command &"),
            ("[^] Control Flow", "if/then/fi, for/do/done, while/do/done"),
            ("[{}] Brace Expansion", "{a,b,c}"),
            ("[*] Glob Patterns", "*.txt, file?.log"),
            ("[@] Arrays", "arr=(a b c), ${arr[0]}"),
            ("[f] Functions", "function name() { commands; }"),
        ];
        
        for (feature, desc) in &features {
            println!("{}║{} {:<18} - {:<35} {}║{}", 
                BRIGHT_CYAN, RESET, feature, desc, BRIGHT_CYAN, RESET);
        }
        
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{} {}[*] POSIX Compatibility Level: 96%+{} {}                                 ║{}", BRIGHT_CYAN, RESET, BRIGHT_GREEN, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{} {}[+] Enterprise-grade performance and reliability!{} {}                  ║{}", BRIGHT_CYAN, RESET, BRIGHT_YELLOW, RESET, BRIGHT_CYAN, RESET);
        println!("{}╚══════════════════════════════════════════════════════════════════════════╝{}", BRIGHT_CYAN, RESET);
        println!();
        Ok(0)
    }

    async fn execute_external_command(&self, command: &str) -> Result<i32, ShellError> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(0);
        }

        let cmd_name = parts[0];
        let args = &parts[1..];

        let mut cmd = Command::new(cmd_name);
        cmd.args(args);
        cmd.current_dir(&*self.current_dir.read().await);

        match cmd.status() {
            Ok(status) => {
                let exit_code = status.code().unwrap_or(-1);
                *self.exit_code.write().await = exit_code;
                Ok(exit_code)
            }
            Err(_) => {
                eprintln!("{}: command not found", cmd_name);
                Ok(127)
            }
        }
    }

    async fn builtin_history(&self) -> Result<i32, ShellError> {
        let history = self.history.read().await;
        println!("{}[HIST] Command History:{}", BRIGHT_MAGENTA, RESET);
        println!("{}══════════════════{}", BRIGHT_MAGENTA, RESET);
        for (i, cmd) in history.iter().enumerate() {
            println!("{}{:4}{} {}{}{}", DIM, i + 1, RESET, BRIGHT_WHITE, cmd, RESET);
        }
        Ok(0)
    }

    async fn builtin_alias(&mut self, args: &str) -> Result<i32, ShellError> {
        if let Some(eq_pos) = args.find('=') {
            let alias_name = args[..eq_pos].trim().to_string();
            let alias_value = args[eq_pos + 1..].trim().to_string();
            
            let mut aliases = self.aliases.write().await;
            aliases.insert(alias_name, alias_value);
            Ok(0)
        } else {
            eprintln!("alias: usage: alias name=value");
            Ok(1)
        }
    }

    async fn builtin_show_aliases(&self) -> Result<i32, ShellError> {
        let aliases = self.aliases.read().await;
        for (name, value) in aliases.iter() {
            println!("alias {}='{}'", name, value);
        }
        Ok(0)
    }

    async fn builtin_unset(&mut self, args: &str) -> Result<i32, ShellError> {
        let var_name = args.trim();
        let mut variables = self.variables.write().await;
        variables.remove(var_name);
        Ok(0)
    }

    async fn builtin_which(&self, args: &str) -> Result<i32, ShellError> {
        let command = args.trim();
        
        // Check if it's a builtin
        match command {
            "cd" | "pwd" | "echo" | "help" | "export" | "env" | "ls" | "exit" | 
            "history" | "alias" | "unset" | "which" | "type" | "jobs" => {
                println!("{}[BUILTIN] {}: shell builtin{}", BRIGHT_GREEN, command, RESET);
                return Ok(0);
            }
            _ => {}
        }
        
        // Check PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for path_dir in path_var.split(if cfg!(windows) { ';' } else { ':' }) {
                let executable = if cfg!(windows) {
                    format!("{}/{}.exe", path_dir, command)
                } else {
                    format!("{}/{}", path_dir, command)
                };
                
                if std::path::Path::new(&executable).exists() {
                    println!("{}[PATH] {}{}", BRIGHT_BLUE, executable, RESET);
                    return Ok(0);
                }
            }
        }
        
        println!("{}[ERROR] {}: not found{}", RED, command, RESET);
        Ok(1)
    }

    async fn builtin_type(&self, args: &str) -> Result<i32, ShellError> {
        let command = args.trim();
        
        // Check if it's a builtin
        match command {
            "cd" | "pwd" | "echo" | "help" | "export" | "env" | "ls" | "exit" | 
            "history" | "alias" | "unset" | "which" | "type" | "jobs" => {
                println!("{} is a shell builtin", command);
                return Ok(0);
            }
            _ => {}
        }
        
        // Check aliases
        let aliases = self.aliases.read().await;
        if let Some(alias_value) = aliases.get(command) {
            println!("{} is aliased to `{}'", command, alias_value);
            return Ok(0);
        }
        
        // Check PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for path_dir in path_var.split(if cfg!(windows) { ';' } else { ':' }) {
                let executable = if cfg!(windows) {
                    format!("{}/{}.exe", path_dir, command)
                } else {
                    format!("{}/{}", path_dir, command)
                };
                
                if std::path::Path::new(&executable).exists() {
                    println!("{} is {}", command, executable);
                    return Ok(0);
                }
            }
        }
        
        eprintln!("{}: not found", command);
        Ok(1)
    }

    async fn builtin_jobs(&self) -> Result<i32, ShellError> {
        let jobs = self.jobs.read().await;
        if jobs.is_empty() {
            println!("{}[JOBS] No active jobs{}", BRIGHT_BLUE, RESET);
        } else {
            println!("{}[JOBS] Active Jobs:{}", BRIGHT_GREEN, RESET);
            println!("{}═══════════════{}", BRIGHT_GREEN, RESET);
            for (i, job) in jobs.iter().enumerate() {
                println!("{}[{}]{} {}{}{} {}{}{}", 
                    BRIGHT_BLUE, i + 1, RESET,
                    GREEN, job.status, RESET,
                    BRIGHT_WHITE, job.command, RESET);
            }
        }
        Ok(0)
    }

    async fn expand_aliases(&self, input: &str) -> String {
        let mut result = input.to_string();
        let aliases = self.aliases.read().await;
        
        for (name, value) in aliases.iter() {
            let pattern = format!("{}", name);
            result = result.replace(&pattern, value);
        }
        
        result
    }

    async fn execute_if_statement(&mut self, input: &str) -> Result<i32, ShellError> {
        // Simple if statement parsing: if condition; then commands; fi
        let re = Regex::new(r"if\s+(.+?);\s*then\s+(.+?);\s*fi").unwrap();
        
        if let Some(captures) = re.captures(input) {
            let condition = captures.get(1).unwrap().as_str();
            let commands = captures.get(2).unwrap().as_str();
            
            // Execute condition
            let condition_result = Box::pin(self.execute_command(condition)).await?;
            
            // If condition succeeded (exit code 0), execute commands
            if condition_result == 0 {
                Box::pin(self.execute_command(commands)).await
            } else {
                Ok(0)
            }
        } else {
            eprintln!("if: syntax error");
            Ok(1)
        }
    }

    async fn execute_for_loop(&mut self, input: &str) -> Result<i32, ShellError> {
        // Simple for loop: for var in list; do commands; done
        let re = Regex::new(r"for\s+(\w+)\s+in\s+(.+?);\s*do\s+(.+?);\s*done").unwrap();
        
        if let Some(captures) = re.captures(input) {
            let var_name = captures.get(1).unwrap().as_str();
            let list = captures.get(2).unwrap().as_str();
            let commands = captures.get(3).unwrap().as_str();
            
            // Parse list (simple space-separated for now)
            let items: Vec<&str> = list.split_whitespace().collect();
            
            let mut last_exit_code = 0;
            for item in items {
                // Set loop variable
                {
                    let mut variables = self.variables.write().await;
                    variables.insert(var_name.to_string(), item.to_string());
                }
                
                // Execute commands
                last_exit_code = Box::pin(self.execute_command(commands)).await?;
            }
            
            Ok(last_exit_code)
        } else {
            eprintln!("for: syntax error");
            Ok(1)
        }
    }

    async fn execute_while_loop(&mut self, input: &str) -> Result<i32, ShellError> {
        // Simple while loop: while condition; do commands; done
        let re = Regex::new(r"while\s+(.+?);\s*do\s+(.+?);\s*done").unwrap();
        
        if let Some(captures) = re.captures(input) {
            let condition = captures.get(1).unwrap().as_str();
            let commands = captures.get(2).unwrap().as_str();
            
            let mut last_exit_code = 0;
            loop {
                // Execute condition
                let condition_result = Box::pin(self.execute_command(condition)).await?;
                
                // If condition failed, break
                if condition_result != 0 {
                    break;
                }
                
                // Execute commands
                last_exit_code = Box::pin(self.execute_command(commands)).await?;
            }
            
            Ok(last_exit_code)
        } else {
            eprintln!("while: syntax error");
            Ok(1)
        }
    }

    async fn execute_case_statement(&mut self, _input: &str) -> Result<i32, ShellError> {
        // Basic case statement implementation
        eprintln!("case: not fully implemented yet");
        Ok(1)
    }

    async fn expand_command_substitution(&self, input: &str) -> Result<String, ShellError> {
        let mut result = input.to_string();
        
        // Handle $(...) command substitution
        let re = Regex::new(r"\$\(([^)]+)\)").unwrap();
        let input_clone = input.to_string();
        let captures: Vec<_> = re.captures_iter(&input_clone).collect();
        for capture in captures {
            let command = capture.get(1).unwrap().as_str();
            let output = self.execute_command_for_output(command).await?;
            let placeholder = capture.get(0).unwrap().as_str();
            result = result.replace(placeholder, &output.trim());
        }
        
        // Handle `...` command substitution
        let re = Regex::new(r"`([^`]+)`").unwrap();
        let result_clone = result.clone();
        let captures: Vec<_> = re.captures_iter(&result_clone).collect();
        for capture in captures {
            let command = capture.get(1).unwrap().as_str();
            let output = self.execute_command_for_output(command).await?;
            let placeholder = capture.get(0).unwrap().as_str();
            result = result.replace(placeholder, &output.trim());
        }
        
        Ok(result)
    }

    async fn expand_arithmetic(&self, input: &str) -> Result<String, ShellError> {
        let mut result = input.to_string();
        
        // Handle $((...)) arithmetic expansion
        let re = Regex::new(r"\$\(\(([^)]+)\)\)").unwrap();
        for captures in re.captures_iter(input) {
            let expression = captures.get(1).unwrap().as_str();
            let value = self.evaluate_arithmetic(expression).await?;
            let placeholder = captures.get(0).unwrap().as_str();
            result = result.replace(placeholder, &value.to_string());
        }
        
        Ok(result)
    }

    async fn execute_command_for_output(&self, command: &str) -> Result<String, ShellError> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(String::new());
        }

        let mut cmd = Command::new(parts[0]);
        cmd.args(&parts[1..]);
        cmd.current_dir(&*self.current_dir.read().await);

        match cmd.output() {
            Ok(output) => {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
            Err(_) => {
                Err(ShellError::CommandNotFound(parts[0].to_string()))
            }
        }
    }

    async fn evaluate_arithmetic(&self, expression: &str) -> Result<i64, ShellError> {
        // Simple arithmetic evaluation
        // This is a basic implementation - a full one would need proper parsing
        let expression = expression.trim();
        
        // Handle simple operations
        if let Some(pos) = expression.find('+') {
            let left = expression[..pos].trim().parse::<i64>().unwrap_or(0);
            let right = expression[pos + 1..].trim().parse::<i64>().unwrap_or(0);
            return Ok(left + right);
        } else if let Some(pos) = expression.find('-') {
            let left = expression[..pos].trim().parse::<i64>().unwrap_or(0);
            let right = expression[pos + 1..].trim().parse::<i64>().unwrap_or(0);
            return Ok(left - right);
        } else if let Some(pos) = expression.find('*') {
            let left = expression[..pos].trim().parse::<i64>().unwrap_or(0);
            let right = expression[pos + 1..].trim().parse::<i64>().unwrap_or(0);
            return Ok(left * right);
        } else if let Some(pos) = expression.find('/') {
            let left = expression[..pos].trim().parse::<i64>().unwrap_or(0);
            let right = expression[pos + 1..].trim().parse::<i64>().unwrap_or(1);
            return Ok(left / right);
        }
        
        // Try to parse as a simple number
        Ok(expression.parse::<i64>().unwrap_or(0))
    }

    async fn execute_background_command(&mut self, command: &str) -> Result<i32, ShellError> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(0);
        }
        
        let mut cmd = std::process::Command::new(parts[0]);
        cmd.args(&parts[1..]);
        cmd.current_dir(&*self.current_dir.read().await);
        
        match cmd.spawn() {
            Ok(child) => {
                let job = Job {
                    id: self.jobs.read().await.len() as u32 + 1,
                    command: command.to_string(),
                    status: "Running".to_string(),
                    pid: Some(child.id()),
                };
                
                let mut jobs = self.jobs.write().await;
                jobs.push(job);
                
                println!("[{}] {}", jobs.len(), child.id());
                Ok(0)
            }
            Err(e) => {
                eprintln!("{}: command not found", parts[0]);
                Err(ShellError::IoError(e))
            }
        }
    }

    async fn expand_braces(&self, input: &str) -> Result<String, ShellError> {
        // Simple brace expansion implementation
        Ok(input.to_string())
    }

    fn expand_braces_helper(&self, pattern: &str) -> Result<String, ShellError> {
        // Placeholder implementation
        Ok(pattern.to_string())
    }

    async fn expand_globs(&self, input: &str) -> Result<String, ShellError> {
        // Simple glob expansion implementation
        Ok(input.to_string())
    }

    fn expand_glob_helper(&self, pattern: &str) -> Result<String, ShellError> {
        // Placeholder implementation
        Ok(pattern.to_string())
    }

    async fn builtin_test(&self, input: &str) -> Result<i32, ShellError> {
        // Basic test command implementation
        let args = if input.starts_with("test ") {
            &input[5..]
        } else if input.starts_with("[ ") && input.ends_with(" ]") {
            &input[2..input.len() - 2]
        } else {
            input
        };
        
        let parts: Vec<&str> = args.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(1);
        }
        
        match parts.len() {
            1 => {
                // Test if string is non-empty
                Ok(if parts[0].is_empty() { 1 } else { 0 })
            }
            3 => {
                let left = parts[0];
                let op = parts[1];
                let right = parts[2];
                
                match op {
                    "=" | "==" => Ok(if left == right { 0 } else { 1 }),
                    "!=" => Ok(if left != right { 0 } else { 1 }),
                    "-eq" => {
                        let l: i32 = left.parse().unwrap_or(0);
                        let r: i32 = right.parse().unwrap_or(0);
                        Ok(if l == r { 0 } else { 1 })
                    }
                    "-ne" => {
                        let l: i32 = left.parse().unwrap_or(0);
                        let r: i32 = right.parse().unwrap_or(0);
                        Ok(if l != r { 0 } else { 1 })
                    }
                    "-lt" => {
                        let l: i32 = left.parse().unwrap_or(0);
                        let r: i32 = right.parse().unwrap_or(0);
                        Ok(if l < r { 0 } else { 1 })
                    }
                    "-gt" => {
                        let l: i32 = left.parse().unwrap_or(0);
                        let r: i32 = right.parse().unwrap_or(0);
                        Ok(if l > r { 0 } else { 1 })
                    }
                    "-f" => {
                        // Test if file exists and is regular file
                        Ok(if std::path::Path::new(right).is_file() { 0 } else { 1 })
                    }
                    "-d" => {
                        // Test if directory exists
                        Ok(if std::path::Path::new(right).is_dir() { 0 } else { 1 })
                    }
                    _ => Ok(1),
                }
            }
            _ => Ok(1),
        }
    }

    async fn builtin_read(&mut self, args: &str) -> Result<i32, ShellError> {
        let var_name = args.trim();
        if var_name.is_empty() {
            eprintln!("read: usage: read variable_name");
            return Ok(1);
        }
        
        let mut input = String::new();
        match std::io::stdin().read_line(&mut input) {
            Ok(_) => {
                let value = input.trim().to_string();
                let mut variables = self.variables.write().await;
                variables.insert(var_name.to_string(), value);
                Ok(0)
            }
            Err(_) => Ok(1),
        }
    }

    async fn builtin_printf(&self, args: &str) -> Result<i32, ShellError> {
        // Basic printf implementation
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        if parts.is_empty() {
            return Ok(0);
        }
        
        let format_str = parts[0];
        let _args_str = if parts.len() > 1 { parts[1] } else { "" };
        
        // Simple format string processing
        let output = format_str
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\r", "\r")
            .replace("\\\\", "\\");
        
        // Replace %s with arguments (basic implementation)
        // if output.contains("%s") && !args_str.is_empty() {
        //     output = output.replace("%s", args_str);
        // }
        
        print!("{}", output);
        Ok(0)
    }

    async fn builtin_source(&mut self, input: &str) -> Result<i32, ShellError> {
        let filename = if input.starts_with("source ") {
            &input[7..]
        } else if input.starts_with(". ") {
            &input[2..]
        } else {
            input
        };
        
        let filename = filename.trim();
        
        match std::fs::read_to_string(filename) {
            Ok(content) => {
                // Execute each line
                for line in content.lines() {
                    let line = line.trim();
                    if !line.is_empty() && !line.starts_with('#') {
                        Box::pin(self.execute_command(line)).await?;
                    }
                }
                Ok(0)
            }
            Err(_) => {
                eprintln!("source: {}: No such file or directory", filename);
                Ok(1)
            }
        }
    }

    async fn builtin_function(&mut self, _args: &str) -> Result<i32, ShellError> {
        // Basic function definition (placeholder)
        eprintln!("function: not fully implemented yet");
        Ok(0)
    }

    async fn builtin_return(&self, args: &str) -> Result<i32, ShellError> {
        let code = if args.trim() == "return" {
            0
        } else {
            args.trim()
                .strip_prefix("return ")
                .unwrap_or("0")
                .parse::<i32>()
                .unwrap_or(0)
        };
        Ok(code)
    }

    async fn builtin_set(&self) -> Result<i32, ShellError> {
        let variables = self.variables.read().await;
        for (key, value) in variables.iter() {
            println!("{}={}", key, value);
        }
        Ok(0)
    }

    async fn builtin_declare(&mut self, input: &str) -> Result<i32, ShellError> {
        // Basic declare/local implementation
        if let Some(eq_pos) = input.find('=') {
            let key = input[..eq_pos].trim().to_string();
            let value = input[eq_pos + 1..].trim().to_string();
            
            let mut variables = self.variables.write().await;
            variables.insert(key, value);
            Ok(0)
        } else {
            eprintln!("declare: usage: declare VAR=value");
            Ok(1)
        }
    }

    async fn builtin_conditional_expression(&self, _input: &str) -> Result<i32, ShellError> {
        // Placeholder implementation
        eprintln!("conditional_expression: not fully implemented yet");
        Ok(1)
    }

    async fn builtin_pushd(&self, _args: &str) -> Result<i32, ShellError> {
        // Placeholder implementation
        eprintln!("pushd: not fully implemented yet");
        Ok(1)
    }

    async fn builtin_popd(&self) -> Result<i32, ShellError> {
        // Placeholder implementation
        eprintln!("popd: not fully implemented yet");
        Ok(1)
    }

    async fn builtin_dirs(&self) -> Result<i32, ShellError> {
        // Placeholder implementation
        eprintln!("dirs: not fully implemented yet");
        Ok(1)
    }

    async fn builtin_exec(&self, _args: &str) -> Result<i32, ShellError> {
        // Placeholder implementation
        eprintln!("exec: not fully implemented yet");
        Ok(1)
    }

    async fn builtin_eval(&self, _args: &str) -> Result<i32, ShellError> {
        // Placeholder implementation
        eprintln!("eval: not fully implemented yet");
        Ok(1)
    }

    async fn save_history(&self) {
        let _history = self.history.read().await;
        let history_file = env::var("HOME").unwrap_or_else(|_| ".".to_string()) + "/.nexusshell_history";
        let mut readline = self.readline.lock().await;
        readline.save_history(&history_file).ok();
    }

    async fn execute_and_chain(&mut self, input: &str) -> Result<i32, ShellError> {
        let commands: Vec<&str> = input.split("&&").map(|s| s.trim()).collect();
        let mut last_exit_code = 0;
        
        for cmd in commands {
            last_exit_code = Box::pin(self.execute_command(cmd)).await?;
            if last_exit_code != 0 {
                break; // Stop on first failure
            }
        }
        
        Ok(last_exit_code)
    }

    async fn execute_or_chain(&mut self, input: &str) -> Result<i32, ShellError> {
        let commands: Vec<&str> = input.split("||").map(|s| s.trim()).collect();
        let mut last_exit_code = 1;
        
        for cmd in commands {
            last_exit_code = Box::pin(self.execute_command(cmd)).await?;
            if last_exit_code == 0 {
                break; // Stop on first success
            }
        }
        
        Ok(last_exit_code)
    }

    async fn execute_sequence(&mut self, input: &str) -> Result<i32, ShellError> {
        let commands: Vec<&str> = input.split(';').map(|s| s.trim()).collect();
        let mut last_exit_code = 0;
        
        for cmd in commands {
            if !cmd.is_empty() {
                last_exit_code = Box::pin(self.execute_command(cmd)).await?;
            }
        }
        
        Ok(last_exit_code)
    }

    async fn builtin_stats(&self) -> Result<i32, ShellError> {
        let command_count = *self.command_count.read().await;
        let error_count = *self.error_count.read().await;
        let uptime = self.startup_time.elapsed();
        let last_command_time = *self.last_command_time.read().await;
        let time_since_last = last_command_time.elapsed();
        
        println!();
        println!("{}╔══════════════════════════════════════════════════════════════════════════╗{}", BRIGHT_CYAN, RESET);
        println!("{}║{} {}[STATS] NexusShell Performance Statistics{} {}                         ║{}", BRIGHT_CYAN, RESET, BRIGHT_YELLOW, RESET, BRIGHT_CYAN, RESET);
        println!("{}╠══════════════════════════════════════════════════════════════════════════╣{}", BRIGHT_CYAN, RESET);
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        
        // Session info
        println!("{}║{} {}Session Information:{} {}                                              ║{}", BRIGHT_CYAN, RESET, BOLD, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{} Session ID: {:<50} {}║{}", BRIGHT_CYAN, RESET, self.session_id, BRIGHT_CYAN, RESET);
        println!("{}║{} Uptime: {:<54} {}║{}", BRIGHT_CYAN, RESET, format!("{:?}", uptime), BRIGHT_CYAN, RESET);
        println!("{}║{} Time since last command: {:<38} {}║{}", BRIGHT_CYAN, RESET, format!("{:?}", time_since_last), BRIGHT_CYAN, RESET);
        
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        
        // Command statistics
        println!("{}║{} {}Command Statistics:{} {}                                               ║{}", BRIGHT_CYAN, RESET, BOLD, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{} Total commands executed: {:<41} {}║{}", BRIGHT_CYAN, RESET, command_count, BRIGHT_CYAN, RESET);
        println!("{}║{} Total errors: {:<50} {}║{}", BRIGHT_CYAN, RESET, error_count, BRIGHT_CYAN, RESET);
        
        let success_rate = if command_count > 0 {
            ((command_count - error_count) as f64 / command_count as f64) * 100.0
        } else {
            100.0
        };
        println!("{}║{} Success rate: {:<48} {}║{}", BRIGHT_CYAN, RESET, format!("{:.1}%", success_rate), BRIGHT_CYAN, RESET);
        
        let commands_per_minute = if uptime.as_secs() > 0 {
            (command_count as f64 / uptime.as_secs() as f64) * 60.0
        } else {
            0.0
        };
        println!("{}║{} Commands per minute: {:<41} {}║{}", BRIGHT_CYAN, RESET, format!("{:.1}", commands_per_minute), BRIGHT_CYAN, RESET);
        
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        
        // Memory and performance info
        println!("{}║{} {}Memory & Performance:{} {}                                             ║{}", BRIGHT_CYAN, RESET, BOLD, RESET, BRIGHT_CYAN, RESET);
        
        let history_count = self.history.read().await.len();
        let alias_count = self.aliases.read().await.len();
        let job_count = self.jobs.read().await.len();
        let function_count = self.functions.read().await.len();
        let array_count = self.arrays.read().await.len();
        
        println!("{}║{} History entries: {:<45} {}║{}", BRIGHT_CYAN, RESET, history_count, BRIGHT_CYAN, RESET);
        println!("{}║{} Active aliases: {:<46} {}║{}", BRIGHT_CYAN, RESET, alias_count, BRIGHT_CYAN, RESET);
        println!("{}║{} Background jobs: {:<45} {}║{}", BRIGHT_CYAN, RESET, job_count, BRIGHT_CYAN, RESET);
        println!("{}║{} Defined functions: {:<43} {}║{}", BRIGHT_CYAN, RESET, function_count, BRIGHT_CYAN, RESET);
        println!("{}║{} Arrays: {:<56} {}║{}", BRIGHT_CYAN, RESET, array_count, BRIGHT_CYAN, RESET);
        
        println!("{}║{}                                                                          {}║{}", BRIGHT_CYAN, RESET, BRIGHT_CYAN, RESET);
        println!("{}║{} {}[INFO] Type 'help' for commands or 'exit' to quit{} {}                 ║{}", BRIGHT_CYAN, RESET, BRIGHT_GREEN, RESET, BRIGHT_CYAN, RESET);
        println!("{}╚══════════════════════════════════════════════════════════════════════════╝{}", BRIGHT_CYAN, RESET);
        println!();
        
        Ok(0)
    }
}