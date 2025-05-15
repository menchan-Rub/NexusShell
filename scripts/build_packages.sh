#!/bin/bash
# NexusShell パッケージビルドスクリプト
# このスクリプトは、Windows(.exe), Debian(.deb), RPM(.rpm)パッケージを生成します

set -e

# 現在のディレクトリを保存
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

# バージョン情報を取得
VERSION=$(grep "^version" Cargo.toml | head -n 1 | cut -d'"' -f2)
echo "ビルドするNexusShellのバージョン: ${VERSION}"

# ビルドディレクトリを作成
BUILD_DIR="${ROOT_DIR}/dist"
mkdir -p "${BUILD_DIR}"

# クリーンビルド
echo "クリーンビルドを実行しています..."
cargo clean
cargo build --release

# Linuxパッケージのビルド（Linux環境の場合）
if [[ "$(uname)" == "Linux" ]]; then
    echo "Debianパッケージをビルドしています..."
    cargo deb
    cp "target/debian/nexus-shell_${VERSION}_amd64.deb" "${BUILD_DIR}/"
    
    echo "RPMパッケージをビルドしています..."
    cargo rpm build
    cp "target/release/rpmbuild/RPMS/x86_64/nexus-shell-${VERSION}-1.x86_64.rpm" "${BUILD_DIR}/"
fi

# Windowsパッケージのビルド（Windows環境の場合）
if [[ "$(uname)" == "MINGW"* ]] || [[ "$(uname)" == "MSYS"* ]] || [[ -n "$WINDIR" ]]; then
    echo "Windows実行可能ファイルをコピーしています..."
    cp "target/release/nexus-shell.exe" "${BUILD_DIR}/"
    
    # WiXツールセットが利用可能な場合はインストーラーをビルド
    if command -v cargo-wix &> /dev/null; then
        echo "Windowsインストーラー(.msi)をビルドしています..."
        cargo wix --no-build --nocapture
        cp "target/wix/nexus-shell-${VERSION}-x86_64.msi" "${BUILD_DIR}/"
    else
        echo "WiXツールセットが見つかりません。MSIパッケージは作成されません。"
        echo "インストールするには: cargo install cargo-wix"
    fi
fi

# macOSパッケージのビルド（macOS環境の場合）
if [[ "$(uname)" == "Darwin" ]]; then
    echo "macOS実行可能ファイルをコピーしています..."
    cp "target/release/nexus-shell" "${BUILD_DIR}/"
    
    # macOSアプリバンドルの作成
    APP_DIR="${BUILD_DIR}/NexusShell.app"
    mkdir -p "${APP_DIR}/Contents/MacOS"
    mkdir -p "${APP_DIR}/Contents/Resources"
    
    # Info.plistの作成
    cat > "${APP_DIR}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>nexus-shell</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.nexusshell.app</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>NexusShell</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2024 NexusShell Team. All rights reserved.</string>
</dict>
</plist>
EOF

    # 実行ファイルをコピー
    cp "target/release/nexus-shell" "${APP_DIR}/Contents/MacOS/"
    
    # アイコンがあればコピー
    if [ -f "res/nexus-shell.icns" ]; then
        cp "res/nexus-shell.icns" "${APP_DIR}/Contents/Resources/AppIcon.icns"
    fi
    
    # DMGパッケージの作成（create-dmg がインストールされている場合）
    if command -v create-dmg &> /dev/null; then
        echo "macOS DMGパッケージを作成しています..."
        create-dmg \
            --volname "NexusShell ${VERSION}" \
            --window-pos 200 120 \
            --window-size 800 400 \
            --icon-size 100 \
            --icon "NexusShell.app" 200 190 \
            --hide-extension "NexusShell.app" \
            --app-drop-link 600 185 \
            "${BUILD_DIR}/NexusShell-${VERSION}.dmg" \
            "${APP_DIR}"
    else
        echo "create-dmgが見つかりません。DMGパッケージは作成されません。"
        echo "インストールするには: brew install create-dmg"
    fi
fi

echo "ビルド完了！パッケージは以下に保存されました: ${BUILD_DIR}"
ls -l "${BUILD_DIR}" 