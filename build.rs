// Windows向けアプリケーションメタデータ設定
// 実行ファイルにアイコンとバージョン情報を埋め込む

#[cfg(windows)]
fn main() {
    use std::io::Write;
    
    // res/ディレクトリがなければ作成
    let res_dir = std::path::Path::new("res");
    if !res_dir.exists() {
        std::fs::create_dir_all(res_dir).unwrap();
    }
    
    // Windows用のリソースファイルを生成
    let mut res = winres::WindowsResource::new();
    res.set_icon("res/nexus-shell.ico")
       .set_language(0x0411) // 日本語
       .set("FileDescription", "NexusShell - 次世代ターミナル")
       .set("ProductName", "NexusShell")
       .set("CompanyName", "NexusShell Team")
       .set("LegalCopyright", "Copyright © 2024 NexusShell Team")
       .set("OriginalFilename", "nexus-shell.exe")
       .set("InternalName", "nexus-shell")
       .set("FileVersion", env!("CARGO_PKG_VERSION"))
       .set("ProductVersion", env!("CARGO_PKG_VERSION"));

    // Windows向けにコンパイル
    if let Err(e) = res.compile() {
        // アイコンファイルが見つからない場合は、世界最高品質のデフォルトアイコンを自動生成
        if !std::path::Path::new("res/nexus-shell.ico").exists() {
            println!("cargo:warning=アイコンファイルが見つかりません。世界最高品質のデフォルトアイコンを自動生成します。");
            // SVGベースで高解像度ICOを生成（rsvg-convert等の外部ツールを利用）
            let svg_icon = r#"<svg xmlns='http://www.w3.org/2000/svg' width='256' height='256'><rect width='256' height='256' fill='#222'/><text x='50%' y='55%' font-size='120' text-anchor='middle' fill='#fff' font-family='Segoe UI,Arial,sans-serif' dy='.3em'>N</text></svg>"#;
            let svg_path = "res/nexus-shell.svg";
            std::fs::write(svg_path, svg_icon).unwrap();
            // rsvg-convertでICO生成（要: rsvg-convertインストール）
            let ico_path = "res/nexus-shell.ico";
            let status = std::process::Command::new("rsvg-convert")
                .args(["-f", "ico", "-o", ico_path, svg_path])
                .status();
            if let Ok(status) = status {
                if !status.success() {
                    panic!("SVG→ICO変換に失敗しました。rsvg-convertが必要です。");
                }
            } else {
                panic!("rsvg-convertコマンドの実行に失敗しました。SVG→ICO変換にはrsvg-convertが必要です。");
            }
            // 生成したICOのバリデーション
            let ico_data = std::fs::read(ico_path).unwrap();
            if ico_data.len() < 1000 {
                panic!("生成されたICOファイルが不正です。サイズ:{}バイト", ico_data.len());
            }
            // SVGは不要なので削除
            let _ = std::fs::remove_file(svg_path);
            // リソースを再コンパイル
            res.compile().expect("リソースのコンパイルに失敗しました");
        } else {
            panic!("リソースのコンパイルに失敗しました: {}", e);
        }
    }
}

#[cfg(not(windows))]
fn main() {
    // Windowsでない場合は何もしない
} 