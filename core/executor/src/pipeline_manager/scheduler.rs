/**
 * パイプラインスケジューラーモジュール
 * 
 * パイプラインの実行スケジューリングを行います。
 * 依存関係を考慮した実行順序の決定や並列実行を管理します。
 */

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use futures::future::{self, Future, FutureExt};
use tokio::sync::{Semaphore, RwLock};
use tokio::time::timeout;
use anyhow::{Result, anyhow, Context};
use tracing::{debug, error, info, warn, instrument};

use super::error::PipelineError;
use super::pipeline::Pipeline;
use super::stages::{PipelineStage, StageResult};

/// スケジューリング戦略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingStrategy {
    /// 順次実行（依存関係を考慮しつつ、1つずつ処理）
    Sequential,
    /// 並列実行（依存関係を考慮しつつ、可能な限り並列処理）
    Parallel,
    /// データフロー実行（データの流れに基づいてスケジューリング）
    DataFlow,
    /// リソース効率化（利用可能なリソースを最大限に活用）
    ResourceOptimized,
}

/// 実行スケジュール
#[derive(Debug)]
pub struct ExecutionSchedule {
    /// 実行順序（ステージIDのリスト）
    execution_order: Vec<usize>,
    /// 並列実行グループ（各グループ内のステージは並列実行可能）
    parallel_groups: Vec<Vec<usize>>,
    /// 各ステージの依存関係
    stage_dependencies: HashMap<usize, HashSet<usize>>,
    /// 各ステージへの逆依存関係
    reverse_dependencies: HashMap<usize, HashSet<usize>>,
    /// スケジューリング戦略
    strategy: SchedulingStrategy,
    /// 最大並列度
    max_parallelism: usize,
}

impl ExecutionSchedule {
    /// 新しい実行スケジュールを作成
    pub fn new(strategy: SchedulingStrategy, max_parallelism: usize) -> Self {
        Self {
            execution_order: Vec::new(),
            parallel_groups: Vec::new(),
            stage_dependencies: HashMap::new(),
            reverse_dependencies: HashMap::new(),
            strategy,
            max_parallelism,
        }
    }
    
    /// ステージの依存関係を追加
    pub fn add_dependency(&mut self, stage_id: usize, depends_on: usize) {
        self.stage_dependencies
            .entry(stage_id)
            .or_insert_with(HashSet::new)
            .insert(depends_on);
            
        self.reverse_dependencies
            .entry(depends_on)
            .or_insert_with(HashSet::new)
            .insert(stage_id);
    }
    
    /// スケジュールを生成（位相ソート）
    pub fn generate_schedule(&mut self, stages: &[PipelineStage]) -> Result<(), PipelineError> {
        // ステージのIDを初期化
        let stage_ids: Vec<usize> = (0..stages.len()).collect();
        
        // 依存関係を解析
        self.analyze_dependencies(stages)?;
        
        // 戦略に基づいてスケジュールを生成
        match self.strategy {
            SchedulingStrategy::Sequential => {
                // 単純な位相ソート
                self.topological_sort(stage_ids)?;
            },
            SchedulingStrategy::Parallel => {
                // 並列グループを生成
                self.generate_parallel_groups(stage_ids)?;
            },
            SchedulingStrategy::DataFlow => {
                // データフローに基づくスケジューリング
                self.schedule_data_flow(stages)?;
            },
            SchedulingStrategy::ResourceOptimized => {
                // リソース最適化スケジューリング
                self.schedule_resource_optimized(stages)?;
            },
        }
        
        Ok(())
    }
    
    /// ステージの依存関係を解析
    fn analyze_dependencies(&mut self, stages: &[PipelineStage]) -> Result<(), PipelineError> {
        for (i, stage) in stages.iter().enumerate() {
            let dependencies = stage.dependencies().clone();
            
            for dep_name in dependencies {
                // 依存先のステージを名前で検索
                let dep_id = stages.iter().position(|s| s.name() == dep_name)
                    .ok_or_else(|| PipelineError::BuildError(
                        format!("依存先のステージが見つかりません: {}", dep_name)
                    ))?;
                
                self.add_dependency(i, dep_id);
            }
        }
        
        // 循環依存をチェック
        self.check_cyclic_dependencies()?;
        
        Ok(())
    }
    
    /// 循環依存をチェック
    fn check_cyclic_dependencies(&self) -> Result<(), PipelineError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        
        for &node in self.stage_dependencies.keys() {
            if !visited.contains(&node) {
                if self.is_cyclic_util(node, &mut visited, &mut rec_stack) {
                    return Err(PipelineError::BuildError(
                        "循環依存が検出されました".to_string()
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// 循環依存検出用の再帰関数
    fn is_cyclic_util(&self, node: usize, visited: &mut HashSet<usize>, rec_stack: &mut HashSet<usize>) -> bool {
        // 現在のノードを訪問済みおよび再帰スタックに追加
        visited.insert(node);
        rec_stack.insert(node);
        
        // このノードの依存先をチェック
        if let Some(dependencies) = self.stage_dependencies.get(&node) {
            for &dep in dependencies {
                // 未訪問の依存先を再帰的にチェック
                if !visited.contains(&dep) && self.is_cyclic_util(dep, visited, rec_stack) {
                    return true;
                } else if rec_stack.contains(&dep) {
                    // 既に再帰スタック内にある依存先は循環を意味する
                    return true;
                }
            }
        }
        
        // このノードの処理が完了したので再帰スタックから削除
        rec_stack.remove(&node);
        false
    }
    
    /// 位相ソート
    fn topological_sort(&mut self, stages: Vec<usize>) -> Result<(), PipelineError> {
        let mut result = Vec::new();
        let mut in_degree = HashMap::new();
        let mut queue = VecDeque::new();
        
        // 入次数を計算
        for &stage_id in &stages {
            in_degree.insert(stage_id, 0);
        }
        
        for (&stage_id, deps) in &self.stage_dependencies {
            for &dep in deps {
                *in_degree.entry(stage_id).or_insert(0) += 1;
            }
        }
        
        // 入次数0のノードをキューに追加
        for &stage_id in &stages {
            if in_degree.get(&stage_id).copied().unwrap_or(0) == 0 {
                queue.push_back(stage_id);
            }
        }
        
        // 位相ソートを実行
        while let Some(stage_id) = queue.pop_front() {
            result.push(stage_id);
            
            // このステージに依存する他のステージの入次数を減らす
            if let Some(rev_deps) = self.reverse_dependencies.get(&stage_id) {
                for &dependent in rev_deps {
                    if let Some(in_deg) = in_degree.get_mut(&dependent) {
                        *in_deg -= 1;
                        
                        if *in_deg == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }
        }
        
        // 全てのステージがソートされたか確認
        if result.len() != stages.len() {
            return Err(PipelineError::BuildError(
                "循環依存のため位相ソートができません".to_string()
            ));
        }
        
        self.execution_order = result;
        Ok(())
    }
    
    /// 並列グループを生成
    fn generate_parallel_groups(&mut self, stages: Vec<usize>) -> Result<(), PipelineError> {
        // まず位相ソートで実行順序を決定
        self.topological_sort(stages)?;
        
        let mut groups = Vec::new();
        let mut current_group = Vec::new();
        let mut processed = HashSet::new();
        
        // 依存関係の「レベル」でグループ化
        for &stage_id in &self.execution_order {
            let mut can_add_to_current = true;
            
            // このステージが現在のグループの何かに依存していないか確認
            if let Some(deps) = self.stage_dependencies.get(&stage_id) {
                for &dep in deps {
                    if current_group.contains(&dep) {
                        can_add_to_current = false;
                        break;
                    }
                }
            }
            
            if can_add_to_current && current_group.len() < self.max_parallelism {
                // 現在のグループに追加
                current_group.push(stage_id);
            } else {
                // 新しいグループを開始
                if !current_group.is_empty() {
                    groups.push(current_group);
                    current_group = Vec::new();
                }
                current_group.push(stage_id);
            }
            
            processed.insert(stage_id);
        }
        
        // 最後のグループを追加
        if !current_group.is_empty() {
            groups.push(current_group);
        }
        
        self.parallel_groups = groups;
        Ok(())
    }
    
    /// データフローに基づくスケジューリング
    fn schedule_data_flow(&mut self, stages: &[PipelineStage]) -> Result<(), PipelineError> {
        // この実装は単純化のため、現在は位相ソートと同じ
        let stage_ids: Vec<usize> = (0..stages.len()).collect();
        self.topological_sort(stage_ids)?;
        
        // 単一のグループとして全ステージを設定
        self.parallel_groups = vec![self.execution_order.clone()];
        
        Ok(())
    }
    
    /// リソース最適化スケジューリング
    fn schedule_resource_optimized(&mut self, stages: &[PipelineStage]) -> Result<(), PipelineError> {
        // この実装は単純化のため、現在は並列グループ化と同じ
        let stage_ids: Vec<usize> = (0..stages.len()).collect();
        self.generate_parallel_groups(stage_ids)?;
        
        Ok(())
    }
    
    /// 実行順序を取得
    pub fn get_execution_order(&self) -> &[usize] {
        &self.execution_order
    }
    
    /// 並列グループを取得
    pub fn get_parallel_groups(&self) -> &[Vec<usize>] {
        &self.parallel_groups
    }
}

/// パイプラインスケジューラー
pub struct PipelineScheduler {
    /// デフォルトの戦略
    default_strategy: SchedulingStrategy,
    /// デフォルトの最大並列度
    default_max_parallelism: usize,
    /// 実行中のパイプライン
    active_pipelines: tokio::sync::Mutex<HashSet<String>>,
    /// 同時実行制限セマフォ
    concurrency_semaphore: Arc<Semaphore>,
}

impl PipelineScheduler {
    /// 新しいパイプラインスケジューラーを作成
    pub fn new() -> Self {
        let max_parallelism = std::cmp::max(1, num_cpus::get());
        
        Self {
            default_strategy: SchedulingStrategy::Parallel,
            default_max_parallelism: max_parallelism,
            active_pipelines: tokio::sync::Mutex::new(HashSet::new()),
            concurrency_semaphore: Arc::new(Semaphore::new(max_parallelism)),
        }
    }
    
    /// デフォルト戦略を設定
    pub fn set_default_strategy(&mut self, strategy: SchedulingStrategy) {
        self.default_strategy = strategy;
    }
    
    /// デフォルト最大並列度を設定
    pub fn set_default_max_parallelism(&mut self, max_parallelism: usize) {
        self.default_max_parallelism = max_parallelism;
        // セマフォも更新
        self.concurrency_semaphore = Arc::new(Semaphore::new(max_parallelism));
    }
    
    /// パイプラインをスケジュールして実行
    #[instrument(skip(self, pipeline))]
    pub async fn schedule_and_execute(&self, pipeline: &mut Pipeline) -> Result<Vec<StageResult>, PipelineError> {
        let pipeline_id = pipeline.id().to_string();
        debug!("パイプライン {} のスケジューリングを開始", pipeline_id);
        
        // パイプラインを実行中リストに追加
        {
            let mut active = self.active_pipelines.lock().await;
            if active.contains(&pipeline_id) {
                return Err(PipelineError::ExecutionFailed(
                    format!("パイプライン {} は既に実行中です", pipeline_id)
                ));
            }
            active.insert(pipeline_id.clone());
        }
        
        // 関数終了時に実行中リストから削除する
        let _cleanup = CleanupGuard {
            pipeline_id: pipeline_id.clone(),
            active_pipelines: &self.active_pipelines,
        };
        
        // ステージを取得
        let stages = {
            let stages_lock = pipeline.stages.read().await;
            stages_lock.clone()
        };
        
        if stages.is_empty() {
            return Err(PipelineError::ExecutionFailed(
                "パイプラインにステージがありません".to_string()
            ));
        }
        
        // 実行スケジュールを作成
        let mut schedule = ExecutionSchedule::new(
            self.default_strategy,
            self.default_max_parallelism
        );
        
        schedule.generate_schedule(&stages)?;
        
        // スケジューリング戦略に基づいて実行
        let results = match self.default_strategy {
            SchedulingStrategy::Sequential => {
                self.execute_sequential(pipeline, &stages, &schedule).await?
            },
            SchedulingStrategy::Parallel => {
                self.execute_parallel(pipeline, &stages, &schedule).await?
            },
            SchedulingStrategy::DataFlow => {
                // データフロー実行はパイプラインのexecute_pipelinedを使用
                pipeline.execute_pipelined().await?;
                Vec::new() // TODO: 結果を取得
            },
            SchedulingStrategy::ResourceOptimized => {
                self.execute_resource_optimized(pipeline, &stages, &schedule).await?
            },
        };
        
        debug!("パイプライン {} のスケジュール実行が完了", pipeline_id);
        
        Ok(results)
    }
    
    /// 順次実行
    async fn execute_sequential(&self, pipeline: &Pipeline, stages: &[PipelineStage], schedule: &ExecutionSchedule) 
        -> Result<Vec<StageResult>, PipelineError> 
    {
        debug!("パイプライン {} を順次実行", pipeline.id());
        
        let mut results = Vec::new();
        
        // 実行順序に従って実行
        for &stage_idx in schedule.get_execution_order() {
            let stage = &stages[stage_idx];
            debug!("ステージ {} を実行", stage.name());
            
            // セマフォを取得（同時実行数を制限）
            let _permit = self.concurrency_semaphore.acquire().await.unwrap();
            
            // ステージを実行
            let stage_result = self.execute_stage(stage).await?;
            results.push(stage_result);
        }
        
        Ok(results)
    }
    
    /// 並列実行
    async fn execute_parallel(&self, pipeline: &Pipeline, stages: &[PipelineStage], schedule: &ExecutionSchedule)
        -> Result<Vec<StageResult>, PipelineError>
    {
        debug!("パイプライン {} を並列実行", pipeline.id());
        
        let mut all_results = Vec::new();
        
        // 各並列グループを順次実行
        for (group_idx, group) in schedule.get_parallel_groups().iter().enumerate() {
            debug!("グループ {} を並列実行（ステージ数: {}）", group_idx, group.len());
            
            // グループ内のステージを並列実行
            let mut futures = Vec::new();
            
            for &stage_idx in group {
                let stage = &stages[stage_idx];
                debug!("ステージ {} を並列実行に追加", stage.name());
                
                // ステージの実行関数
                let semaphore = self.concurrency_semaphore.clone();
                let stage_clone = stage.clone();
                
                let future = async move {
                    // セマフォを取得
                    let _permit = semaphore.acquire().await.unwrap();
                    
                    // ステージを実行
                    let result = stage_clone.execute().await;
                    (stage_idx, result)
                };
                
                futures.push(future.boxed());
            }
            
            // すべてのフューチャーを実行して結果を待つ
            let group_results = future::join_all(futures).await;
            
            // 結果を順序付けしてallResultsに追加
            let mut group_results_map: HashMap<usize, Result<(), PipelineError>> = HashMap::new();
            for (idx, result) in group_results {
                group_results_map.insert(idx, result);
            }
            
            // 結果を実行順に並べる
            for &stage_idx in group {
                if let Some(result) = group_results_map.get(&stage_idx) {
                    match result {
                        Ok(_) => all_results.push(StageResult {
                            name: stages[stage_idx].name().to_string(),
                            success: true,
                            exit_code: Some(0),
                            output: None,
                            error: None,
                            execution_time: Duration::from_secs(0), // 仮の値
                        }),
                        Err(e) => {
                            all_results.push(StageResult {
                                name: stages[stage_idx].name().to_string(),
                                success: false,
                                exit_code: Some(1),
                                output: None,
                                error: Some(e.to_string().into_bytes()),
                                execution_time: Duration::from_secs(0), // 仮の値
                            });
                            
                            // エラーが発生した場合、残りのグループは実行しない
                            return Ok(all_results);
                        }
                    }
                }
            }
        }
        
        Ok(all_results)
    }
    
    /// リソース最適化実行
    async fn execute_resource_optimized(&self, pipeline: &Pipeline, stages: &[PipelineStage], schedule: &ExecutionSchedule)
        -> Result<Vec<StageResult>, PipelineError>
    {
        // 現在の実装では並列実行と同じ
        self.execute_parallel(pipeline, stages, schedule).await
    }
    
    /// 単一ステージを実行
    async fn execute_stage(&self, stage: &PipelineStage) -> Result<StageResult, PipelineError> {
        let start_time = Instant::now();
        
        // ステージを実行
        let result = stage.execute().await;
        
        let execution_time = start_time.elapsed();
        
        match result {
            Ok(_) => Ok(StageResult {
                name: stage.name().to_string(),
                success: true,
                exit_code: Some(0),
                output: None,
                error: None,
                execution_time,
            }),
            Err(e) => Ok(StageResult {
                name: stage.name().to_string(),
                success: false,
                exit_code: Some(1),
                output: None,
                error: Some(e.to_string().into_bytes()),
                execution_time,
            }),
        }
    }
    
    /// アクティブなパイプライン数を取得
    pub async fn active_pipeline_count(&self) -> usize {
        self.active_pipelines.lock().await.len()
    }
}

impl Default for PipelineScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// クリーンアップガード（パイプライン終了時に実行中リストから削除）
struct CleanupGuard<'a> {
    pipeline_id: String,
    active_pipelines: &'a tokio::sync::Mutex<HashSet<String>>,
}

impl<'a> Drop for CleanupGuard<'a> {
    fn drop(&mut self) {
        let pipeline_id = self.pipeline_id.clone();
        let active_pipelines = self.active_pipelines.clone();
        
        tokio::spawn(async move {
            let mut active = active_pipelines.lock().await;
            active.remove(&pipeline_id);
        });
    }
} 