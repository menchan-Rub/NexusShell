/**
 * パイプラインプランナーモジュール
 * 
 * コマンドラインから実行計画を生成するためのモジュール
 */

use std::collections::HashMap;
use anyhow::{Result, Context, anyhow};
use crate::pipeline_manager::error::PipelineError;
use crate::pipeline_manager::stages::{PipelineStage, StageContext};
use std::sync::Arc;
use tracing::{debug, info, warn, error};

/// パイプラインプラン
#[derive(Debug, Clone)]
pub struct PipelinePlan {
    /// プランID
    id: String,
    /// プラン名
    name: Option<String>,
    /// ステージプラン
    stages: Vec<StagePlan>,
    /// プランのメタデータ
    metadata: HashMap<String, String>,
}

impl PipelinePlan {
    /// 新しいパイプラインプランを作成
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            stages: Vec::new(),
            metadata: HashMap::new(),
        }
    }
    
    /// 名前を設定
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    
    /// ステージプランを追加
    pub fn add_stage(&mut self, stage_plan: StagePlan) {
        self.stages.push(stage_plan);
    }
    
    /// ステージプランのリストを取得
    pub fn stages(&self) -> &[StagePlan] {
        &self.stages
    }
    
    /// メタデータを設定
    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }
    
    /// メタデータを取得
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
    
    /// IDを取得
    pub fn id(&self) -> &str {
        &self.id
    }
    
    /// 名前を取得
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// ステージプラン
#[derive(Debug, Clone)]
pub struct StagePlan {
    /// ステージ名
    name: String,
    /// ステージ種別
    stage_type: StageType,
    /// ステージ設定
    config: HashMap<String, String>,
    /// 依存関係
    dependencies: Vec<String>,
}

impl StagePlan {
    /// 新しいステージプランを作成
    pub fn new(name: impl Into<String>, stage_type: StageType) -> Self {
        Self {
            name: name.into(),
            stage_type,
            config: HashMap::new(),
            dependencies: Vec::new(),
        }
    }
    
    /// 設定を追加
    pub fn with_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.insert(key.into(), value.into());
        self
    }
    
    /// 依存関係を追加
    pub fn with_dependency(mut self, dependency: impl Into<String>) -> Self {
        self.dependencies.push(dependency.into());
        self
    }
    
    /// 依存関係を複数追加
    pub fn with_dependencies(mut self, dependencies: Vec<impl Into<String>>) -> Self {
        for dep in dependencies {
            self.dependencies.push(dep.into());
        }
        self
    }
    
    /// 名前を取得
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// ステージ種別を取得
    pub fn stage_type(&self) -> &StageType {
        &self.stage_type
    }
    
    /// 設定を取得
    pub fn config(&self) -> &HashMap<String, String> {
        &self.config
    }
    
    /// 依存関係を取得
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

/// ステージ種別
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageType {
    /// コマンド実行ステージ
    Command(String),
    /// パイプステージ
    Pipe,
    /// フィルターステージ
    Filter(String),
    /// マップステージ
    Map(String),
    /// リダイレクションステージ
    Redirect(RedirectType),
    /// サブシェルステージ
    Subshell,
    /// スクリプトステージ
    Script(String),
    /// カスタムステージ
    Custom(String),
}

/// リダイレクト種別
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectType {
    /// 標準出力リダイレクト
    StdoutToFile(String),
    /// 標準出力追加リダイレクト
    StdoutAppendToFile(String),
    /// 標準エラー出力リダイレクト
    StderrToFile(String),
    /// 標準入力リダイレクト
    StdinFromFile(String),
    /// 標準出力と標準エラー出力をマージ
    MergeStderrToStdout,
}

/// パイプラインプランナー
pub struct PipelinePlanner {
    /// コマンドパーサー
    command_parser: Option<Arc<dyn CommandParser>>,
    /// プランキャッシュ
    plan_cache: HashMap<String, PipelinePlan>,
}

impl PipelinePlanner {
    /// 新しいパイプラインプランナーを作成
    pub fn new() -> Self {
        Self {
            command_parser: None,
            plan_cache: HashMap::new(),
        }
    }
    
    /// コマンドパーサーを設定
    pub fn with_parser(mut self, parser: impl CommandParser + 'static) -> Self {
        self.command_parser = Some(Arc::new(parser));
        self
    }
    
    /// プランを作成
    pub async fn create_plan(&self, command_line: &str) -> Result<PipelinePlan, PipelineError> {
        debug!("パイプラインプランを作成中: '{}'", command_line);
        
        // 既存のキャッシュからプランを探す
        if let Some(cached_plan) = self.plan_cache.get(command_line) {
            debug!("キャッシュからプランを取得: {}", cached_plan.id());
            return Ok(cached_plan.clone());
        }
        
        // 新しいプランを作成
        let plan_id = format!("plan-{}", uuid::Uuid::new_v4());
        let mut plan = PipelinePlan::new(plan_id).with_name(command_line.to_string());
        
        // コマンドラインを解析
        if let Some(parser) = &self.command_parser {
            match parser.parse(command_line) {
                Ok(commands) => {
                    self.build_plan_from_commands(&mut plan, commands)?;
                }
                Err(e) => {
                    return Err(PipelineError::SyntaxError(format!("コマンドラインの解析に失敗: {}", e)));
                }
            }
        } else {
            // パーサーがない場合は単一コマンドステージとして扱う
            let stage_plan = StagePlan::new(
                "command",
                StageType::Command(command_line.to_string())
            );
            plan.add_stage(stage_plan);
        }
        
        // メタデータの設定
        plan.set_metadata("created_at", chrono::Utc::now().to_rfc3339());
        plan.set_metadata("command_line", command_line.to_string());
        
        // プランを返す（ここでキャッシュに保存することもできる）
        
        debug!("パイプラインプラン作成完了: {} (ステージ数: {})", plan.id(), plan.stages().len());
        Ok(plan)
    }
    
    /// コマンドからプランを構築
    fn build_plan_from_commands(&self, plan: &mut PipelinePlan, commands: Vec<ParsedCommand>) -> Result<(), PipelineError> {
        if commands.is_empty() {
            return Err(PipelineError::BuildError("コマンドが空です".to_string()));
        }
        
        let mut prev_stage_name = None;
        
        for (i, cmd) in commands.iter().enumerate() {
            let stage_name = format!("stage-{}", i);
            
            let mut stage_plan = match &cmd.kind {
                CommandKind::Simple(cmd_str) => {
                    StagePlan::new(stage_name.clone(), StageType::Command(cmd_str.clone()))
                },
                CommandKind::Pipe => {
                    StagePlan::new(stage_name.clone(), StageType::Pipe)
                },
                CommandKind::Redirect(redirect_type) => {
                    StagePlan::new(stage_name.clone(), StageType::Redirect(redirect_type.clone()))
                },
                CommandKind::Subshell(subcmds) => {
                    let mut subplan = PipelinePlan::new(format!("subplan-{}", i));
                    self.build_plan_from_commands(&mut subplan, subcmds.clone())?;
                    
                    // サブプランからサブシェルステージを作成
                    let mut stage = StagePlan::new(stage_name.clone(), StageType::Subshell);
                    
                    // サブプラン情報を設定に追加
                    for (j, substage) in subplan.stages().iter().enumerate() {
                        stage.config.insert(
                            format!("substage_{}_name", j),
                            substage.name().to_string()
                        );
                        stage.config.insert(
                            format!("substage_{}_type", j),
                            format!("{:?}", substage.stage_type())
                        );
                    }
                    
                    stage
                },
                CommandKind::Custom(name, args) => {
                    let mut stage = StagePlan::new(stage_name.clone(), StageType::Custom(name.clone()));
                    
                    // 引数を設定に追加
                    for (j, arg) in args.iter().enumerate() {
                        stage.config.insert(format!("arg_{}", j), arg.clone());
                    }
                    
                    stage
                },
            };
            
            // 前のステージへの依存関係を追加
            if let Some(prev_name) = prev_stage_name {
                stage_plan = stage_plan.with_dependency(prev_name);
            }
            
            // このステージをプランに追加
            plan.add_stage(stage_plan);
            
            // 次のステージのために、このステージ名を記録
            prev_stage_name = Some(stage_name);
        }
        
        Ok(())
    }
    
    /// プランをキャッシュ
    pub fn cache_plan(&mut self, command_line: String, plan: PipelinePlan) {
        self.plan_cache.insert(command_line, plan);
    }
    
    /// キャッシュからプランを取得
    pub fn get_cached_plan(&self, command_line: &str) -> Option<&PipelinePlan> {
        self.plan_cache.get(command_line)
    }
    
    /// キャッシュをクリア
    pub fn clear_cache(&mut self) {
        self.plan_cache.clear();
    }
}

impl Default for PipelinePlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析されたコマンド
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    /// コマンド種別
    kind: CommandKind,
    /// コマンドの位置
    position: usize,
}

impl ParsedCommand {
    /// 新しい解析済みコマンドを作成
    pub fn new(kind: CommandKind, position: usize) -> Self {
        Self {
            kind,
            position,
        }
    }
    
    /// 種別を取得
    pub fn kind(&self) -> &CommandKind {
        &self.kind
    }
    
    /// 位置を取得
    pub fn position(&self) -> usize {
        self.position
    }
}

/// コマンド種別
#[derive(Debug, Clone)]
pub enum CommandKind {
    /// 単純コマンド
    Simple(String),
    /// パイプ
    Pipe,
    /// リダイレクト
    Redirect(RedirectType),
    /// サブシェル
    Subshell(Vec<ParsedCommand>),
    /// カスタムコマンド
    Custom(String, Vec<String>),
}

/// コマンドパーサーのトレイト
pub trait CommandParser: Send + Sync {
    /// コマンドラインを解析
    fn parse(&self, command_line: &str) -> Result<Vec<ParsedCommand>, String>;
}

/// シンプルなコマンドパーサー実装
pub struct SimpleCommandParser;

impl SimpleCommandParser {
    /// 新しいパーサーを作成
    pub fn new() -> Self {
        Self
    }
}

impl CommandParser for SimpleCommandParser {
    fn parse(&self, command_line: &str) -> Result<Vec<ParsedCommand>, String> {
        let mut commands = Vec::new();
        let mut position = 0;
        
        // 簡易パース - パイプ区切りを処理
        let parts: Vec<&str> = command_line.split('|').collect();
        
        for (i, part) in parts.iter().enumerate() {
            let trimmed = part.trim();
            
            if !trimmed.is_empty() {
                commands.push(ParsedCommand::new(
                    CommandKind::Simple(trimmed.to_string()),
                    position
                ));
                position += trimmed.len();
            }
            
            // パイプを追加（最後の部分以外）
            if i < parts.len() - 1 {
                commands.push(ParsedCommand::new(
                    CommandKind::Pipe,
                    position
                ));
                position += 1; // パイプ文字のサイズ
            }
        }
        
        Ok(commands)
    }
} 