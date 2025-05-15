/**
 * パイプラインプロセッサモジュール
 * 
 * パイプラインのデータ処理とフローを実装します。
 */

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{mpsc, RwLock};
use anyhow::{Result, anyhow};

use crate::pipeline_manager::error::PipelineError;
use crate::pipeline_manager::{PipelineData, StageContext, Pipeline};

/// プロセッサの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessorKind {
    /// フィルター（データをフィルタリング）
    Filter,
    /// マッパー（データを変換）
    Mapper,
    /// リデューサー（データを集約）
    Reducer,
    /// スプリッター（データを分割）
    Splitter,
    /// ジョイナー（データを結合）
    Joiner,
    /// ソーター（データをソート）
    Sorter,
    /// グルーパー（データをグループ化）
    Grouper,
}

/// データプロセッサのトレイト
pub trait DataProcessor: Send + Sync {
    /// プロセッサの名前を取得
    fn name(&self) -> &str;
    
    /// プロセッサの種類を取得
    fn kind(&self) -> ProcessorKind;
    
    /// データを処理
    fn process(&self, data: PipelineData) -> Result<PipelineData, PipelineError>;
    
    /// 非同期データ処理
    async fn process_async(&self, data: PipelineData) -> Result<PipelineData, PipelineError> {
        // デフォルト実装は同期処理を呼び出す
        self.process(data)
    }
    
    /// バッチ処理（複数データを一括処理）
    fn process_batch(&self, batch: Vec<PipelineData>) -> Result<Vec<PipelineData>, PipelineError> {
        // デフォルト実装は各データを個別に処理
        let mut results = Vec::with_capacity(batch.len());
        
        for data in batch {
            results.push(self.process(data)?);
        }
        
        Ok(results)
    }
    
    /// 非同期バッチ処理
    async fn process_batch_async(&self, batch: Vec<PipelineData>) -> Result<Vec<PipelineData>, PipelineError> {
        // デフォルト実装は各データを順次処理
        let mut results = Vec::with_capacity(batch.len());
        
        for data in batch {
            results.push(self.process_async(data).await?);
        }
        
        Ok(results)
    }
}

/// フィルタープロセッサ
pub struct FilterProcessor {
    /// プロセッサ名
    name: String,
    /// フィルター条件
    filter: Box<dyn Fn(&PipelineData) -> bool + Send + Sync>,
}

impl FilterProcessor {
    /// 新しいフィルタープロセッサを作成
    pub fn new(name: impl Into<String>, filter: impl Fn(&PipelineData) -> bool + Send + Sync + 'static) -> Self {
        Self {
            name: name.into(),
            filter: Box::new(filter),
        }
    }
}

impl DataProcessor for FilterProcessor {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn kind(&self) -> ProcessorKind {
        ProcessorKind::Filter
    }
    
    fn process(&self, data: PipelineData) -> Result<PipelineData, PipelineError> {
        if (self.filter)(&data) {
            Ok(data)
        } else {
            // 条件に合わない場合は空データを返す
            Ok(PipelineData::Empty)
        }
    }
}

/// マッパープロセッサ
pub struct MapperProcessor {
    /// プロセッサ名
    name: String,
    /// マッピング関数
    mapper: Box<dyn Fn(PipelineData) -> Result<PipelineData, PipelineError> + Send + Sync>,
}

impl MapperProcessor {
    /// 新しいマッパープロセッサを作成
    pub fn new(
        name: impl Into<String>,
        mapper: impl Fn(PipelineData) -> Result<PipelineData, PipelineError> + Send + Sync + 'static
    ) -> Self {
        Self {
            name: name.into(),
            mapper: Box::new(mapper),
        }
    }
}

impl DataProcessor for MapperProcessor {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn kind(&self) -> ProcessorKind {
        ProcessorKind::Mapper
    }
    
    fn process(&self, data: PipelineData) -> Result<PipelineData, PipelineError> {
        (self.mapper)(data)
    }
}

/// リデューサープロセッサ
pub struct ReducerProcessor {
    /// プロセッサ名
    name: String,
    /// 初期値
    initial_value: PipelineData,
    /// リデューサー関数
    reducer: Box<dyn Fn(PipelineData, PipelineData) -> Result<PipelineData, PipelineError> + Send + Sync>,
}

impl ReducerProcessor {
    /// 新しいリデューサープロセッサを作成
    pub fn new(
        name: impl Into<String>,
        initial_value: PipelineData,
        reducer: impl Fn(PipelineData, PipelineData) -> Result<PipelineData, PipelineError> + Send + Sync + 'static
    ) -> Self {
        Self {
            name: name.into(),
            initial_value,
            reducer: Box::new(reducer),
        }
    }
}

impl DataProcessor for ReducerProcessor {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn kind(&self) -> ProcessorKind {
        ProcessorKind::Reducer
    }
    
    fn process(&self, data: PipelineData) -> Result<PipelineData, PipelineError> {
        (self.reducer)(self.initial_value.clone(), data)
    }
    
    fn process_batch(&self, batch: Vec<PipelineData>) -> Result<Vec<PipelineData>, PipelineError> {
        if batch.is_empty() {
            return Ok(vec![self.initial_value.clone()]);
        }
        
        let mut result = self.initial_value.clone();
        
        for data in batch {
            result = (self.reducer)(result, data)?;
        }
        
        Ok(vec![result])
    }
}

/// プロセッサチェイン（複数プロセッサを連結）
pub struct ProcessorChain {
    /// チェイン名
    name: String,
    /// プロセッサリスト
    processors: Vec<Box<dyn DataProcessor>>,
}

impl ProcessorChain {
    /// 新しいプロセッサチェインを作成
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            processors: Vec::new(),
        }
    }
    
    /// プロセッサを追加
    pub fn add(&mut self, processor: impl DataProcessor + 'static) -> &mut Self {
        self.processors.push(Box::new(processor));
        self
    }
}

impl DataProcessor for ProcessorChain {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn kind(&self) -> ProcessorKind {
        // 最後のプロセッサの種類を返す（または最も重要なプロセッサの種類）
        self.processors.last().map_or(ProcessorKind::Mapper, |p| p.kind())
    }
    
    fn process(&self, data: PipelineData) -> Result<PipelineData, PipelineError> {
        let mut current_data = data;
        
        for processor in &self.processors {
            current_data = processor.process(current_data)?;
            
            // 空データが返された場合、そこで処理を停止
            if matches!(current_data, PipelineData::Empty) {
                break;
            }
        }
        
        Ok(current_data)
    }
    
    async fn process_async(&self, data: PipelineData) -> Result<PipelineData, PipelineError> {
        let mut current_data = data;
        
        for processor in &self.processors {
            current_data = processor.process_async(current_data).await?;
            
            // 空データが返された場合、そこで処理を停止
            if matches!(current_data, PipelineData::Empty) {
                break;
            }
        }
        
        Ok(current_data)
    }
}

/// プロセッサマネージャー
pub struct ProcessorManager {
    /// 登録済みプロセッサ
    processors: HashMap<String, Box<dyn DataProcessor>>,
}

impl ProcessorManager {
    /// 新しいプロセッサマネージャーを作成
    pub fn new() -> Self {
        Self {
            processors: HashMap::new(),
        }
    }
    
    /// プロセッサを登録
    pub fn register(&mut self, processor: impl DataProcessor + 'static) -> Result<(), PipelineError> {
        let name = processor.name().to_string();
        
        if self.processors.contains_key(&name) {
            return Err(PipelineError::ComponentError {
                component_type: "Processor".to_string(),
                component_name: name,
                message: "同名のプロセッサが既に登録されています".to_string(),
            });
        }
        
        self.processors.insert(name, Box::new(processor));
        Ok(())
    }
    
    /// プロセッサを取得
    pub fn get(&self, name: &str) -> Option<&dyn DataProcessor> {
        self.processors.get(name).map(|boxed| boxed.as_ref())
    }
    
    /// 登録済みプロセッサの名前リストを取得
    pub fn list_processors(&self) -> Vec<String> {
        self.processors.keys().cloned().collect()
    }
    
    /// データを処理
    pub fn process(&self, name: &str, data: PipelineData) -> Result<PipelineData, PipelineError> {
        match self.get(name) {
            Some(processor) => processor.process(data),
            None => Err(PipelineError::ComponentError {
                component_type: "Processor".to_string(),
                component_name: name.to_string(),
                message: "指定されたプロセッサが見つかりません".to_string(),
            }),
        }
    }
    
    /// データを非同期処理
    pub async fn process_async(&self, name: &str, data: PipelineData) -> Result<PipelineData, PipelineError> {
        match self.get(name) {
            Some(processor) => processor.process_async(data).await,
            None => Err(PipelineError::ComponentError {
                component_type: "Processor".to_string(),
                component_name: name.to_string(),
                message: "指定されたプロセッサが見つかりません".to_string(),
            }),
        }
    }
}

impl Default for ProcessorManager {
    fn default() -> Self {
        Self::new()
    }
} 