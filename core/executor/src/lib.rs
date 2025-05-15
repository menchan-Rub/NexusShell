// NexusShell エグゼキュータモジュール
// コマンドの実行管理を担当します

pub mod pipeline_manager;
pub mod job_controller;
pub mod async_runtime;
pub mod sandbox;
pub mod remote_executor;

use std::sync::Arc;

/// エグゼキュータのインターフェース
pub struct Executor {
    /// パイプラインマネージャー
    pipeline_manager: Arc<pipeline_manager::PipelineManager>,
    /// ジョブコントローラー
    job_controller: Arc<job_controller::JobController>,
    /// 非同期ランタイム
    async_runtime: Arc<async_runtime::AsyncRuntime>,
    /// サンドボックス
    sandbox: Arc<sandbox::Sandbox>,
    /// リモートエグゼキュータ
    remote_executor: Arc<remote_executor::RemoteExecutor>,
}

impl Executor {
    /// 新しいエグゼキュータを作成します
    pub fn new() -> Self {
        Self {
            pipeline_manager: Arc::new(pipeline_manager::PipelineManager::new()),
            job_controller: Arc::new(job_controller::JobController::new()),
            async_runtime: Arc::new(async_runtime::AsyncRuntime::new()),
            sandbox: Arc::new(sandbox::Sandbox::new()),
            remote_executor: Arc::new(remote_executor::RemoteExecutor::new()),
        }
    }

    /// パイプラインマネージャーを取得します
    pub fn pipeline_manager(&self) -> Arc<pipeline_manager::PipelineManager> {
        self.pipeline_manager.clone()
    }

    /// ジョブコントローラーを取得します
    pub fn job_controller(&self) -> Arc<job_controller::JobController> {
        self.job_controller.clone()
    }

    /// 非同期ランタイムを取得します
    pub fn async_runtime(&self) -> Arc<async_runtime::AsyncRuntime> {
        self.async_runtime.clone()
    }

    /// サンドボックスを取得します
    pub fn sandbox(&self) -> Arc<sandbox::Sandbox> {
        self.sandbox.clone()
    }

    /// リモートエグゼキュータを取得します
    pub fn remote_executor(&self) -> Arc<remote_executor::RemoteExecutor> {
        self.remote_executor.clone()
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
} 