# NexusShell - 次世代インテリジェントシェル

NexusShellは、強力な構文解析エンジン、型システム、エラー回復機能を備えた次世代ターミナルシェルです。Windows Terminal風のモダンなインターフェースで、クロスプラットフォーム対応の高機能なコマンドライン体験を提供します。

![NexusShell Screenshot](docs/images/screenshot.png)

## 特徴

- **モダンなUI**: Windows Terminal風の洗練されたインターフェース
- **強力な型システム**: シェルスクリプトに型安全性を提供
- **高度なエラー回復**: 構文エラーを自動的に検出・修復
- **インテリジェント補完**: コンテキスト対応の強力な補完機能
- **クロスプラットフォーム**: Windows, Linux, macOSをサポート
- **高性能**: Rustで実装された高速なパフォーマンス

## インストール方法

### Windows

```bash
# インストーラーをダウンロードして実行
curl -LO https://github.com/nexusshell/nexusshell/releases/latest/download/nexus-shell-setup.exe
./nexus-shell-setup.exe

# または、Scoop経由でインストール
scoop install nexus-shell
```

### Linux

```bash
# Debian/Ubuntu系
curl -LO https://github.com/nexusshell/nexusshell/releases/latest/download/nexus-shell.deb
sudo dpkg -i nexus-shell.deb

# Red Hat/Fedora系
curl -LO https://github.com/nexusshell/nexusshell/releases/latest/download/nexus-shell.rpm
sudo rpm -i nexus-shell.rpm
```

### macOS

```bash
# Homebrew経由でインストール
brew install nexus-shell
```

### ソースからビルド

```bash
# リポジトリをクローン
git clone https://github.com/nexusshell/nexusshell.git
cd nexusshell

# ビルド
cargo build --release

# インストール
cargo install --path .
```

## 使用方法

```bash
# NexusShellを起動
nexus-shell

# スクリプトを実行
nexus-shell script.nx

# 特定のコマンドを実行
nexus-shell -c "echo 'Hello, World!'"
```

## カスタマイズ

NexusShellは高度にカスタマイズ可能です。`~/.config/nexus-shell/config.toml`ファイルを編集することで、テーマ、フォント、動作などをカスタマイズできます。

```toml
[theme]
name = "dark"  # dark, light, or custom
accent_color = "#0099ff"
background_opacity = 0.95

[terminal]
font = "Cascadia Code"
font_size = 12
cursor_style = "block"  # block, underscore, or bar

[behavior]
enable_auto_suggestions = true
enable_error_correction = true
```

## 開発

### 依存関係

- Rust 1.70以上
- Cargo

### ビルド方法

```bash
# デバッグビルド
cargo build

# リリースビルド
cargo build --release

# パッケージビルド
cargo deb  # .debパッケージを作成
cargo rpm  # .rpmパッケージを作成
cargo wix  # Windowsインストーラーを作成
```

## ライセンス

MIT または Apache-2.0 ライセンスのデュアルライセンス

## コントリビューション

貢献は歓迎します！バグレポート、機能リクエスト、プルリクエストは[GitHub Issues](https://github.com/nexusshell/nexusshell/issues)までお願いします。

```bash
# インストール（まだ準備中）
cargo install nexusshell

# 起動
nexus

# スクリプト実行
nexus script.nx

# 対話モード（高度な補完と予測機能付き）
nexus --interactive
```
