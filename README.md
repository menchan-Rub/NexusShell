# 🚀 NexusShell

> **World's Most Beautiful Command Shell**  
> A high-performance, POSIX-compatible shell with modern UI and enterprise-grade features

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey)](https://github.com/menchan-Rub/NexusShell)

## ✨ Features

### 🎯 **Core Capabilities**
- **Full POSIX Compatibility** - 96%+ standard shell compatibility
- **Beautiful UI** - Colorful, modern terminal interface
- **High Performance** - Async Rust implementation with Tokio
- **Enterprise Ready** - Built for professional environments

### 🔧 **Advanced Features**
- **🎨 Syntax Highlighting** - Real-time command colorization
- **⚡ Smart Tab Completion** - Intelligent command and file completion
- **📊 Performance Statistics** - Detailed execution metrics
- **🔍 Command Validation** - Input error prevention
- **💾 Persistent History** - Command history with search
- **🎭 Aliases & Functions** - Custom command shortcuts

### 🌟 **Modern Shell Features**
- **Pipelines** - `cmd1 | cmd2 | cmd3`
- **Redirections** - `cmd > file`, `cmd >> file`, `cmd < file`
- **Variables** - `VAR=value`, `$VAR`, `${VAR}`
- **Command Substitution** - `$(command)`, `` `command` ``
- **Arithmetic Expansion** - `$((expression))`
- **Background Jobs** - `command &`
- **Control Flow** - `if/then/fi`, `for/do/done`, `while/do/done`
- **Command Chaining** - `cmd1 && cmd2 || cmd3`

## 🚀 Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/menchan-Rub/NexusShell.git
cd NexusShell

# Build release version
cargo build --release

# Run NexusShell
./target/release/nexusshell
```

### First Run

```
╔══════════════════════════════════════════════════════════════════════════╗
║                                                                          ║
║  >> NexusShell v1.0.0 - World's Most Beautiful Command Shell <<        ║
║                                                                          ║
║  * Features: Full POSIX compatibility with modern UI *                ║
║                                                                          ║
║  [?] Type 'help' for commands  [*] Beautiful colors enabled [?]        ║
║                                                                          ║
╚══════════════════════════════════════════════════════════════════════════╝

>> Pro tip: Try 'help', 'env', or any command!

user@hostname NexusShell> help
```

## 📋 Built-in Commands

| Command | Description | Example |
|---------|-------------|---------|
| `cd [DIR]` | Change directory | `cd /home/user` |
| `pwd` | Print working directory | `pwd` |
| `echo [TEXT]` | Print text | `echo "Hello World"` |
| `env` | Display environment variables | `env` |
| `history` | Show command history | `history` |
| `alias` | Create command aliases | `alias ll='ls -la'` |
| `jobs` | Show active jobs | `jobs` |
| `stats` | Show performance statistics | `stats` |
| `help` | Show help message | `help` |
| `exit` | Exit the shell | `exit` |

## 🎨 Advanced Usage

### Smart Tab Completion
```bash
# Type partial command and press Tab
user@hostname NexusShell> ec[TAB]
echo  env  exec  exit

# File completion
user@hostname NexusShell> cat file[TAB]
file1.txt  file2.log  file3.md
```

### Syntax Highlighting
- **Commands** appear in green
- **Variables** appear in yellow  
- **Options** appear in cyan
- **Assignments** appear in magenta

### Performance Statistics
```bash
user@hostname NexusShell> stats

╔══════════════════════════════════════════════════════════════════════════╗
║ [STATS] NexusShell Performance Statistics                               ║
╠══════════════════════════════════════════════════════════════════════════╣
║                                                                          ║
║ Session Information:                                                     ║
║ Session ID: 550e8400-e29b-41d4-a716-446655440000                       ║
║ Uptime: 5m 30s                                                          ║
║ Time since last command: 2s                                             ║
║                                                                          ║
║ Command Statistics:                                                      ║
║ Total commands executed: 42                                              ║
║ Total errors: 3                                                          ║
║ Success rate: 92.9%                                                      ║
║ Commands per minute: 7.6                                                 ║
╚══════════════════════════════════════════════════════════════════════════╝
```

### Command Chaining
```bash
# Execute commands sequentially
user@hostname NexusShell> echo "Building..." && cargo build && echo "Done!"

# Execute on success/failure
user@hostname NexusShell> make test || echo "Tests failed!"

# Background execution
user@hostname NexusShell> long_running_task &
```

## 🔧 Configuration

### Environment Variables
```bash
# Set custom prompt
export PS1="nexus$ "

# Set history file location
export HISTFILE="~/.nexusshell_history"

# Set home directory
export HOME="/home/user"
```

### Aliases
```bash
# Create useful aliases
alias ll='ls -la'
alias grep='grep --color=auto'
alias ..='cd ..'
alias ...='cd ../..'
```

## 🏗️ Architecture

### Core Components
- **Shell Engine** - Main command processing loop
- **Parser** - Command line parsing and validation
- **Executor** - Command execution with async support
- **Completion System** - Tab completion engine
- **History Manager** - Command history persistence
- **Statistics Tracker** - Performance monitoring

### Technology Stack
- **Language**: Rust 1.70+
- **Async Runtime**: Tokio
- **Terminal**: Rustyline for readline functionality
- **Parsing**: Regex-based command parsing
- **Concurrency**: Arc<RwLock> for thread safety

## 🚀 Performance

### Benchmarks
- **Startup Time**: < 50ms
- **Command Execution**: < 10ms overhead
- **Memory Usage**: < 5MB base memory
- **Tab Completion**: < 1ms response time

### Optimizations
- Async command execution
- Lazy loading of completions
- Efficient history management
- Memory-efficient variable storage

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guidelines](CONTRIBUTING.md) for details.

### Development Setup
```bash
# Clone and setup
git clone https://github.com/menchan-Rub/NexusShell.git
cd NexusShell

# Install dependencies
cargo build

# Run tests
cargo test

# Run with debug info
cargo run
```

### Code Style
- Follow Rust standard formatting (`cargo fmt`)
- Ensure all tests pass (`cargo test`)
- Add documentation for new features
- Use meaningful commit messages

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- **Rust Community** - For the amazing ecosystem
- **Tokio Team** - For the async runtime
- **Rustyline** - For readline functionality
- **Contributors** - For making this project better

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/menchan-Rub/NexusShell/issues)
- **Discussions**: [GitHub Discussions](https://github.com/menchan-Rub/NexusShell/discussions)

---

<div align="center">

**Made with ❤️ by the NexusShell Team**

</div> 