use crate::{BuiltinCommand, CommandContext, CommandResult};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tracing::{debug, warn, error, info};
use nix::unistd::{chown, Uid, Gid};
use users::{get_user_by_name, get_group_by_name};

/// ファイル検索コマンド
///
/// 柔軟な条件でファイルシステム内のファイルやディレクトリを検索します。
/// 名前、サイズ、修正日時、ファイルタイプなど、様々な条件で検索できます。
///
/// # 使用例
///
/// ```bash
/// find . -name "*.rs"                # Rustソースファイルを検索
/// find /usr -type d -name "bin"      # 名前が「bin」のディレクトリを検索
/// find . -mtime -7                    # 7日以内に変更されたファイルを検索
/// find . -size +1M                    # 1MB以上のファイルを検索
/// ```
pub struct FindCommand;

/// 検索条件の種類
enum FindCriterion {
    /// ファイル名に基づく検索
    Name(Regex),
    /// ファイルパスに基づく検索
    Path(Regex),
    /// ファイルタイプに基づく検索
    Type(FileType),
    /// サイズに基づく検索
    Size(SizeComparison),
    /// 修正時刻に基づく検索
    ModificationTime(TimeComparison),
    /// アクセス時刻に基づく検索
    AccessTime(TimeComparison),
    /// ユーザーID/名に基づく検索
    User(String),
    /// グループID/名に基づく検索
    Group(String),
    /// パーミッションに基づく検索
    Permission(u32),
    /// 空のファイル/ディレクトリ
    Empty,
    /// NOT条件（指定された条件の否定）
    Not(Box<FindCriterion>),
    /// AND条件（すべての条件を満たす）
    And(Vec<FindCriterion>),
    /// OR条件（いずれかの条件を満たす）
    Or(Vec<FindCriterion>),
}

/// ファイルタイプ
enum FileType {
    Regular,      // 通常ファイル
    Directory,    // ディレクトリ
    SymbolicLink, // シンボリックリンク
    BlockDevice,  // ブロックデバイス
    CharDevice,   // キャラクタデバイス
    Socket,       // ソケット
    Pipe,         // パイプ
}

/// サイズ比較の方法
enum SizeComparison {
    Exact(u64),   // 指定サイズと一致
    Less(u64),    // 指定サイズより小さい
    Greater(u64), // 指定サイズより大きい
}

/// 時間比較の方法
enum TimeComparison {
    Exact(Duration),   // 指定時間と一致
    Less(Duration),    // 指定時間より前
    Greater(Duration), // 指定時間より後
}

/// 検索結果の処理方法
enum Action {
    /// 結果をリストアップ
    Print,
    /// カスタムコマンドを実行
    Exec(String),
    /// 結果を削除
    Delete,
    /// 権限を変更
    Chmod(u32),
    /// 所有者を変更
    Chown(String),
}

#[async_trait]
impl BuiltinCommand for FindCommand {
    fn name(&self) -> &'static str {
        "find"
    }

    fn description(&self) -> &'static str {
        "指定された条件に基づいてファイルを検索します"
    }

    fn usage(&self) -> &'static str {
        "find [パス...] [条件] [アクション]\n\n\
        主な条件:\n\
        -name <パターン>     ファイル名が指定パターンに一致\n\
        -path <パターン>     ファイルパスが指定パターンに一致\n\
        -type <タイプ>       指定されたタイプのファイル (f:通常ファイル, d:ディレクトリ, l:シンボリックリンク)\n\
        -size [+-]<サイズ>   指定サイズに一致/より大きい(+)/より小さい(-)\n\
        -mtime [+-]<日数>    指定日数前に変更されたファイル\n\
        -user <ユーザー>     指定ユーザーが所有するファイル\n\
        -group <グループ>    指定グループが所有するファイル\n\
        -perm <モード>       指定パーミッションを持つファイル\n\
        -empty              空のファイルまたはディレクトリ\n\
        \n\
        論理演算子:\n\
        -not <条件>          条件の否定\n\
        <条件1> -and <条件2> 両方の条件を満たす（デフォルト）\n\
        <条件1> -or <条件2>  いずれかの条件を満たす\n\
        \n\
        アクション:\n\
        -print               結果を表示（デフォルト）\n\
        -exec <コマンド> ;   各ファイルに対してコマンドを実行（{}はファイルパスに置換）\n\
        -delete              見つかったファイルを削除\n\
        -chmod <モード>      見つかったファイルの権限を変更\n\
        -chown <ユーザー>    見つかったファイルの所有者を変更"
    }

    async fn execute(&self, context: CommandContext) -> Result<CommandResult> {
        if context.args.len() < 2 {
            return Err(anyhow!("検索パスを指定してください。使用方法: find [パス] [条件] [アクション]"));
        }

        // 引数を解析
        let mut paths = Vec::new();
        let mut criteria = Vec::new();
        let mut actions = Vec::new();
        let mut i = 1;

        // 最初にパスを解析
        while i < context.args.len() && !context.args[i].starts_with('-') {
            let path_str = &context.args[i];
            let path = if Path::new(path_str).is_absolute() {
                PathBuf::from(path_str)
            } else {
                context.current_dir.join(path_str)
            };
            paths.push(path);
            i += 1;
        }

        // パスが指定されていない場合はカレントディレクトリを使用
        if paths.is_empty() {
            paths.push(context.current_dir.clone());
        }

        // 条件とアクションを解析
        while i < context.args.len() {
            let arg = &context.args[i];
            match arg.as_str() {
                "-name" => {
                    i += 1;
                    if i < context.args.len() {
                        let pattern = &context.args[i];
                        let regex = glob_to_regex(pattern)?;
                        criteria.push(FindCriterion::Name(regex));
                    } else {
                        return Err(anyhow!("-name オプションにはパターンが必要です"));
                    }
                },
                "-path" => {
                    i += 1;
                    if i < context.args.len() {
                        let pattern = &context.args[i];
                        let regex = glob_to_regex(pattern)?;
                        criteria.push(FindCriterion::Path(regex));
                    } else {
                        return Err(anyhow!("-path オプションにはパターンが必要です"));
                    }
                },
                "-type" => {
                    i += 1;
                    if i < context.args.len() {
                        let type_char = context.args[i].chars().next()
                            .ok_or_else(|| anyhow!("-type オプションには f/d/l などの文字が必要です"))?;
                        
                        let file_type = match type_char {
                            'f' => FileType::Regular,
                            'd' => FileType::Directory,
                            'l' => FileType::SymbolicLink,
                            'b' => FileType::BlockDevice,
                            'c' => FileType::CharDevice,
                            's' => FileType::Socket,
                            'p' => FileType::Pipe,
                            _ => return Err(anyhow!("不明なファイルタイプ: {}", type_char)),
                        };
                        
                        criteria.push(FindCriterion::Type(file_type));
                    } else {
                        return Err(anyhow!("-type オプションにはタイプが必要です"));
                    }
                },
                "-size" => {
                    i += 1;
                    if i < context.args.len() {
                        let size_str = &context.args[i];
                        let size_comp = parse_size_comparison(size_str)?;
                        criteria.push(FindCriterion::Size(size_comp));
                    } else {
                        return Err(anyhow!("-size オプションにはサイズが必要です"));
                    }
                },
                "-mtime" => {
                    i += 1;
                    if i < context.args.len() {
                        let time_str = &context.args[i];
                        let time_comp = parse_time_comparison(time_str)?;
                        criteria.push(FindCriterion::ModificationTime(time_comp));
                    } else {
                        return Err(anyhow!("-mtime オプションには日数が必要です"));
                    }
                },
                "-user" => {
                    i += 1;
                    if i < context.args.len() {
                        criteria.push(FindCriterion::User(context.args[i].clone()));
                    } else {
                        return Err(anyhow!("-user オプションにはユーザー名が必要です"));
                    }
                },
                "-group" => {
                    i += 1;
                    if i < context.args.len() {
                        criteria.push(FindCriterion::Group(context.args[i].clone()));
                    } else {
                        return Err(anyhow!("-group オプションにはグループ名が必要です"));
                    }
                },
                "-perm" => {
                    i += 1;
                    if i < context.args.len() {
                        let perm_str = &context.args[i];
                        let mode = u32::from_str_radix(perm_str, 8)
                            .map_err(|_| anyhow!("無効なパーミッション: {}", perm_str))?;
                        criteria.push(FindCriterion::Permission(mode));
                    } else {
                        return Err(anyhow!("-perm オプションにはモードが必要です"));
                    }
                },
                "-empty" => {
                    criteria.push(FindCriterion::Empty);
                },
                "-not" => {
                    i += 1;
                    if i < context.args.len() {
                        // 次の条件を取得して否定
                        // 単純化のため、ここでは単一条件の否定のみサポート
                        if context.args[i].starts_with('-') {
                            let option = &context.args[i];
                            i += 1;
                            if i < context.args.len() {
                                let value = &context.args[i];
                                let inner_criterion = match option.as_str() {
                                    "-name" => {
                                        let regex = glob_to_regex(value)?;
                                        FindCriterion::Name(regex)
                                    },
                                    "-type" => {
                                        let type_char = value.chars().next()
                                            .ok_or_else(|| anyhow!("-type オプションには文字が必要です"))?;
                                        let file_type = match type_char {
                                            'f' => FileType::Regular,
                                            'd' => FileType::Directory,
                                            'l' => FileType::SymbolicLink,
                                            // ... 他のタイプ
                                            _ => return Err(anyhow!("不明なファイルタイプ: {}", type_char)),
                                        };
                                        FindCriterion::Type(file_type)
                                    },
                                    // ... 他の条件
                                    _ => return Err(anyhow!("not 演算子の後に不明なオプション: {}", option)),
                                };
                                criteria.push(FindCriterion::Not(Box::new(inner_criterion)));
                            } else {
                                return Err(anyhow!("{} オプションには値が必要です", option));
                            }
                        } else {
                            return Err(anyhow!("-not の後に条件が必要です"));
                        }
                    } else {
                        return Err(anyhow!("-not オプションの後に条件が必要です"));
                    }
                },
                // アクション解析
                "-print" => {
                    actions.push(Action::Print);
                },
                "-exec" => {
                    // -exec コマンド {} \; の形式をパース
                    let mut cmd_parts = Vec::new();
                    i += 1;
                    while i < context.args.len() && context.args[i] != ";" {
                        cmd_parts.push(context.args[i].clone());
                        i += 1;
                    }
                    
                    if i >= context.args.len() || context.args[i] != ";" {
                        return Err(anyhow!("-exec オプションは ; で終了する必要があります"));
                    }
                    
                    let command = cmd_parts.join(" ");
                    actions.push(Action::Exec(command));
                },
                "-delete" => {
                    actions.push(Action::Delete);
                },
                "-chmod" => {
                    i += 1;
                    if i < context.args.len() {
                        let mode_str = &context.args[i];
                        let mode = u32::from_str_radix(mode_str, 8)
                            .map_err(|_| anyhow!("無効なパーミッション: {}", mode_str))?;
                        actions.push(Action::Chmod(mode));
                    } else {
                        return Err(anyhow!("-chmod オプションにはモードが必要です"));
                    }
                },
                "-chown" => {
                    i += 1;
                    if i < context.args.len() {
                        actions.push(Action::Chown(context.args[i].clone()));
                    } else {
                        return Err(anyhow!("-chown オプションにはユーザー名が必要です"));
                    }
                },
                _ => {
                    return Err(anyhow!("不明なオプション: {}", arg));
                }
            }
            
            i += 1;
        }

        // デフォルトではPrintアクションを使用
        if actions.is_empty() {
            actions.push(Action::Print);
        }

        // 検索実行
        let mut output = Vec::new();
        
        for path in &paths {
            search_recursively(path, &criteria, &actions, &mut output)?;
        }
        
        Ok(CommandResult::success().with_stdout(output))
    }
}

/// ディレクトリを再帰的に検索し、条件に一致するファイルに対してアクションを実行
fn search_recursively(
    path: &Path,
    criteria: &[FindCriterion],
    actions: &[Action],
    output: &mut Vec<u8>
) -> Result<()> {
    if !path.exists() {
        warn!("パスが存在しません: {}", path.display());
        return Ok(());
    }

    // このパスが条件に一致するか確認
    if criteria.is_empty() || matches_criteria(path, criteria)? {
        // 一致した場合、アクションを実行
        for action in actions {
            execute_action(path, action, output)?;
        }
    }

    // ディレクトリの場合は再帰的に検索
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            search_recursively(&entry_path, criteria, actions, output)?;
        }
    }

    Ok(())
}

/// パスが検索条件に一致するかチェック
fn matches_criteria(path: &Path, criteria: &[FindCriterion]) -> Result<bool> {
    // 条件が空の場合は常に一致
    if criteria.is_empty() {
        return Ok(true);
    }

    // すべての条件をANDで結合（デフォルトの動作）
    for criterion in criteria {
        if !matches_criterion(path, criterion)? {
            return Ok(false);
        }
    }

    Ok(true)
}

/// 単一の検索条件に一致するかチェック
fn matches_criterion(path: &Path, criterion: &FindCriterion) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    
    match criterion {
        FindCriterion::Name(regex) => {
            let file_name = path.file_name()
                .ok_or_else(|| anyhow!("ファイル名を取得できません"))?
                .to_string_lossy();
            Ok(regex.is_match(&file_name))
        },
        FindCriterion::Path(regex) => {
            let path_str = path.to_string_lossy();
            Ok(regex.is_match(&path_str))
        },
        FindCriterion::Type(file_type) => {
            match file_type {
                FileType::Regular => Ok(metadata.is_file()),
                FileType::Directory => Ok(metadata.is_dir()),
                FileType::SymbolicLink => Ok(metadata.file_type().is_symlink()),
                FileType::BlockDevice => Ok(metadata.file_type().is_block_device()),
                FileType::CharDevice => Ok(metadata.file_type().is_char_device()),
                FileType::Socket => Ok(metadata.file_type().is_socket()),
                FileType::Pipe => Ok(metadata.file_type().is_fifo()),
            }
        },
        FindCriterion::Size(size_comp) => {
            let file_size = metadata.len();
            match size_comp {
                SizeComparison::Exact(size) => Ok(file_size == *size),
                SizeComparison::Less(size) => Ok(file_size < *size),
                SizeComparison::Greater(size) => Ok(file_size > *size),
            }
        },
        FindCriterion::ModificationTime(time_comp) => {
            let mtime = metadata.modified()?
                .duration_since(UNIX_EPOCH)
                .map_err(|e| anyhow!("時間の変換エラー: {}", e))?;
            
            let now = SystemTime::now().duration_since(UNIX_EPOCH)
                .map_err(|e| anyhow!("現在時刻の取得エラー: {}", e))?;
            
            match time_comp {
                TimeComparison::Exact(duration) => {
                    let diff = if now > mtime { now - mtime } else { mtime - now };
                    Ok(diff.as_secs() / 86400 == duration.as_secs() / 86400)
                },
                TimeComparison::Less(duration) => {
                    Ok(now - mtime > *duration)
                },
                TimeComparison::Greater(duration) => {
                    Ok(now - mtime < *duration)
                },
            }
        },
        FindCriterion::Empty => {
            if metadata.is_dir() {
                Ok(fs::read_dir(path)?.next().is_none())
            } else {
                Ok(metadata.len() == 0)
            }
        },
        FindCriterion::Not(inner) => {
            Ok(!matches_criterion(path, inner)?)
        },
        FindCriterion::And(criteria) => {
            for c in criteria {
                if !matches_criterion(path, c)? {
                    return Ok(false);
                }
            }
            Ok(true)
        },
        FindCriterion::Or(criteria) => {
            for c in criteria {
                if matches_criterion(path, c)? {
                    return Ok(true);
                }
            }
            Ok(false)
        },
        FindCriterion::User(user) => {
            #[cfg(unix)]
            {
                let meta = fs::metadata(path)?;
                if let Some(u) = get_user_by_name(user) {
                    Ok(meta.uid() == u.uid())
                } else {
                    Ok(false)
                }
            }
            #[cfg(not(unix))]
            { Ok(false) }
        },
        FindCriterion::Group(group) => {
            #[cfg(unix)]
            {
                let meta = fs::metadata(path)?;
                if let Some(g) = get_group_by_name(group) {
                    Ok(meta.gid() == g.gid())
                } else {
                    Ok(false)
                }
            }
            #[cfg(not(unix))]
            { Ok(false) }
        },
        FindCriterion::Permission(perm) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = fs::metadata(path)?;
                Ok(meta.permissions().mode() & 0o777 == *perm)
            }
            #[cfg(not(unix))]
            { Ok(false) }
        },
        _ => Ok(false),
    }
}

/// アクションを実行
fn execute_action(path: &Path, action: &Action, output: &mut Vec<u8>) -> Result<()> {
    match action {
        Action::Print => {
            output.extend_from_slice(format!("{}\n", path.display()).as_bytes());
            Ok(())
        },
        Action::Exec(command) => {
            // コマンドの {} をファイルパスに置換
            let path_str = path.to_string_lossy();
            let cmd = command.replace("{}", &path_str);
            
            // コマンドを実行
            debug!("実行: {}", cmd);
            
            // コマンドの構成要素に分解
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.is_empty() {
                return Err(anyhow!("空のコマンドを実行できません"));
            }
            
            // プロセスを生成
            let output = std::process::Command::new(parts[0])
                .args(&parts[1..])
                .output()
                .map_err(|e| anyhow!("コマンド '{}' の実行に失敗: {}", cmd, e))?;
            
            // 標準出力を追加
            if !output.stdout.is_empty() {
                output.extend_from_slice(&output.stdout);
            }
            
            // エラー出力があれば追加
            if !output.stderr.is_empty() {
                output.extend_from_slice(&output.stderr);
            }
            
            // 終了ステータスを確認
            if !output.status.success() {
                let code = output.status.code().unwrap_or(-1);
                return Err(anyhow!("コマンド '{}' が非ゼロ終了コード {} で終了しました", cmd, code));
            }
            
            Ok(())
        },
        Action::Delete => {
            debug!("削除: {}", path.display());
            if path.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
            Ok(())
        },
        Action::Chmod(mode) => {
            debug!("権限変更: {} -> {:o}", path.display(), mode);
            
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                
                // Unixプラットフォームでの権限変更
                let permissions = fs::Permissions::from_mode(*mode);
                fs::set_permissions(path, permissions)
                    .map_err(|e| anyhow!("'{}' の権限変更に失敗: {}", path.display(), e))?;
            }
            
            #[cfg(not(unix))]
            {
                // Windows等の非Unixプラットフォームでは読み書き権限のみ設定
                let mut permissions = fs::metadata(path)?.permissions();
                permissions.set_readonly(*mode & 0o200 == 0); // 書き込み不可の場合、読み取り専用に
                fs::set_permissions(path, permissions)
                    .map_err(|e| anyhow!("'{}' の権限変更に失敗: {}", path.display(), e))?;
                
                // フルモードが設定できないことを出力に記録
                output.extend_from_slice(format!("警告: 非Unixプラットフォームでは完全な権限モード {:o} の設定はサポートされていません\n", mode).as_bytes());
            }
            
            Ok(())
        },
        Action::Chown(user) => {
            debug!("所有者変更試行: {} -> {}", path.display(), user);

            #[cfg(unix)]
            {
                use nix::unistd::{chown, Uid, Gid};
                use users::{get_user_by_name, get_group_by_name};
                // use std::os::unix::fs::MetadataExt; // 既存のUID/GIDを維持する場合に必要

                let parts: Vec<&str> = user.splitn(2, ':').collect();
                let user_spec = parts.get(0).copied().filter(|s| !s.is_empty());
                let group_spec = parts.get(1).copied().filter(|s| !s.is_empty());

                let target_uid: Option<Uid> = match user_spec {
                    Some(name) => {
                        if let Ok(uid_val) = name.parse::<u32>() {
                            Some(Uid::from_raw(uid_val))
                        } else if let Some(u) = get_user_by_name(name) {
                            Some(Uid::from_raw(u.uid()))
                        } else {
                            error!("chown: ユーザー '{} ' が見つかりません。", name);
                            // エラーを伝播させるためにここでリターン
                            // return Err(anyhow!("ユーザー '{} ' が見つかりません。", name));
                            // アクションの失敗は find 全体の失敗とせず、警告に留める場合もある
                            // 今回は find 全体は続行し、個々の chown の失敗としてログ出力
                            output.extend_from_slice(format!("find: '{}': ユーザー '{} ' が見つかりません。\n", path.display(), name).as_bytes());
                            return Ok(()); // このファイルに対するアクションは失敗したが、検索は続ける
                        }
                    }
                    None => None, // ユーザー指定なし
                };

                let target_gid: Option<Gid> = match group_spec {
                    Some(name) => {
                        if let Ok(gid_val) = name.parse::<u32>() {
                            Some(Gid::from_raw(gid_val))
                        } else if let Some(g) = get_group_by_name(name) {
                            Some(Gid::from_raw(g.gid()))
                        } else {
                            error!("chown: グループ '{} ' が見つかりません。", name);
                            output.extend_from_slice(format!("find: '{}': グループ '{} ' が見つかりません。\n", path.display(), name).as_bytes());
                            return Ok(());
                        }
                    }
                    None => None, // グループ指定なし
                };

                if target_uid.is_none() && target_gid.is_none() {
                    warn!("chown: '{}': ユーザーもグループも指定されていません。変更はありません。", path.display());
                    output.extend_from_slice(format!("find: '{}': ユーザーもグループも指定されていません。\n", path.display()).as_bytes());
                } else {
                    debug!("chown {} を実行: UID={:?}, GID={:?}", path.display(), target_uid, target_gid);
                    if let Err(e) = chown(path, target_uid, target_gid) {
                        error!("chown 失敗 '{}': {}", path.display(), e);
                        output.extend_from_slice(format!("find: '{}': chown失敗: {}\n", path.display(), e).as_bytes());
                        // return Err(anyhow!("chown 失敗 '{}': {}", path.display(), e));
                    } else {
                        info!("{} の所有者を {:?}:{:?} に変更しました。", path.display(), target_uid, target_gid);
                        // 成功時は標準出力には何も出さないのが一般的
                    }
                }
            }

            #[cfg(windows)]
            {
                warn!("Windows環境でのchownアクション ({}) は現在サポートされていません。", path.display());
                output.extend_from_slice(format!("find: '{}': Windowsではchownはサポートされていません。\n", path.display()).as_bytes());
            }
        },
    }
}

/// グロブパターンを正規表現に変換
fn glob_to_regex(pattern: &str) -> Result<Regex> {
    let mut regex_pattern = "^".to_string();
    
    let mut i = 0;
    let chars: Vec<char> = pattern.chars().collect();
    
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    // ** は任意のディレクトリにマッチ
                    regex_pattern.push_str(".*");
                    i += 2;
                } else {
                    // * は任意の文字列にマッチ（ディレクトリ区切り文字を除く）
                    regex_pattern.push_str("[^/]*");
                    i += 1;
                }
            },
            '?' => {
                // ? は単一の文字にマッチ（ディレクトリ区切り文字を除く）
                regex_pattern.push_str("[^/]");
                i += 1;
            },
            '[' => {
                // 文字クラス
                regex_pattern.push('[');
                i += 1;
                
                if i < chars.len() && chars[i] == '!' {
                    regex_pattern.push('^');
                    i += 1;
                } else if i < chars.len() && chars[i] == ']' {
                    regex_pattern.push(']');
                    i += 1;
                }
                
                while i < chars.len() && chars[i] != ']' {
                    regex_pattern.push(chars[i]);
                    i += 1;
                }
                
                if i < chars.len() {
                    regex_pattern.push(']');
                    i += 1;
                }
            },
            '{' => {
                // 選択肢のグループ
                regex_pattern.push('(');
                i += 1;
                
                let mut first = true;
                while i < chars.len() && chars[i] != '}' {
                    if chars[i] == ',' {
                        regex_pattern.push('|');
                        i += 1;
                    } else {
                        if !first {
                            regex_pattern.push('|');
                        }
                        
                        while i < chars.len() && chars[i] != ',' && chars[i] != '}' {
                            // エスケープシーケンスを正規表現用にエスケープ
                            if "[](){}.*+?^$|\\".contains(chars[i]) {
                                regex_pattern.push('\\');
                            }
                            regex_pattern.push(chars[i]);
                            i += 1;
                        }
                        
                        first = false;
                    }
                }
                
                regex_pattern.push(')');
                if i < chars.len() {
                    i += 1; // '}'をスキップ
                }
            },
            '\\' => {
                // エスケープシーケンス
                i += 1;
                if i < chars.len() {
                    // 正規表現のメタ文字をエスケープ
                    if "[](){}.*+?^$|\\".contains(chars[i]) {
                        regex_pattern.push('\\');
                    }
                    regex_pattern.push(chars[i]);
                    i += 1;
                }
            },
            // 正規表現のメタ文字をエスケープ
            '.' | '+' | '(' | ')' | '|' | '^' | '$' => {
                regex_pattern.push('\\');
                regex_pattern.push(chars[i]);
                i += 1;
            },
            _ => {
                // その他の文字はそのまま
                regex_pattern.push(chars[i]);
                i += 1;
            }
        }
    }
    
    regex_pattern.push('$');
    Regex::new(&regex_pattern).map_err(|e| anyhow!("正規表現の作成に失敗しました: {}", e))
}

/// サイズ比較条件のパース
fn parse_size_comparison(size_str: &str) -> Result<SizeComparison> {
    let mut chars = size_str.chars();
    
    // 比較演算子を取得
    let comparison_type = match chars.next() {
        Some('+') => SizeComparison::Greater,
        Some('-') => SizeComparison::Less,
        Some(c) if c.is_digit(10) => {
            // 演算子がない場合は、最初の文字を数値の一部として扱う
            return parse_size_value(&format!("{}{}", c, chars.collect::<String>()));
        },
        _ => return Err(anyhow!("無効なサイズ指定: {}", size_str)),
    };
    
    // サイズ値を解析
    let size_value = chars.collect::<String>();
    let size = parse_size_value(&size_value)?;
    
    match size {
        SizeComparison::Exact(value) => match comparison_type {
            SizeComparison::Greater => Ok(SizeComparison::Greater(value)),
            SizeComparison::Less => Ok(SizeComparison::Less(value)),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

/// サイズ値の文字列をバイト数に変換
fn parse_size_value(size_str: &str) -> Result<SizeComparison> {
    let re = Regex::new(r"^(\d+)([kmgt]?)$")?;
    
    if let Some(caps) = re.captures(&size_str.to_lowercase()) {
        let value: u64 = caps[1].parse()?;
        let unit = caps.get(2).map_or("", |m| m.as_str());
        
        let multiplier = match unit {
            "k" => 1024,
            "m" => 1024 * 1024,
            "g" => 1024 * 1024 * 1024,
            "t" => 1024 * 1024 * 1024 * 1024,
            _ => 1,
        };
        
        Ok(SizeComparison::Exact(value * multiplier))
    } else {
        Err(anyhow!("無効なサイズ形式: {}", size_str))
    }
}

/// 時間比較条件のパース
fn parse_time_comparison(time_str: &str) -> Result<TimeComparison> {
    let mut chars = time_str.chars();
    
    // 比較演算子を取得
    let comparison_type = match chars.next() {
        Some('+') => TimeComparison::Less, // N日より古い = 現在時刻との差がN日より大きい
        Some('-') => TimeComparison::Greater, // N日より新しい = 現在時刻との差がN日より小さい
        Some(c) if c.is_digit(10) => {
            // 演算子がない場合は、最初の文字を数値の一部として扱う
            let days: u64 = format!("{}{}", c, chars.collect::<String>()).parse()?;
            return Ok(TimeComparison::Exact(Duration::from_secs(days * 86400)));
        },
        _ => return Err(anyhow!("無効な時間指定: {}", time_str)),
    };
    
    // 日数を解析
    let days: u64 = chars.collect::<String>().parse()?;
    let duration = Duration::from_secs(days * 86400); // 日数を秒に変換
    
    match comparison_type {
        TimeComparison::Less => Ok(TimeComparison::Less(duration)),
        TimeComparison::Greater => Ok(TimeComparison::Greater(duration)),
        _ => unreachable!(),
    }
}

// Unix環境でユーザー名からUIDを取得する関数
#[cfg(unix)]
fn get_user_uid(username: &str) -> Option<uid_t> {
    use std::ffi::CString;
    use libc::{passwd, getpwnam, uid_t};
    
    let c_name = match CString::new(username) {
        Ok(name) => name,
        Err(_) => return None,
    };
    
    unsafe {
        let pwd = getpwnam(c_name.as_ptr());
        if pwd.is_null() {
            None
        } else {
            Some((*pwd).pw_uid)
        }
    }
} 