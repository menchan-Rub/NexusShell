# 📖 NexusShell 完全マニュアル v2.2.0

## 目次

1. [概要](#概要)
2. [インストール・セットアップ](#インストール・セットアップ)
3. [基本操作](#基本操作)
4. [コマンド詳細リファレンス](#コマンド詳細リファレンス)
5. [高度機能](#高度機能)
6. [設定・カスタマイズ](#設定・カスタマイズ)
7. [トラブルシューティング](#トラブルシューティング)
8. [パフォーマンス最適化](#パフォーマンス最適化)
9. [開発者向け情報](#開発者向け情報)

---

## 概要

**NexusShell v1.0.0**は、Rustで構築された次世代エンタープライズシェルです。従来のシェルの制約を打ち破り、現代の開発・運用環境に最適化された革新的な機能を提供します。

### 🎯 設計思想

- **安全性第一**: Rustのメモリ安全性による堅牢な実行環境
- **パフォーマンス重視**: ナノ秒単位の精密な測定と最適化
- **ユーザビリティ**: 直感的で美しいインターフェース
- **拡張性**: モジュラー設計による柔軟な機能拡張

---

## インストール・セットアップ

### システム要件

| 項目 | 最小要件 | 推奨要件 |
|------|----------|----------|
| OS | Windows 10, Linux 4.0+, macOS 10.15+ | Windows 11, Linux 5.0+, macOS 12+ |
| CPU | x86_64 | x86_64 (マルチコア) |
| メモリ | 512MB | 2GB以上 |
| ストレージ | 50MB | 200MB以上 |

### インストール手順

#### 1. ソースからビルド

```bash
# 1. Rustツールチェーンのインストール
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 2. リポジトリクローン
git clone https://github.com/menchan-Rub/NexusShell.git
cd NexusShell

# 3. 依存関係確認
cargo check

# 4. リリースビルド
cargo build --release

# 5. インストール（オプション）
cargo install --path .
```

#### 2. 実行確認

```bash
# 直接実行
./target/release/nexusshell

# または（インストール済みの場合）
nexusshell
```

### 初期設定

#### 環境変数設定

```bash
# .bashrc または .zshrc に追加
export NEXUS_SHELL_PATH="/path/to/nexusshell"
export NEXUS_SHELL_CONFIG="$HOME/.nexusshell"
```

#### 設定ディレクトリ作成

```bash
mkdir -p ~/.nexusshell/{config,cache,logs}
```

---

## 基本操作

### 起動・終了

```bash
# 起動
nexusshell

# 終了方法
exit        # または quit
Ctrl+D      # EOF
Ctrl+C      # 割り込み（コマンド実行中）
```

### プロンプト理解

```
Aqua@aqua-machine:NexusShell$ 
│    │            │           │
│    │            │           └─ コマンド入力待ち
│    │            └─ 現在のディレクトリ
│    └─ ホスト名
└─ ユーザー名
```

### 基本ナビゲーション

```bash
# 現在ディレクトリ確認
pwd

# ディレクトリ変更
cd /path/to/directory
cd ~                    # ホームディレクトリ
cd -                    # 前のディレクトリ
cd ..                   # 親ディレクトリ

# ディレクトリ内容表示
ls                      # 基本表示
ls -la                  # 詳細表示（隠しファイル含む）
ls -lh                  # 人間が読みやすいサイズ表示
```

---

## コマンド詳細リファレンス

### 🏠 コアコマンド

#### `help` - ヘルプシステム

```bash
help                    # 全コマンド一覧
help <command>          # 特定コマンドのヘルプ
help --categories       # カテゴリ別表示
```

**出力例:**
```
===== CORE COMMANDS =====
help         - Show this comprehensive help system
version      - Display detailed version and build information
stats        - Show advanced usage statistics and metrics
...
```

#### `version` - バージョン情報

```bash
version                 # 基本バージョン情報
version --full          # 詳細情報
version --build         # ビルド情報
```

**出力例:**
```
NexusShell v2.2.0 - World's Most Advanced Shell
Build: release-optimized
Rust Version: 1.70.0
Build Date: 2024-01-15
Commit: abc123def456
Features: All Enabled (10/10)
```

#### `stats` - 統計情報

```bash
stats                   # 全統計表示
stats --session         # セッション統計のみ
stats --performance     # パフォーマンス統計のみ
stats --reset           # 統計リセット
```

**出力例:**
```
===== NEXUSSHELL ADVANCED STATISTICS =====
EXECUTION METRICS:
Total Commands: 25
Successful: 23
Failed: 2
Success Rate: 92.0%
```

### 📁 ファイルシステムコマンド

#### `ls` - ディレクトリ内容表示

```bash
ls [OPTIONS] [PATH...]

OPTIONS:
-l, --long              # 詳細表示
-a, --all               # 隠しファイル表示
-h, --human-readable    # 人間が読みやすいサイズ
-t, --time              # 更新時間順ソート
-r, --reverse           # 逆順ソート
-S, --size              # サイズ順ソート
-R, --recursive         # 再帰的表示
```

**使用例:**
```bash
ls -la                  # 詳細表示（隠しファイル含む）
ls -lht                 # 詳細、人間が読みやすい、時間順
ls -lSr                 # 詳細、サイズ順、逆順
```

#### `cd` - ディレクトリ変更

```bash
cd [PATH]

特殊パス:
~                       # ホームディレクトリ
-                       # 前のディレクトリ
..                      # 親ディレクトリ
../..                   # 2階層上
```

#### `mkdir` - ディレクトリ作成

```bash
mkdir [OPTIONS] DIRECTORY...

OPTIONS:
-p, --parents           # 親ディレクトリも作成
-v, --verbose           # 詳細出力
-m, --mode MODE         # 権限設定
```

**使用例:**
```bash
mkdir newdir                    # 単一ディレクトリ作成
mkdir -p path/to/deep/dir      # 階層ディレクトリ作成
mkdir -v dir1 dir2 dir3        # 複数ディレクトリ作成
```

### 📄 ファイル操作コマンド

#### `cat` - ファイル内容表示

```bash
cat [OPTIONS] FILE...

OPTIONS:
-n, --number            # 行番号表示
-b, --number-nonblank   # 空白行以外に行番号
-s, --squeeze-blank     # 連続する空白行を1行に
-T, --show-tabs         # タブを^Iで表示
```

**使用例:**
```bash
cat file.txt                   # ファイル内容表示
cat -n file.txt                # 行番号付きで表示
cat file1.txt file2.txt        # 複数ファイル連結表示
```

#### `grep` - パターン検索

```bash
grep [OPTIONS] PATTERN FILE...

OPTIONS:
-i, --ignore-case       # 大文字小文字を区別しない
-r, --recursive         # 再帰的検索
-n, --line-number       # 行番号表示
-v, --invert-match      # マッチしない行を表示
-c, --count             # マッチした行数のみ表示
-l, --files-with-matches # マッチしたファイル名のみ表示
```

**使用例:**
```bash
grep "pattern" file.txt         # 基本検索
grep -i "pattern" file.txt      # 大文字小文字無視
grep -rn "pattern" directory/   # 再帰的検索（行番号付き）
grep -v "pattern" file.txt      # パターンにマッチしない行
```

#### `find` - ファイル検索

```bash
find [PATH] [OPTIONS]

OPTIONS:
-name PATTERN           # ファイル名パターン
-type TYPE              # ファイルタイプ（f=ファイル, d=ディレクトリ）
-size SIZE              # ファイルサイズ
-mtime DAYS             # 更新日時
-exec COMMAND {} \;     # 見つかったファイルでコマンド実行
```

**使用例:**
```bash
find . -name "*.txt"            # .txtファイル検索
find . -type d                  # ディレクトリのみ検索
find . -size +1M                # 1MB以上のファイル
find . -mtime -7                # 7日以内に更新されたファイル
```

### ✏️ テキスト処理コマンド

#### `sort` - 行ソート

```bash
sort [OPTIONS] FILE...

OPTIONS:
-r, --reverse           # 逆順ソート
-n, --numeric-sort      # 数値ソート
-u, --unique            # 重複行削除
-k, --key FIELD         # 指定フィールドでソート
-t, --field-separator   # フィールド区切り文字指定
```

**使用例:**
```bash
sort file.txt                   # アルファベット順ソート
sort -n numbers.txt             # 数値順ソート
sort -r file.txt                # 逆順ソート
sort -u file.txt                # 重複削除ソート
```

#### `sed` - ストリームエディタ

```bash
sed [OPTIONS] 'COMMAND' FILE...

主要コマンド:
s/PATTERN/REPLACEMENT/FLAGS     # 置換
d                               # 行削除
p                               # 行印刷
```

**使用例:**
```bash
sed 's/old/new/g' file.txt      # 全置換
sed 's/old/new/' file.txt       # 最初のマッチのみ置換
sed '/pattern/d' file.txt       # パターンマッチ行削除
sed -n '1,5p' file.txt          # 1-5行目のみ表示
```

#### `awk` - パターン処理

```bash
awk 'PROGRAM' FILE...

基本構文:
{ action }                      # 全行に対してaction実行
pattern { action }              # patternマッチ行にaction実行
BEGIN { action }                # 処理開始前にaction実行
END { action }                  # 処理終了後にaction実行
```

**使用例:**
```bash
awk '{print $1}' file.txt       # 第1フィールド表示
awk '{print NF}' file.txt       # フィールド数表示
awk '{print NR}' file.txt       # 行番号表示
awk '/pattern/ {print}' file.txt # パターンマッチ行表示
```

---

## 高度機能

### 🎛️ 機能管理システム

NexusShellは10の高度機能カテゴリを提供：

#### 機能一覧

```bash
features                        # 全機能状態表示
features --enabled              # 有効機能のみ表示
features --disabled             # 無効機能のみ表示
```

#### 機能制御

```bash
enable <feature_name>           # 機能有効化
disable <feature_name>          # 機能無効化
enable --all                    # 全機能有効化
disable --all                   # 全機能無効化
```

**利用可能機能:**

1. **file_operations** - 高度ファイルシステム操作
2. **text_processing** - テキスト処理・操作
3. **system_monitoring** - システム監視・分析
4. **network_tools** - ネットワークユーティリティ
5. **compression** - アーカイブ・圧縮ツール
6. **development_tools** - 開発環境統合
7. **advanced_search** - 高度検索・フィルタリング
8. **job_control** - プロセス・ジョブ管理
9. **performance_monitoring** - パフォーマンス分析
10. **security_tools** - セキュリティ・暗号化

### 📊 パフォーマンス監視

#### リアルタイム統計

```bash
performance                     # 詳細パフォーマンス表示
performance --live              # リアルタイム更新
performance --history           # 履歴表示
performance --export            # JSON形式でエクスポート
```

#### メトリクス項目

- **実行メトリクス**: コマンド数、成功率、エラー率
- **パフォーマンス**: 実行時間、I/O操作、スループット
- **リソース**: メモリ使用量、CPU使用率、キャッシュ効率
- **セッション**: セッション時間、履歴サイズ、アクティブジョブ

### 🛡️ セキュリティ機能

#### サンドボックス実行

```bash
system --security               # セキュリティ状態表示
```

**セキュリティ機能:**
- サンドボックス実行環境
- 権限制御システム
- 監査証跡記録
- エンタープライズレベルセキュリティ

---

## 設定・カスタマイズ

### 設定ファイル

#### メイン設定ファイル: `~/.nexusshell/config.toml`

```toml
[shell]
prompt_format = "{user}@{host}:{dir}$ "
history_size = 10000
auto_completion = true
syntax_highlighting = true

[performance]
enable_monitoring = true
detailed_stats = true
cache_size = "64MB"

[security]
sandbox_mode = true
audit_trail = true
permission_check = true

[features]
file_operations = true
text_processing = true
system_monitoring = true
network_tools = true
compression = true
development_tools = true
advanced_search = true
job_control = true
performance_monitoring = true
security_tools = true
```

### エイリアス設定

#### エイリアスファイル: `~/.nexusshell/aliases.toml`

```toml
[aliases]
ll = "ls -la"
la = "ls -A"
l = "ls -CF"
".." = "cd .."
"..." = "cd ../.."
"...." = "cd ../../.."
h = "history"
c = "clear"
q = "exit"

# カスタムエイリアス
gst = "git status"
gco = "git checkout"
gpl = "git pull"
gps = "git push"
```

### 環境変数

#### 環境変数ファイル: `~/.nexusshell/env.toml`

```toml
[environment]
EDITOR = "nano"
PAGER = "less"
BROWSER = "firefox"
NEXUS_THEME = "default"
NEXUS_LOG_LEVEL = "info"
```

---

## トラブルシューティング

### よくある問題と解決方法

#### 1. 起動時エラー

**問題**: NexusShellが起動しない

**解決方法**:
```bash
# 1. 実行権限確認
chmod +x ./target/release/nexusshell

# 2. 依存関係確認
ldd ./target/release/nexusshell

# 3. 設定ファイル確認
ls -la ~/.nexusshell/

# 4. ログ確認
cat ~/.nexusshell/logs/nexusshell.log
```

#### 2. コマンド実行エラー

**問題**: 特定のコマンドが動作しない

**解決方法**:
```bash
# 1. 機能状態確認
features

# 2. 必要機能の有効化
enable <feature_name>

# 3. システム情報確認
system

# 4. パフォーマンス確認
stats
```

#### 3. パフォーマンス問題

**問題**: 動作が遅い

**解決方法**:
```bash
# 1. パフォーマンス分析
performance

# 2. 不要機能の無効化
disable <unused_feature>

# 3. キャッシュクリア
rm -rf ~/.nexusshell/cache/*

# 4. 設定最適化
# config.tomlでcache_sizeを調整
```

### ログとデバッグ

#### ログファイル場所

```
~/.nexusshell/logs/
├── nexusshell.log      # メインログ
├── performance.log     # パフォーマンスログ
├── security.log        # セキュリティログ
└── debug.log          # デバッグログ
```

#### デバッグモード

```bash
# デバッグモードで起動
NEXUS_LOG_LEVEL=debug nexusshell

# 詳細ログ出力
NEXUS_DEBUG=1 nexusshell
```

---

## パフォーマンス最適化

### システム最適化

#### 1. ビルド最適化

```bash
# 最高レベル最適化
cargo build --release

# ターゲット最適化
cargo build --release --target-cpu=native

# LTO有効化（Cargo.tomlで設定済み）
[profile.release]
lto = true
codegen-units = 1
```

#### 2. 実行時最適化

```bash
# 環境変数設定
export NEXUS_CACHE_SIZE=128MB
export NEXUS_WORKER_THREADS=4
export NEXUS_ENABLE_SIMD=1
```

#### 3. メモリ最適化

```toml
# config.toml
[performance]
cache_size = "128MB"        # キャッシュサイズ
gc_threshold = "256MB"      # GC閾値
buffer_size = "64KB"        # バッファサイズ
```

### ベンチマーク

#### パフォーマンステスト

```bash
# 基本ベンチマーク
time nexusshell -c "ls -la > /dev/null"

# 複合ベンチマーク
time nexusshell -c "find . -name '*.txt' | grep pattern | wc -l"

# メモリ使用量測定
/usr/bin/time -v nexusshell -c "command"
```

---

## 開発者向け情報

### アーキテクチャ

#### モジュール構成

```
src/
├── main.rs                 # メインエントリーポイント
├── shell/                  # コアシェル機能
├── commands/               # コマンド実装
├── parser/                 # コマンドパーサー
├── executor/               # 実行エンジン
├── performance/            # パフォーマンス監視
├── security/               # セキュリティ機能
└── utils/                  # ユーティリティ
```

#### 主要データ構造

```rust
pub struct NexusShell {
    config: ShellConfig,
    features: HashMap<String, bool>,
    command_count: u64,
    performance_data: PerformanceData,
    current_dir: PathBuf,
    environment_vars: HashMap<String, String>,
    command_history: Vec<CommandHistoryEntry>,
    aliases: HashMap<String, String>,
    jobs: Vec<Job>,
    last_command_status: i32,
}
```

### 拡張開発

#### カスタムコマンド追加

```rust
impl NexusShell {
    async fn custom_command(&self, args: &[String]) -> Result<String, Box<dyn std::error::Error>> {
        // カスタムロジック実装
        Ok("Custom command result".to_string())
    }
}
```

#### プラグインインターフェース

```rust
pub trait Plugin {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn execute(&self, args: &[String]) -> Result<String, Box<dyn std::error::Error>>;
}
```

### テスト

#### ユニットテスト

```bash
# 全テスト実行
cargo test

# 特定モジュールテスト
cargo test commands

# 統合テスト
cargo test --test integration
```

#### パフォーマンステスト

```bash
# ベンチマーク実行
cargo bench

# プロファイリング
cargo run --release --bin profiler
```

---

## 付録

### キーボードショートカット

| ショートカット | 機能 |
|---------------|------|
| Tab | コマンド補完 |
| Ctrl+C | コマンド中断 |
| Ctrl+D | EOF/終了 |
| Ctrl+L | 画面クリア |
| Ctrl+R | 履歴検索 |
| ↑/↓ | 履歴ナビゲーション |
| Ctrl+A | 行頭移動 |
| Ctrl+E | 行末移動 |

### 設定例

#### 開発者向け設定

```toml
[shell]
prompt_format = "{user}@{host}:{git_branch}:{dir}$ "
history_size = 50000
auto_completion = true
syntax_highlighting = true

[aliases]
gst = "git status"
gco = "git checkout"
gpl = "git pull"
gps = "git push"
glog = "git log --oneline --graph"
build = "cargo build --release"
test = "cargo test"
```

#### システム管理者向け設定

```toml
[shell]
prompt_format = "root@{host}:{dir}# "
history_size = 100000
audit_commands = true

[security]
sandbox_mode = true
audit_trail = true
permission_check = true
log_all_commands = true

[features]
system_monitoring = true
security_tools = true
network_tools = true
```

---

<div align="center">

**📖 NexusShell完全マニュアル v2.2.0**

このマニュアルは継続的に更新されます。
最新版は [GitHub Wiki](https://github.com/menchan-Rub/NexusShell/wiki) で確認してください。

</div> 