/**
 * パイプライン最適化エンジン
 * 
 * パイプライン実行計画を最適化し、効率的な実行を実現するモジュール
 */

use std::collections::{HashMap, HashSet};
use std::time::Duration;
use anyhow::{Result, Context};
use tracing::{debug, info, warn, error};

use super::error::PipelineError;
use super::planner::{PipelinePlan, StagePlan, StageType};

/// 最適化オプション
#[derive(Debug, Clone)]
pub struct OptimizationOptions {
    /// ステージの融合を許可するか
    pub enable_stage_fusion: bool,
    /// 並列化を許可するか
    pub enable_parallelization: bool,
    /// データローカリティ最適化を許可するか
    pub enable_data_locality: bool,
    /// リソース制約の考慮を許可するか
    pub enable_resource_constraints: bool,
    /// 最適化レベル（0-3）
    pub optimization_level: u8,
    /// 最適化のタイムアウト（秒）
    pub timeout_seconds: u64,
}

impl Default for OptimizationOptions {
    fn default() -> Self {
        Self {
            enable_stage_fusion: true,
            enable_parallelization: true,
            enable_data_locality: true,
            enable_resource_constraints: true,
            optimization_level: 1,
            timeout_seconds: 10,
        }
    }
}

/// パイプライン最適化エンジン
pub struct PipelineOptimizer {
    /// 最適化オプション
    options: OptimizationOptions,
    /// コスト計算モデル
    cost_model: CostModel,
    /// 最適化統計情報
    stats: OptimizationStats,
}

impl PipelineOptimizer {
    /// 新しいパイプライン最適化エンジンを作成
    pub fn new() -> Self {
        Self {
            options: OptimizationOptions::default(),
            cost_model: CostModel::default(),
            stats: OptimizationStats::default(),
        }
    }
    
    /// オプションを設定
    pub fn with_options(mut self, options: OptimizationOptions) -> Self {
        self.options = options;
        self
    }
    
    /// コストモデルを設定
    pub fn with_cost_model(mut self, cost_model: CostModel) -> Self {
        self.cost_model = cost_model;
        self
    }
    
    /// パイプラインプランを最適化
    pub async fn optimize(&self, plan: PipelinePlan) -> Result<PipelinePlan, PipelineError> {
        debug!("パイプラインプラン '{}' の最適化を開始", plan.id());
        
        let mut optimized_plan = plan.clone();
        let mut stats = OptimizationStats::default();
        
        // 最適化レベルに基づいて最適化
        match self.options.optimization_level {
            0 => {
                // 最適化なし
                debug!("最適化レベル0: 最適化なし");
                return Ok(plan);
            },
            1 => {
                // 基本的な最適化
                debug!("最適化レベル1: 基本的な最適化を実行");
                if self.options.enable_stage_fusion {
                    self.apply_stage_fusion(&mut optimized_plan, &mut stats)?;
                }
            },
            2 => {
                // 中間レベルの最適化
                debug!("最適化レベル2: 中間レベルの最適化を実行");
                if self.options.enable_stage_fusion {
                    self.apply_stage_fusion(&mut optimized_plan, &mut stats)?;
                }
                if self.options.enable_parallelization {
                    self.apply_parallelization(&mut optimized_plan, &mut stats)?;
                }
            },
            _ => {
                // 最大レベルの最適化
                debug!("最適化レベル3+: すべての最適化を実行");
                if self.options.enable_stage_fusion {
                    self.apply_stage_fusion(&mut optimized_plan, &mut stats)?;
                }
                if self.options.enable_parallelization {
                    self.apply_parallelization(&mut optimized_plan, &mut stats)?;
                }
                if self.options.enable_data_locality {
                    self.apply_data_locality(&mut optimized_plan, &mut stats)?;
                }
                if self.options.enable_resource_constraints {
                    self.apply_resource_constraints(&mut optimized_plan, &mut stats)?;
                }
                // 最終的な最適化チューニング
                self.apply_final_tuning(&mut optimized_plan, &mut stats)?;
            }
        }
        
        // コスト計算と最終調整
        let original_cost = self.cost_model.estimate_cost(&plan);
        let optimized_cost = self.cost_model.estimate_cost(&optimized_plan);
        
        // 最適化の効果を計測
        stats.original_cost = original_cost;
        stats.optimized_cost = optimized_cost;
        stats.improvement_percent = if original_cost > 0.0 {
            ((original_cost - optimized_cost) / original_cost) * 100.0
        } else {
            0.0
        };
        
        // 最適化情報をプランのメタデータに追加
        let mut optimized_plan = optimized_plan;
        optimized_plan.set_metadata("optimization_level", self.options.optimization_level.to_string());
        optimized_plan.set_metadata("original_cost", format!("{:.2}", original_cost));
        optimized_plan.set_metadata("optimized_cost", format!("{:.2}", optimized_cost));
        optimized_plan.set_metadata("improvement_percent", format!("{:.2}%", stats.improvement_percent));
        optimized_plan.set_metadata("optimized_at", chrono::Utc::now().to_rfc3339());
        
        debug!("パイプラインプラン '{}' の最適化が完了: 改善率 {:.2}%", 
               optimized_plan.id(), stats.improvement_percent);
        
        // 最適化情報を内部に保存
        let mut stats_mut = self.stats.clone();
        stats_mut.plans_optimized += 1;
        stats_mut.total_improvement_percent += stats.improvement_percent;
        stats_mut.average_improvement_percent = stats_mut.total_improvement_percent / stats_mut.plans_optimized as f64;
        
        Ok(optimized_plan)
    }
    
    /// ステージ融合の適用
    fn apply_stage_fusion(&self, plan: &mut PipelinePlan, stats: &mut OptimizationStats) -> Result<(), PipelineError> {
        debug!("ステージ融合を適用");
        
        let stages = plan.stages().to_vec();
        if stages.len() <= 1 {
            return Ok(());
        }
        
        // 融合可能なステージペアを特定
        let mut fusion_candidates = Vec::new();
        
        for i in 0..stages.len() - 1 {
            for j in i + 1..stages.len() {
                if self.can_fuse_stages(&stages[i], &stages[j]) {
                    fusion_candidates.push((i, j));
                }
            }
        }
        
        if fusion_candidates.is_empty() {
            debug!("融合可能なステージが見つかりませんでした");
            return Ok(());
        }
        
        // 各候補のコスト削減効果を計算
        let mut cost_reductions = Vec::new();
        
        for (i, j) in &fusion_candidates {
            let cost_before = self.cost_model.estimate_stage_cost(&stages[*i]) + 
                              self.cost_model.estimate_stage_cost(&stages[*j]);
                              
            let fused_stage = self.create_fused_stage(&stages[*i], &stages[*j])?;
            let cost_after = self.cost_model.estimate_stage_cost(&fused_stage);
            
            let reduction = cost_before - cost_after;
            cost_reductions.push((i, j, reduction));
        }
        
        // コスト削減効果順にソート
        cost_reductions.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        
        // 最良の融合を適用
        let mut fused_indices = HashSet::new();
        let mut new_stages = Vec::new();
        
        for (i, j, reduction) in cost_reductions {
            if fused_indices.contains(i) || fused_indices.contains(j) {
                continue;
            }
            
            if reduction <= 0.0 {
                continue;
            }
            
            // ステージを融合
            let fused_stage = self.create_fused_stage(&stages[*i], &stages[*j])?;
            new_stages.push(fused_stage);
            
            fused_indices.insert(*i);
            fused_indices.insert(*j);
            
            stats.fused_stages += 1;
        }
        
        // 融合されなかったステージを追加
        for (i, stage) in stages.into_iter().enumerate() {
            if !fused_indices.contains(&i) {
                new_stages.push(stage);
            }
        }
        
        // プランを更新
        let mut new_plan = PipelinePlan::new(plan.id().to_string());
        if let Some(name) = plan.name() {
            new_plan = new_plan.with_name(name.to_string());
        }
        
        for stage in new_stages {
            new_plan.add_stage(stage);
        }
        
        // メタデータを転送
        for (key, value) in plan.get_metadata("*").unwrap_or_default() {
            new_plan.set_metadata(key, value);
        }
        
        *plan = new_plan;
        
        debug!("ステージ融合を適用: {} ステージを融合", stats.fused_stages);
        Ok(())
    }
    
    /// ステージを融合できるか判定
    fn can_fuse_stages(&self, stage1: &StagePlan, stage2: &StagePlan) -> bool {
        // ステージタイプに基づく融合可能性判定
        match (stage1.stage_type(), stage2.stage_type()) {
            (StageType::Filter(_), StageType::Filter(_)) => true,
            (StageType::Map(_), StageType::Map(_)) => true,
            (StageType::Command(cmd1), StageType::Command(cmd2)) => {
                // 単純なコマンドの場合、一定の条件で融合可能
                // 例: echoコマンドとgrep/sedのような単純パイプライン
                cmd1.contains("echo") && (cmd2.contains("grep") || cmd2.contains("sed"))
            },
            // その他の組み合わせは融合不可
            _ => false,
        }
    }
    
    /// 融合されたステージを作成
    fn create_fused_stage(&self, stage1: &StagePlan, stage2: &StagePlan) -> Result<StagePlan, PipelineError> {
        let new_name = format!("fused_{}_and_{}", stage1.name(), stage2.name());
        
        // ステージタイプに基づく融合ロジック
        let new_type = match (stage1.stage_type(), stage2.stage_type()) {
            (StageType::Filter(f1), StageType::Filter(f2)) => {
                // フィルターを結合
                StageType::Filter(format!("({}) AND ({})", f1, f2))
            },
            (StageType::Map(m1), StageType::Map(m2)) => {
                // マッパーを結合
                StageType::Map(format!("{} >> {}", m1, m2))
            },
            (StageType::Command(cmd1), StageType::Command(cmd2)) => {
                // コマンドをパイプで結合
                StageType::Command(format!("{} | {}", cmd1, cmd2))
            },
            _ => {
                return Err(PipelineError::BuildError(
                    format!("サポートされていないステージ融合: {:?} + {:?}", 
                            stage1.stage_type(), stage2.stage_type())
                ));
            }
        };
        
        let mut new_stage = StagePlan::new(new_name, new_type);
        
        // 設定の結合
        for (key, value) in stage1.config() {
            new_stage = new_stage.with_config(format!("1_{}", key), value);
        }
        
        for (key, value) in stage2.config() {
            new_stage = new_stage.with_config(format!("2_{}", key), value);
        }
        
        // 依存関係の結合
        let mut all_deps = stage1.dependencies().to_vec();
        all_deps.extend_from_slice(stage2.dependencies());
        
        if !all_deps.is_empty() {
            new_stage = new_stage.with_dependencies(all_deps);
        }
        
        Ok(new_stage)
    }
    
    /// 並列化の適用
    fn apply_parallelization(&self, plan: &mut PipelinePlan, stats: &mut OptimizationStats) -> Result<(), PipelineError> {
        debug!("並列化最適化を適用");
        
        let stages = plan.stages().to_vec();
        if stages.len() <= 1 {
            return Ok(());
        }
        
        // 並列化可能なステージを特定
        let mut parallelizable_stages = Vec::new();
        
        for (i, stage) in stages.iter().enumerate() {
            if self.is_parallelizable(stage) {
                parallelizable_stages.push((i, stage.clone()));
            }
        }
        
        if parallelizable_stages.is_empty() {
            debug!("並列化可能なステージが見つかりませんでした");
            return Ok(());
        }
        
        // 並列化の適用
        let mut new_stages = stages.clone();
        
        for (i, stage) in parallelizable_stages {
            // 並列度を決定
            let parallelism = self.determine_parallelism(&stage);
            
            if parallelism <= 1 {
                continue;
            }
            
            // 並列化設定を追加
            let mut new_stage = stage.clone();
            new_stage = new_stage.with_config("parallelism", parallelism.to_string());
            new_stage = new_stage.with_config("parallel_execution", "true");
            
            // ステージを更新
            new_stages[i] = new_stage;
            stats.parallelized_stages += 1;
        }
        
        // プランを更新
        let mut new_plan = PipelinePlan::new(plan.id().to_string());
        if let Some(name) = plan.name() {
            new_plan = new_plan.with_name(name.to_string());
        }
        
        for stage in new_stages {
            new_plan.add_stage(stage);
        }
        
        // メタデータを転送
        for (key, value) in plan.get_metadata("*").unwrap_or_default() {
            new_plan.set_metadata(key, value);
        }
        
        *plan = new_plan;
        
        debug!("並列化最適化を適用: {} ステージを並列化", stats.parallelized_stages);
        Ok(())
    }
    
    /// ステージが並列化可能か判定
    fn is_parallelizable(&self, stage: &StagePlan) -> bool {
        match stage.stage_type() {
            StageType::Filter(_) | StageType::Map(_) => true,
            StageType::Command(cmd) => {
                // 特定のコマンドは並列化可能
                cmd.contains("grep") || cmd.contains("sort") || cmd.contains("find")
            },
            _ => false,
        }
    }
    
    /// 適切な並列度を決定
    fn determine_parallelism(&self, stage: &StagePlan) -> usize {
        // コストモデルに基づく並列度の決定
        let base_cost = self.cost_model.estimate_stage_cost(stage);
        
        // 簡易モデル: コストが高いほど高い並列度を割り当て
        if base_cost > 100.0 {
            4
        } else if base_cost > 50.0 {
            2
        } else {
            1
        }
    }
    
    /// データローカリティ最適化の適用
    fn apply_data_locality(&self, plan: &mut PipelinePlan, stats: &mut OptimizationStats) -> Result<(), PipelineError> {
        debug!("データローカリティ最適化を適用");
        
        // この実装は簡易的なもの
        stats.data_locality_optimizations += 1;
        
        Ok(())
    }
    
    /// リソース制約の適用
    fn apply_resource_constraints(&self, plan: &mut PipelinePlan, stats: &mut OptimizationStats) -> Result<(), PipelineError> {
        debug!("リソース制約最適化を適用");
        
        // この実装は簡易的なもの
        stats.resource_constraints_applied += 1;
        
        Ok(())
    }
    
    /// 最終調整
    fn apply_final_tuning(&self, plan: &mut PipelinePlan, stats: &mut OptimizationStats) -> Result<(), PipelineError> {
        debug!("最終最適化調整を適用");
        
        // この実装は簡易的なもの
        stats.final_tunings_applied += 1;
        
        Ok(())
    }
    
    /// 最適化統計情報を取得
    pub fn get_stats(&self) -> &OptimizationStats {
        &self.stats
    }
}

impl Default for PipelineOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// 最適化統計情報
#[derive(Debug, Clone, Default)]
pub struct OptimizationStats {
    /// 最適化されたプラン数
    pub plans_optimized: usize,
    /// 融合されたステージ数
    pub fused_stages: usize,
    /// 並列化されたステージ数
    pub parallelized_stages: usize,
    /// データローカリティ最適化数
    pub data_locality_optimizations: usize,
    /// リソース制約適用数
    pub resource_constraints_applied: usize,
    /// 最終調整適用数
    pub final_tunings_applied: usize,
    /// 元のコスト
    pub original_cost: f64,
    /// 最適化後のコスト
    pub optimized_cost: f64,
    /// 改善率（%）
    pub improvement_percent: f64,
    /// 合計改善率（%）
    pub total_improvement_percent: f64,
    /// 平均改善率（%）
    pub average_improvement_percent: f64,
}

/// コスト計算モデル
#[derive(Debug, Clone)]
pub struct CostModel {
    /// メモリコスト重み
    memory_weight: f64,
    /// CPU時間コスト重み
    cpu_weight: f64,
    /// 入出力コスト重み
    io_weight: f64,
    /// ネットワークコスト重み
    network_weight: f64,
    /// コスト係数マップ
    cost_factors: HashMap<String, f64>,
}

impl CostModel {
    /// 新しいコストモデルを作成
    pub fn new() -> Self {
        let mut cost_factors = HashMap::new();
        
        // 基本的なコスト係数の設定
        cost_factors.insert("command".to_string(), 10.0);
        cost_factors.insert("pipe".to_string(), 5.0);
        cost_factors.insert("filter".to_string(), 3.0);
        cost_factors.insert("map".to_string(), 4.0);
        cost_factors.insert("redirect".to_string(), 7.0);
        cost_factors.insert("subshell".to_string(), 15.0);
        
        Self {
            memory_weight: 1.0,
            cpu_weight: 2.0,
            io_weight: 1.5,
            network_weight: 3.0,
            cost_factors,
        }
    }
    
    /// プラン全体のコストを推定
    pub fn estimate_cost(&self, plan: &PipelinePlan) -> f64 {
        let mut total_cost = 0.0;
        
        for stage in plan.stages() {
            let stage_cost = self.estimate_stage_cost(stage);
            total_cost += stage_cost;
        }
        
        total_cost
    }
    
    /// ステージのコストを推定
    pub fn estimate_stage_cost(&self, stage: &StagePlan) -> f64 {
        let base_cost = match stage.stage_type() {
            StageType::Command(cmd) => {
                let factor = self.cost_factors.get("command").copied().unwrap_or(10.0);
                
                // コマンドの複雑さに基づくコスト
                let complexity = if cmd.contains("|") {
                    1.5 + 0.5 * cmd.matches("|").count() as f64
                } else {
                    1.0
                };
                
                factor * complexity
            },
            StageType::Pipe => {
                self.cost_factors.get("pipe").copied().unwrap_or(5.0)
            },
            StageType::Filter(_) => {
                self.cost_factors.get("filter").copied().unwrap_or(3.0)
            },
            StageType::Map(_) => {
                self.cost_factors.get("map").copied().unwrap_or(4.0)
            },
            StageType::Redirect(_) => {
                self.cost_factors.get("redirect").copied().unwrap_or(7.0)
            },
            StageType::Subshell => {
                self.cost_factors.get("subshell").copied().unwrap_or(15.0)
            },
            _ => 10.0, // デフォルトコスト
        };
        
        // 並列度による調整
        let parallelism = stage.config().get("parallelism")
            .and_then(|p| p.parse::<f64>().ok())
            .unwrap_or(1.0);
            
        let parallel_factor = if parallelism > 1.0 {
            // 並列化によるオーバーヘッドを考慮
            0.7 + (0.3 / parallelism)
        } else {
            1.0
        };
        
        base_cost * parallel_factor
    }
    
    /// ステージ間のデータ転送コストを推定
    pub fn estimate_transfer_cost(&self, _from_stage: &StagePlan, _to_stage: &StagePlan) -> f64 {
        // 簡易実装: 固定コスト
        2.0
    }
}

impl Default for CostModel {
    fn default() -> Self {
        Self::new()
    }
} 