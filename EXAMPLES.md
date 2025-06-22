# 🚀 NexusShell 実用例集

## 目次

1. [基本操作例](#基本操作例)
2. [ファイル操作例](#ファイル操作例)
3. [テキスト処理例](#テキスト処理例)
4. [システム管理例](#システム管理例)
5. [開発作業例](#開発作業例)
6. [高度な使用例](#高度な使用例)

---

## 基本操作例

### ディレクトリナビゲーション

```bash
# 現在位置確認
Aqua@aqua-machine:~$ pwd
/home/Aqua

# プロジェクトディレクトリに移動
Aqua@aqua-machine:~$ cd /home/Aqua/projects
Aqua@aqua-machine:projects$ 

# 詳細なディレクトリ内容表示
Aqua@aqua-machine:projects$ ls -la
drwxr-xr-x 1 Aqua Aqua    512 Jan 15 10:30 .
drwxr-xr-x 1 Aqua Aqua    512 Jan 15 09:15 ..
drwxr-xr-x 1 Aqua Aqua    512 Jan 15 10:25 NexusShell/
drwxr-xr-x 1 Aqua Aqua    256 Jan 14 15:20 website/
-rw-r--r-- 1 Aqua Aqua   1024 Jan 15 10:30 README.md
```

### ヘルプとシステム情報

```bash
# 包括的ヘルプ表示
Aqua@aqua-machine:projects$ help
NexusShell v2.2.0 - World's Most Advanced Shell
==========================================
Session: 46b11858 | Commands: 1 | Uptime: 175.09s
Success Rate: 66.7% | Features: 10 Active

===== CORE COMMANDS =====
help         - Show this comprehensive help system
version      - Display detailed version and build information
...

# システム情報確認
Aqua@aqua-machine:projects$ system
===== SYSTEM INFORMATION =====
OPERATING SYSTEM:
OS: windows
Architecture: x86_64
CPU Cores: 16
Memory: Available
```

---

## ファイル操作例

### ファイル作成と編集

```bash
# 新しいファイル作成
Aqua@aqua-machine:projects$ touch new_file.txt

# ディレクトリ作成
Aqua@aqua-machine:projects$ mkdir -p docs/api/v1
Aqua@aqua-machine:projects$ tree docs
docs
├── api
    └── v1

# ファイル内容表示
Aqua@aqua-machine:projects$ cat README.md
# My Projects
This directory contains all my development projects.
```

### ファイル検索と管理

```bash
# 特定の拡張子ファイルを検索
Aqua@aqua-machine:projects$ find . -name "*.rs"
./NexusShell/src/main.rs
./NexusShell/src/shell.rs
./NexusShell/src/executor.rs

# ファイルサイズでソート表示
Aqua@aqua-machine:projects$ ls -lhS
-rw-r--r-- 1 Aqua Aqua  61K Jan 15 10:30 src/main.rs
-rw-r--r-- 1 Aqua Aqua  6.9K Jan 15 10:25 src/shell.rs
-rw-r--r-- 1 Aqua Aqua  2.9K Jan 15 10:20 src/executor.rs

# ディスク使用量確認
Aqua@aqua-machine:projects$ du -h NexusShell/
125.4 MB    NexusShell/target
2.1 MB      NexusShell/src
127.5 MB    NexusShell/
```

---

## テキスト処理例

### ログファイル分析

```bash
# アクセスログの分析
Aqua@aqua-machine:logs$ cat access.log
192.168.1.1 - - [15/Jan/2024:10:30:45] "GET /api/users" 200
192.168.1.2 - - [15/Jan/2024:10:31:12] "POST /api/login" 401
192.168.1.1 - - [15/Jan/2024:10:31:45] "GET /api/dashboard" 200

# エラーレスポンスのみ抽出
Aqua@aqua-machine:logs$ grep " 4[0-9][0-9]\| 5[0-9][0-9]" access.log
192.168.1.2 - - [15/Jan/2024:10:31:12] "POST /api/login" 401

# IPアドレス別アクセス数集計
Aqua@aqua-machine:logs$ awk '{print $1}' access.log | sort | uniq -c
      2 192.168.1.1
      1 192.168.1.2
```

### データ処理

```bash
# CSVファイル処理
Aqua@aqua-machine:data$ cat users.csv
name,age,city
Alice,25,Tokyo
Bob,30,Osaka
Charlie,35,Kyoto

# 特定の列を抽出
Aqua@aqua-machine:data$ cut -d',' -f1,3 users.csv
name,city
Alice,Tokyo
Bob,Osaka
Charlie,Kyoto

# 年齢でソート
Aqua@aqua-machine:data$ sort -t',' -k2 -n users.csv
name,age,city
Alice,25,Tokyo
Bob,30,Osaka
Charlie,35,Kyoto
```

---

## システム管理例

### パフォーマンス監視

```bash
# 詳細統計情報表示
Aqua@aqua-machine:~$ stats
===== NEXUSSHELL ADVANCED STATISTICS =====

EXECUTION METRICS:
Total Commands: 25
Successful: 23
Failed: 2
Success Rate: 92.0%

PERFORMANCE METRICS:
Total Execution Time: 125.456ms
Average Command Time: 5.018ms
Commands Per Second: 0.12

RESOURCE UTILIZATION:
Memory Usage: 1.04 MB
CPU Usage: 0.1%
Cache Hit Rate: 95.2%
```

### 機能管理

```bash
# 利用可能機能確認
Aqua@aqua-machine:~$ features
===== NEXUSSHELL ADVANCED FEATURES =====

Advanced File System Operations [✓ ENABLED]
Text Processing & Manipulation [✓ ENABLED]
System Monitoring & Analysis [✓ ENABLED]
Network Utilities & Diagnostics [✓ ENABLED]
...

# 特定機能の無効化（軽量化）
Aqua@aqua-machine:~$ disable network_tools
Feature 'network_tools' has been disabled.

# 機能再有効化
Aqua@aqua-machine:~$ enable network_tools
Feature 'network_tools' has been enabled.
```

---

## 開発作業例

### Rustプロジェクト管理

```bash
# プロジェクトディレクトリに移動
Aqua@aqua-machine:~$ cd NexusShell
Aqua@aqua-machine:NexusShell$ 

# プロジェクト構造確認
Aqua@aqua-machine:NexusShell$ tree -L 2
NexusShell
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs
│   ├── shell.rs
│   └── ...
├── target/
└── README.md

# ビルドとテスト
Aqua@aqua-machine:NexusShell$ cargo build --release
   Compiling nexusshell v2.2.0
    Finished release [optimized] target(s) in 3m 19s

# 実行ファイルサイズ確認
Aqua@aqua-machine:NexusShell$ ls -lh target/release/nexusshell*
-rwxr-xr-x 1 Aqua Aqua 12.5M Jan 15 10:45 target/release/nexusshell.exe
```

### Git操作

```bash
# Git状態確認
Aqua@aqua-machine:NexusShell$ git status
On branch master
Your branch is up to date with 'origin/master'.

# 変更ファイル確認
Aqua@aqua-machine:NexusShell$ git diff --name-only
src/main.rs
Cargo.toml

# コミット履歴確認
Aqua@aqua-machine:NexusShell$ git log --oneline -5
abc123d Fix compilation errors
def456e Add performance monitoring
ghi789e Implement advanced features
```

---

## 高度な使用例

### パイプライン処理

```bash
# 複雑なデータパイプライン
Aqua@aqua-machine:data$ cat large_log.txt | \
  grep "ERROR" | \
  awk '{print $1, $4}' | \
  sort | \
  uniq -c | \
  sort -nr | \
  head -10

# ファイル検索とパターンマッチ
Aqua@aqua-machine:project$ find . -name "*.rs" | \
  xargs grep -l "async fn" | \
  wc -l
15

# 統計処理
Aqua@aqua-machine:data$ cat numbers.txt | \
  sort -n | \
  awk '{sum+=$1; count++} END {print "Average:", sum/count}'
Average: 42.5
```

### バッチ処理

```bash
# 複数ファイルの一括処理
Aqua@aqua-machine:docs$ for file in *.md; do
  echo "Processing $file..."
  wc -l "$file"
done
Processing README.md...
45 README.md
Processing MANUAL.md...
234 MANUAL.md

# バックアップ作成
Aqua@aqua-machine:project$ find . -name "*.rs" -exec cp {} backup/ \;

# 権限一括変更
Aqua@aqua-machine:scripts$ find . -name "*.sh" -exec chmod +x {} \;
```

### パフォーマンス分析

```bash
# コマンド実行時間測定
Aqua@aqua-machine:~$ time find /large/directory -name "*.txt" | wc -l
1234
real    0m2.345s
user    0m1.234s
sys     0m0.567s

# メモリ使用量監視
Aqua@aqua-machine:~$ performance
===== PERFORMANCE METRICS =====
Memory Usage: Optimized
CPU Utilization: 0.1%
I/O Performance: Excellent
Cache Hit Rate: 95.2%
```

### セキュリティ監査

```bash
# セキュリティ状態確認
Aqua@aqua-machine:~$ system | grep -A 10 "SECURITY STATUS"
SECURITY STATUS:
Execution Mode: Sandboxed
Permissions: Controlled
Security Level: Enterprise
Audit Trail: Enabled
Sandbox Status: Active

# ファイル権限監査
Aqua@aqua-machine:sensitive$ find . -type f -perm -o+w
# (結果なし = 良好)

# 最近の変更ファイル確認
Aqua@aqua-machine:project$ find . -mtime -1 -ls
```

---

## 実用的なワークフロー例

### 日常的な開発作業

```bash
# 1. プロジェクト開始
Aqua@aqua-machine:~$ cd projects/new-feature
Aqua@aqua-machine:new-feature$ git checkout -b feature/awesome-feature

# 2. 作業状況確認
Aqua@aqua-machine:new-feature$ stats
# パフォーマンス状況確認

# 3. コード編集後のテスト
Aqua@aqua-machine:new-feature$ cargo test
Aqua@aqua-machine:new-feature$ cargo build --release

# 4. 変更確認とコミット
Aqua@aqua-machine:new-feature$ git diff
Aqua@aqua-machine:new-feature$ git add .
Aqua@aqua-machine:new-feature$ git commit -m "Implement awesome feature"

# 5. パフォーマンス確認
Aqua@aqua-machine:new-feature$ performance
```

### システム保守作業

```bash
# 1. システム状態確認
Aqua@aqua-machine:~$ system
Aqua@aqua-machine:~$ df -h

# 2. ログ分析
Aqua@aqua-machine:logs$ tail -f application.log | grep ERROR

# 3. 不要ファイル削除
Aqua@aqua-machine:tmp$ find . -name "*.tmp" -mtime +7 -delete

# 4. バックアップ確認
Aqua@aqua-machine:backup$ du -sh daily-backup-*
2.1G    daily-backup-2024-01-14
2.3G    daily-backup-2024-01-15
```

---

<div align="center">

**🚀 NexusShell実用例集**

これらの例を参考に、NexusShellの強力な機能を最大限活用してください！

Made with ❤️ by [menchan-Rub](https://github.com/menchan-Rub)

</div> 