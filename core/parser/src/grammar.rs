// 文法定義モジュール
// NexusShellの文法定義を管理します

use std::collections::{HashMap, HashSet};
use once_cell::sync::Lazy;
use thiserror::Error;
use std::fmt;
use regex::Regex;

/// 文法規則の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GrammarRuleKind {
    Command,
    Argument,
    Option,
    Pipeline,
    Redirection,
    Sequence,
    Compound,
    Group,
    Function,
    Loop,
    Conditional,
    Assignment,
    Expression,
}

impl fmt::Display for GrammarRuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command => write!(f, "コマンド"),
            Self::Argument => write!(f, "引数"),
            Self::Option => write!(f, "オプション"),
            Self::Pipeline => write!(f, "パイプライン"),
            Self::Redirection => write!(f, "リダイレクション"),
            Self::Sequence => write!(f, "シーケンス"),
            Self::Compound => write!(f, "複合コマンド"),
            Self::Group => write!(f, "グループ"),
            Self::Function => write!(f, "関数"),
            Self::Loop => write!(f, "ループ"),
            Self::Conditional => write!(f, "条件分岐"),
            Self::Assignment => write!(f, "変数代入"),
            Self::Expression => write!(f, "式"),
        }
    }
}

/// 文法規則の優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RulePriority {
    Lowest = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Highest = 4,
}

impl Default for RulePriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// 文法要素の出現頻度指定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occurrence {
    /// 0回または1回 (?)
    Optional,
    /// 0回以上 (*)
    ZeroOrMore,
    /// 1回以上 (+)
    OneOrMore,
    /// 厳密に1回
    Exactly,
    /// 指定回数
    Count(usize),
    /// 指定範囲内の回数
    Range(usize, usize),
}

impl Default for Occurrence {
    fn default() -> Self {
        Self::Exactly
    }
}

/// 文法規則の構成要素
#[derive(Debug, Clone)]
pub struct GrammarElement {
    /// 要素名
    pub name: String,
    /// 要素の種類
    pub kind: GrammarRuleKind,
    /// 出現頻度
    pub occurrence: Occurrence,
    /// 要素の説明
    pub description: Option<String>,
}

/// 文法規則
#[derive(Debug, Clone)]
pub struct GrammarRule {
    /// 規則名
    pub name: String,
    /// 規則の種類
    pub kind: GrammarRuleKind,
    /// 規則のパターン（人間可読形式）
    pub pattern: String,
    /// 規則の説明
    pub description: String,
    /// 規則の要素
    pub elements: Vec<GrammarElement>,
    /// 規則の優先度
    pub priority: RulePriority,
    /// 規則が追加された時刻（起動時からの秒数）
    pub added_at: f64,
    /// ユーザー定義かどうか
    pub is_user_defined: bool,
    /// 規則に関連付けられたタグ
    pub tags: HashSet<String>,
}

/// 文法検証エラー
#[derive(Debug, Error)]
pub enum GrammarValidationError {
    #[error("規則名が無効です: {0}")]
    InvalidRuleName(String),
    
    #[error("規則の要素が空です: {0}")]
    EmptyRuleElements(String),
    
    #[error("循環参照が検出されました: {0}")]
    CircularReference(String),
    
    #[error("未定義の規則が参照されました: {0} in {1}")]
    UndefinedRule(String, String),
    
    #[error("規則定義が重複しています: {0}")]
    DuplicateRule(String),
    
    #[error("無効なパターン構文: {0} in {1}")]
    InvalidPatternSyntax(String, String),
}

/// 文法規則のコレクション
pub static GRAMMAR_RULES: Lazy<HashMap<String, GrammarRule>> = Lazy::new(|| {
    let mut rules = HashMap::new();
    
    // コマンド規則
    rules.insert(
        "command".to_string(),
        GrammarRule {
            name: "command".to_string(),
            kind: GrammarRuleKind::Command,
            pattern: "[command_name] [arguments]* [redirections]*".to_string(),
            description: "コマンド実行".to_string(),
            elements: vec![
                GrammarElement {
                    name: "command_name".to_string(),
                    kind: GrammarRuleKind::Identifier,
                    occurrence: Occurrence::Exactly,
                    description: Some("コマンド名".to_string()),
                },
                GrammarElement {
                    name: "arguments".to_string(),
                    kind: GrammarRuleKind::Argument,
                    occurrence: Occurrence::ZeroOrMore,
                    description: Some("コマンド引数".to_string()),
                },
                GrammarElement {
                    name: "redirections".to_string(),
                    kind: GrammarRuleKind::Redirection,
                    occurrence: Occurrence::ZeroOrMore,
                    description: Some("入出力リダイレクション".to_string()),
                },
            ],
            priority: RulePriority::Normal,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "execution".to_string()]),
        }
    );
    
    // パイプライン規則
    rules.insert(
        "pipeline".to_string(),
        GrammarRule {
            name: "pipeline".to_string(),
            kind: GrammarRuleKind::Pipeline,
            pattern: "command (| command)+".to_string(),
            description: "パイプライン".to_string(),
            elements: vec![
                GrammarElement {
                    name: "first_command".to_string(),
                    kind: GrammarRuleKind::Command,
                    occurrence: Occurrence::Exactly,
                    description: Some("最初のコマンド".to_string()),
                },
                GrammarElement {
                    name: "pipe_commands".to_string(),
                    kind: GrammarRuleKind::Command,
                    occurrence: Occurrence::OneOrMore,
                    description: Some("パイプ後のコマンド".to_string()),
                },
            ],
            priority: RulePriority::High,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "pipeline".to_string()]),
        }
    );
    
    // リダイレクション規則
    rules.insert(
        "redirection".to_string(),
        GrammarRule {
            name: "redirection".to_string(),
            kind: GrammarRuleKind::Redirection,
            pattern: "(> | >> | < | &>) [file]".to_string(),
            description: "入出力リダイレクション".to_string(),
            elements: vec![
                GrammarElement {
                    name: "operator".to_string(),
                    kind: GrammarRuleKind::Operator,
                    occurrence: Occurrence::Exactly,
                    description: Some("リダイレクト演算子".to_string()),
                },
                GrammarElement {
                    name: "target".to_string(),
                    kind: GrammarRuleKind::Argument,
                    occurrence: Occurrence::Exactly,
                    description: Some("リダイレクト先".to_string()),
                },
            ],
            priority: RulePriority::Normal,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "io".to_string()]),
        }
    );
    
    // 条件分岐規則
    rules.insert(
        "if".to_string(),
        GrammarRule {
            name: "if".to_string(),
            kind: GrammarRuleKind::Conditional,
            pattern: "if [condition] { [commands] } else { [commands] }".to_string(),
            description: "条件分岐".to_string(),
            elements: vec![
                GrammarElement {
                    name: "condition".to_string(),
                    kind: GrammarRuleKind::Expression,
                    occurrence: Occurrence::Exactly,
                    description: Some("条件式".to_string()),
                },
                GrammarElement {
                    name: "true_branch".to_string(),
                    kind: GrammarRuleKind::Compound,
                    occurrence: Occurrence::Exactly,
                    description: Some("条件が真の場合に実行するコマンド".to_string()),
                },
                GrammarElement {
                    name: "false_branch".to_string(),
                    kind: GrammarRuleKind::Compound,
                    occurrence: Occurrence::Optional,
                    description: Some("条件が偽の場合に実行するコマンド".to_string()),
                },
            ],
            priority: RulePriority::High,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "control-flow".to_string()]),
        }
    );
    
    // ループ規則
    rules.insert(
        "for".to_string(),
        GrammarRule {
            name: "for".to_string(),
            kind: GrammarRuleKind::Loop,
            pattern: "for [var] in [items] { [commands] }".to_string(),
            description: "forループ".to_string(),
            elements: vec![
                GrammarElement {
                    name: "variable".to_string(),
                    kind: GrammarRuleKind::Identifier,
                    occurrence: Occurrence::Exactly,
                    description: Some("イテレーション変数".to_string()),
                },
                GrammarElement {
                    name: "iterable".to_string(),
                    kind: GrammarRuleKind::Expression,
                    occurrence: Occurrence::Exactly,
                    description: Some("反復対象".to_string()),
                },
                GrammarElement {
                    name: "body".to_string(),
                    kind: GrammarRuleKind::Compound,
                    occurrence: Occurrence::Exactly,
                    description: Some("ループ本体".to_string()),
                },
            ],
            priority: RulePriority::High,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "control-flow".to_string()]),
        }
    );
    
    // 変数代入規則
    rules.insert(
        "assignment".to_string(),
        GrammarRule {
            name: "assignment".to_string(),
            kind: GrammarRuleKind::Assignment,
            pattern: "[var] = [value]".to_string(),
            description: "変数代入".to_string(),
            elements: vec![
                GrammarElement {
                    name: "variable".to_string(),
                    kind: GrammarRuleKind::Identifier,
                    occurrence: Occurrence::Exactly,
                    description: Some("変数名".to_string()),
                },
                GrammarElement {
                    name: "value".to_string(),
                    kind: GrammarRuleKind::Expression,
                    occurrence: Occurrence::Exactly,
                    description: Some("代入値".to_string()),
                },
            ],
            priority: RulePriority::Normal,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "variable".to_string()]),
        }
    );
    
    // 関数定義規則
    rules.insert(
        "function".to_string(),
        GrammarRule {
            name: "function".to_string(),
            kind: GrammarRuleKind::Function,
            pattern: "fn [name]([params]*) { [body] }".to_string(),
            description: "関数定義".to_string(),
            elements: vec![
                GrammarElement {
                    name: "name".to_string(),
                    kind: GrammarRuleKind::Identifier,
                    occurrence: Occurrence::Exactly,
                    description: Some("関数名".to_string()),
                },
                GrammarElement {
                    name: "params".to_string(),
                    kind: GrammarRuleKind::Argument,
                    occurrence: Occurrence::ZeroOrMore,
                    description: Some("パラメータリスト".to_string()),
                },
                GrammarElement {
                    name: "body".to_string(),
                    kind: GrammarRuleKind::Compound,
                    occurrence: Occurrence::Exactly,
                    description: Some("関数本体".to_string()),
                },
            ],
            priority: RulePriority::High,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "function".to_string()]),
        }
    );
    
    // アレイリテラル規則
    rules.insert(
        "array".to_string(),
        GrammarRule {
            name: "array".to_string(),
            kind: GrammarRuleKind::Expression,
            pattern: "[elements,*]".to_string(),
            description: "配列リテラル".to_string(),
            elements: vec![
                GrammarElement {
                    name: "elements".to_string(),
                    kind: GrammarRuleKind::Expression,
                    occurrence: Occurrence::ZeroOrMore,
                    description: Some("配列要素".to_string()),
                },
            ],
            priority: RulePriority::Normal,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "data-structure".to_string()]),
        }
    );
    
    // オブジェクトリテラル規則
    rules.insert(
        "object".to_string(),
        GrammarRule {
            name: "object".to_string(),
            kind: GrammarRuleKind::Expression,
            pattern: "{ [key]: [value],* }".to_string(),
            description: "オブジェクトリテラル".to_string(),
            elements: vec![
                GrammarElement {
                    name: "entries".to_string(),
                    kind: GrammarRuleKind::Assignment,
                    occurrence: Occurrence::ZeroOrMore,
                    description: Some("オブジェクトエントリー".to_string()),
                },
            ],
            priority: RulePriority::Normal,
            added_at: 0.0,
            is_user_defined: false,
            tags: HashSet::from(["core".to_string(), "data-structure".to_string()]),
        }
    );
    
    rules
});

/// 文法規則マネージャー
#[derive(Debug, Default)]
pub struct GrammarManager {
    /// ユーザー定義の規則
    user_rules: HashMap<String, GrammarRule>,
    /// 検証済みの規則セット
    validated: bool,
    /// 規則の依存関係グラフ (name -> [dependencies])
    dependencies: HashMap<String, HashSet<String>>,
}

impl GrammarManager {
    /// 新しい文法規則マネージャーを作成
    pub fn new() -> Self {
        Self {
            user_rules: HashMap::new(),
            validated: false,
            dependencies: HashMap::new(),
        }
    }
    
    /// 規則を追加
    pub fn add_rule(&mut self, rule: GrammarRule) -> Result<(), GrammarValidationError> {
        // 規則名の検証
        if rule.name.is_empty() || !Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_-]*$").unwrap().is_match(&rule.name) {
            return Err(GrammarValidationError::InvalidRuleName(rule.name.clone()));
        }
        
        // 要素の検証
        if rule.elements.is_empty() {
            return Err(GrammarValidationError::EmptyRuleElements(rule.name.clone()));
        }
        
        self.user_rules.insert(rule.name.clone(), rule);
        self.validated = false;
        
        Ok(())
    }
    
    /// 規則を更新
    pub fn update_rule(&mut self, rule: GrammarRule) -> Result<(), GrammarValidationError> {
        if !self.user_rules.contains_key(&rule.name) && !GRAMMAR_RULES.contains_key(&rule.name) {
            return Err(GrammarValidationError::UndefinedRule(rule.name.clone(), "update_rule".to_string()));
        }
        
        self.user_rules.insert(rule.name.clone(), rule);
        self.validated = false;
        
        Ok(())
    }
    
    /// 規則を削除
    pub fn remove_rule(&mut self, name: &str) -> Result<(), GrammarValidationError> {
        if !self.user_rules.contains_key(name) {
            return Err(GrammarValidationError::UndefinedRule(name.to_string(), "remove_rule".to_string()));
        }
        
        self.user_rules.remove(name);
        self.validated = false;
        
        Ok(())
    }
    
    /// 指定した名前の規則を取得
    pub fn get_rule(&self, name: &str) -> Option<&GrammarRule> {
        self.user_rules.get(name).or_else(|| GRAMMAR_RULES.get(name))
    }
    
    /// 特定の種類の文法規則をすべて取得
    pub fn get_rules_by_kind(&self, kind: GrammarRuleKind) -> Vec<&GrammarRule> {
        let mut rules = Vec::new();
        
        // 組み込み規則から検索
        for rule in GRAMMAR_RULES.values() {
            if rule.kind == kind {
                rules.push(rule);
            }
        }
        
        // ユーザー定義規則から検索
        for rule in self.user_rules.values() {
            if rule.kind == kind {
                rules.push(rule);
            }
        }
        
        rules
    }
    
    /// すべての文法規則を取得
    pub fn get_all_rules(&self) -> Vec<&GrammarRule> {
        let mut rules = Vec::new();
        
        // 組み込み規則を追加
        for rule in GRAMMAR_RULES.values() {
            rules.push(rule);
        }
        
        // ユーザー定義規則を追加 (組み込み規則を上書きする可能性あり)
        for rule in self.user_rules.values() {
            rules.push(rule);
        }
        
        rules
    }
    
    /// タグで規則をフィルタリング
    pub fn get_rules_by_tag(&self, tag: &str) -> Vec<&GrammarRule> {
        let mut rules = Vec::new();
        
        // 組み込み規則から検索
        for rule in GRAMMAR_RULES.values() {
            if rule.tags.contains(tag) {
                rules.push(rule);
            }
        }
        
        // ユーザー定義規則から検索
        for rule in self.user_rules.values() {
            if rule.tags.contains(tag) {
                rules.push(rule);
            }
        }
        
        rules
    }
    
    /// 文法規則の完全性を検証
    pub fn validate(&mut self) -> Result<(), Vec<GrammarValidationError>> {
        let mut errors = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        
        // 依存関係グラフを構築
        self.build_dependency_graph();
        
        // 循環参照チェック
        for rule_name in self.user_rules.keys().chain(GRAMMAR_RULES.keys()) {
            if !visited.contains(rule_name) {
                if let Err(err) = self.check_circular_dependencies(rule_name, &mut visited, &mut stack) {
                    errors.push(err);
                }
            }
        }
        
        // 未定義規則の参照チェック
        for rule in self.user_rules.values() {
            for element in &rule.elements {
                if element.kind == GrammarRuleKind::Command 
                   || element.kind == GrammarRuleKind::Expression
                   || element.kind == GrammarRuleKind::Compound {
                    if !self.get_rule(&element.name).is_some() && !self.is_primitive_type(&element.name) {
                        errors.push(GrammarValidationError::UndefinedRule(
                            element.name.clone(),
                            rule.name.clone()
                        ));
                    }
                }
            }
        }
        
        if errors.is_empty() {
            self.validated = true;
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// 依存関係グラフを構築
    fn build_dependency_graph(&mut self) {
        self.dependencies.clear();
        
        // すべての規則をイテレート
        for rule in self.user_rules.values().chain(GRAMMAR_RULES.values()) {
            let mut deps = HashSet::new();
            
            // 規則の要素から依存関係を抽出
            for element in &rule.elements {
                // コマンド、式、複合要素は他の規則を参照する可能性がある
                if element.kind == GrammarRuleKind::Command 
                   || element.kind == GrammarRuleKind::Expression
                   || element.kind == GrammarRuleKind::Compound {
                    deps.insert(element.name.clone());
                }
            }
            
            self.dependencies.insert(rule.name.clone(), deps);
        }
    }
    
    /// 循環参照をチェック
    fn check_circular_dependencies(
        &self,
        rule_name: &str,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>
    ) -> Result<(), GrammarValidationError> {
        visited.insert(rule_name.to_string());
        stack.push(rule_name.to_string());
        
        if let Some(deps) = self.dependencies.get(rule_name) {
            for dep in deps {
                if !visited.contains(dep) {
                    if let Err(err) = self.check_circular_dependencies(dep, visited, stack) {
                        return Err(err);
                    }
                } else if stack.contains(dep) {
                    // 循環参照を検出
                    let cycle_start = stack.iter().position(|r| r == dep).unwrap();
                    let cycle = stack[cycle_start..].join(" -> ");
                    return Err(GrammarValidationError::CircularReference(
                        format!("循環参照: {}", cycle)
                    ));
                }
            }
        }
        
        stack.pop();
        Ok(())
    }
    
    /// プリミティブ型か判定
    fn is_primitive_type(&self, type_name: &str) -> bool {
        matches!(type_name,
            "string" | "integer" | "float" | "boolean" | "array" | "object" | "null"
        )
    }
}

/// 文法規則を取得
pub fn get_rule(name: &str) -> Option<&GrammarRule> {
    GRAMMAR_RULES.get(name)
}

/// 特定の種類の文法規則をすべて取得
pub fn get_rules_by_kind(kind: GrammarRuleKind) -> Vec<&GrammarRule> {
    GRAMMAR_RULES.values()
        .filter(|rule| rule.kind == kind)
        .collect()
}

/// 文法規則をすべて取得
pub fn get_all_rules() -> Vec<&GrammarRule> {
    GRAMMAR_RULES.values().collect()
}

/// 文法規則マネージャーのグローバルインスタンス
static mut GRAMMAR_MANAGER: Option<GrammarManager> = None;

/// グローバルな文法規則マネージャーを取得
pub fn get_grammar_manager() -> &'static mut GrammarManager {
    unsafe {
        if GRAMMAR_MANAGER.is_none() {
            GRAMMAR_MANAGER = Some(GrammarManager::new());
        }
        GRAMMAR_MANAGER.as_mut().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_rules() {
        let command_rule = get_rule("command").unwrap();
        assert_eq!(command_rule.kind, GrammarRuleKind::Command);
        
        let pipeline_rules = get_rules_by_kind(GrammarRuleKind::Pipeline);
        assert!(!pipeline_rules.is_empty());
        
        let all_rules = get_all_rules();
        assert!(all_rules.len() >= 6); // 少なくとも6つの規則が定義されているはず
    }
    
    #[test]
    fn test_grammar_manager() {
        let mut manager = GrammarManager::new();
        
        // 新しい規則を追加
        let custom_rule = GrammarRule {
            name: "custom_command".to_string(),
            kind: GrammarRuleKind::Command,
            pattern: "custom [arg1] [arg2]?".to_string(),
            description: "カスタムコマンド".to_string(),
            elements: vec![
                GrammarElement {
                    name: "arg1".to_string(),
                    kind: GrammarRuleKind::Argument,
                    occurrence: Occurrence::Exactly,
                    description: Some("第一引数".to_string()),
                },
                GrammarElement {
                    name: "arg2".to_string(),
                    kind: GrammarRuleKind::Argument,
                    occurrence: Occurrence::Optional,
                    description: Some("第二引数".to_string()),
                },
            ],
            priority: RulePriority::Normal,
            added_at: 0.0,
            is_user_defined: true,
            tags: HashSet::from(["custom".to_string()]),
        };
        
        assert!(manager.add_rule(custom_rule.clone()).is_ok());
        
        // 規則を取得して検証
        let retrieved_rule = manager.get_rule("custom_command").unwrap();
        assert_eq!(retrieved_rule.name, "custom_command");
        assert_eq!(retrieved_rule.elements.len(), 2);
        
        // タグでフィルタリング
        let custom_rules = manager.get_rules_by_tag("custom");
        assert_eq!(custom_rules.len(), 1);
        assert_eq!(custom_rules[0].name, "custom_command");
        
        // 種類でフィルタリング
        let command_rules = manager.get_rules_by_kind(GrammarRuleKind::Command);
        assert!(command_rules.len() >= 2); // 組み込みのcommandとcustom_command
        
        // 検証
        assert!(manager.validate().is_ok());
    }
} 