use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status, Result as TonicResult};
use tracing::{info, error};

use crate::daemon::NexusDaemon;
use crate::generated::*;
use crate::generated::container_service_server::ContainerService;
use crate::generated::image_service_server::ImageService;
use crate::generated::volume_service_server::VolumeService;
use crate::generated::network_service_server::NetworkService;
use crate::generated::system_service_server::SystemService;

#[derive(Debug)]
pub struct GrpcServer {
    #[allow(dead_code)]
    daemon: Arc<RwLock<NexusDaemon>>,
    addr: std::net::SocketAddr,
}

impl GrpcServer {
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>, addr: std::net::SocketAddr) -> Self {
        Self { daemon, addr }
    }

    pub async fn serve(&self) -> anyhow::Result<()> {
        // gRPCサーバーは一時的に無効化（protobufコンパイラが必要）
        tracing::info!("gRPC server temporarily disabled (protobuf compiler required)");
        tracing::info!("gRPC server would listen on {}", self.addr);
        
        // 将来的にはここでgRPCサーバーを起動
        // let container_service = ContainerServiceImpl::new(self.daemon.clone());
        // let image_service = ImageServiceImpl::new(self.daemon.clone());
        // let volume_service = VolumeServiceImpl::new(self.daemon.clone());
        // let network_service = NetworkServiceImpl::new(self.daemon.clone());
        // let system_service = SystemServiceImpl::new(self.daemon.clone());
        
        // Server::builder()
        //     .add_service(ContainerServiceServer::new(container_service))
        //     .add_service(ImageServiceServer::new(image_service))
        //     .add_service(VolumeServiceServer::new(volume_service))
        //     .add_service(NetworkServiceServer::new(network_service))
        //     .add_service(SystemServiceServer::new(system_service))
        //     .serve(self.addr)
        //     .await?;
        
        Ok(())
    }
}

// コンテナサービス実装
#[derive(Debug)]
pub struct ContainerServiceImpl {
    #[allow(dead_code)]
    daemon: Arc<RwLock<NexusDaemon>>,
}

impl ContainerServiceImpl {
    #[allow(dead_code)]
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>) -> Self {
        Self { daemon }
    }
}

#[tonic::async_trait]
impl ContainerService for ContainerServiceImpl {
    type ContainerLogsStream = tokio_stream::wrappers::ReceiverStream<Result<ContainerLogsResponse, Status>>;

    async fn create_container(
        &self,
        request: Request<CreateContainerRequest>,
    ) -> TonicResult<Response<CreateContainerResponse>> {
        let req = request.into_inner();
        info!("Creating container: {}", req.name);
        
        // 模擬的なコンテナ作成
        let container_id = format!("container_{}", &uuid::Uuid::new_v4().to_string()[..8]);
        
        Ok(Response::new(CreateContainerResponse {
            id: container_id,
            warnings: vec![],
        }))
    }

    async fn start_container(
        &self,
        request: Request<StartContainerRequest>,
    ) -> TonicResult<Response<StartContainerResponse>> {
        let req = request.into_inner();
        info!("Starting container: {}", req.id);
        
        Ok(Response::new(StartContainerResponse {}))
    }

    async fn stop_container(
        &self,
        request: Request<StopContainerRequest>,
    ) -> TonicResult<Response<StopContainerResponse>> {
        let req = request.into_inner();
        info!("Stopping container: {}", req.id);
        
        Ok(Response::new(StopContainerResponse {}))
    }

    async fn restart_container(
        &self,
        request: Request<RestartContainerRequest>,
    ) -> TonicResult<Response<RestartContainerResponse>> {
        let req = request.into_inner();
        info!("Restarting container: {}", req.id);
        
        Ok(Response::new(RestartContainerResponse {}))
    }

    async fn remove_container(
        &self,
        request: Request<RemoveContainerRequest>,
    ) -> TonicResult<Response<RemoveContainerResponse>> {
        let req = request.into_inner();
        info!("Removing container: {}", req.id);
        
        Ok(Response::new(RemoveContainerResponse {}))
    }

    async fn list_containers(
        &self,
        request: Request<ListContainersRequest>,
    ) -> TonicResult<Response<ListContainersResponse>> {
        let req = request.into_inner();
        error!("Listing containers with filters: {:?}", req.filters);
        
        Ok(Response::new(ListContainersResponse {
            containers: vec![],
        }))
    }

    async fn inspect_container(
        &self,
        request: Request<InspectContainerRequest>,
    ) -> TonicResult<Response<InspectContainerResponse>> {
        let req = request.into_inner();
        info!("Inspecting container: {}", req.id);
        
        Ok(Response::new(InspectContainerResponse {
            container: None,
        }))
    }

    async fn container_logs(
        &self,
        request: Request<ContainerLogsRequest>,
    ) -> TonicResult<Response<Self::ContainerLogsStream>> {
        let req = request.into_inner();
        let daemon = self.daemon.read().await;

        let (tx, rx) = tokio::sync::mpsc::channel(128);

        let container_id = req.container_id.clone();
        let container_manager = daemon.container_manager.clone();

        tokio::spawn(async move {
            match container_manager.get_container_logs(&container_id, req.follow, Some(100)).await {
                Ok(logs) => {
                    for line in logs {
                        let response = ContainerLogsResponse {
                            data: line.into_bytes(),
                            stream_type: 1, // stdout
                        };
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(format!("Failed to get logs: {}", e)))).await;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn exec_in_container(
        &self,
        request: Request<ExecInContainerRequest>,
    ) -> TonicResult<Response<ExecInContainerResponse>> {
        let req = request.into_inner();
        let daemon = self.daemon.read().await;

        let env: Vec<String> = req.env.into_iter().map(|(k, v)| format!("{}={}", k, v)).collect();

        match daemon.container_manager.exec_in_container(&req.container_id, req.cmd, env, req.workdir).await {
            Ok(exec_id) => Ok(Response::new(ExecInContainerResponse { exec_id })),
            Err(e) => Err(Status::internal(format!("Failed to exec in container: {}", e))),
        }
    }

    async fn pause_container(
        &self,
        request: Request<PauseContainerRequest>,
    ) -> TonicResult<Response<PauseContainerResponse>> {
        let req = request.into_inner();
        let daemon = self.daemon.read().await;

        match daemon.container_manager.pause_container(&req.container_id).await {
            Ok(_) => Ok(Response::new(PauseContainerResponse {})),
            Err(e) => Err(Status::internal(format!("Failed to pause container: {}", e))),
        }
    }

    async fn unpause_container(
        &self,
        request: Request<UnpauseContainerRequest>,
    ) -> TonicResult<Response<UnpauseContainerResponse>> {
        let req = request.into_inner();
        let daemon = self.daemon.read().await;

        match daemon.container_manager.unpause_container(&req.container_id).await {
            Ok(_) => Ok(Response::new(UnpauseContainerResponse {})),
            Err(e) => Err(Status::internal(format!("Failed to unpause container: {}", e))),
        }
    }

    async fn update_container(
        &self,
        _request: Request<UpdateContainerRequest>,
    ) -> TonicResult<Response<UpdateContainerResponse>> {
        // TODO: 実装
        Ok(Response::new(UpdateContainerResponse {
            warnings: vec!["Update not implemented yet".to_string()],
        }))
    }

    async fn get_container_stats(
        &self,
        request: Request<GetContainerStatsRequest>,
    ) -> TonicResult<Response<GetContainerStatsResponse>> {
        let req = request.into_inner();
        let daemon = self.daemon.read().await;

        match daemon.container_manager.get_container_stats(&req.container_id).await {
            Ok(_stats_map) => {
                let stats = ContainerStats {
                    read: chrono::Utc::now().to_rfc3339(),
                    preread: String::new(),
                    pids_stats: None,
                    blkio_stats: None,
                    num_procs: 0,
                    storage_stats: None,
                    cpu_stats: None,
                    precpu_stats: None,
                    memory_stats: None,
                    name: format!("/{}", req.container_id),
                    id: req.container_id,
                    networks: HashMap::new(),
                };

                Ok(Response::new(GetContainerStatsResponse {
                    stats: Some(stats),
                }))
            }
            Err(e) => Err(Status::internal(format!("Failed to get container stats: {}", e))),
        }
    }
}

// イメージサービス実装
#[derive(Debug)]
pub struct ImageServiceImpl {
    #[allow(dead_code)]
    daemon: Arc<RwLock<NexusDaemon>>,
}

impl ImageServiceImpl {
    #[allow(dead_code)]
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>) -> Self {
        Self { daemon }
    }
}

#[tonic::async_trait]
impl ImageService for ImageServiceImpl {
    async fn list_images(
        &self,
        _request: Request<ListImagesRequest>,
    ) -> TonicResult<Response<ListImagesResponse>> {
        Ok(Response::new(ListImagesResponse {
            images: vec![],
        }))
    }

    async fn pull_image(
        &self,
        _request: Request<PullImageRequest>,
    ) -> TonicResult<Response<PullImageResponse>> {
        Ok(Response::new(PullImageResponse {
            status: "Pull not implemented yet".to_string(),
            progress: String::new(),
            progress_detail: None,
            id: String::new(),
        }))
    }

    async fn push_image(
        &self,
        _request: Request<PushImageRequest>,
    ) -> TonicResult<Response<PushImageResponse>> {
        Ok(Response::new(PushImageResponse {
            status: "Push not implemented yet".to_string(),
            progress: String::new(),
            progress_detail: None,
            id: String::new(),
        }))
    }

    async fn remove_image(
        &self,
        _request: Request<RemoveImageRequest>,
    ) -> TonicResult<Response<RemoveImageResponse>> {
        Ok(Response::new(RemoveImageResponse {
            deleted: vec![],
        }))
    }

    async fn inspect_image(
        &self,
        _request: Request<InspectImageRequest>,
    ) -> TonicResult<Response<InspectImageResponse>> {
        Err(Status::not_found("Image not found"))
    }
}

// ボリュームサービス実装
#[derive(Debug)]
pub struct VolumeServiceImpl {
    #[allow(dead_code)]
    daemon: Arc<RwLock<NexusDaemon>>,
}

impl VolumeServiceImpl {
    #[allow(dead_code)]
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>) -> Self {
        Self { daemon }
    }
}

#[tonic::async_trait]
impl VolumeService for VolumeServiceImpl {
    async fn create_volume(
        &self,
        _request: Request<CreateVolumeRequest>,
    ) -> TonicResult<Response<CreateVolumeResponse>> {
        Ok(Response::new(CreateVolumeResponse {
            volume: None,
        }))
    }

    async fn list_volumes(
        &self,
        _request: Request<ListVolumesRequest>,
    ) -> TonicResult<Response<ListVolumesResponse>> {
        Ok(Response::new(ListVolumesResponse {
            volumes: vec![],
            warnings: vec![],
        }))
    }

    async fn remove_volume(
        &self,
        _request: Request<RemoveVolumeRequest>,
    ) -> TonicResult<Response<RemoveVolumeResponse>> {
        Ok(Response::new(RemoveVolumeResponse {}))
    }

    async fn inspect_volume(
        &self,
        _request: Request<InspectVolumeRequest>,
    ) -> TonicResult<Response<InspectVolumeResponse>> {
        Err(Status::not_found("Volume not found"))
    }
}

// ネットワークサービス実装
#[derive(Debug)]
pub struct NetworkServiceImpl {
    #[allow(dead_code)]
    daemon: Arc<RwLock<NexusDaemon>>,
}

impl NetworkServiceImpl {
    #[allow(dead_code)]
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>) -> Self {
        Self { daemon }
    }
}

#[tonic::async_trait]
impl NetworkService for NetworkServiceImpl {
    async fn create_network(
        &self,
        _request: Request<CreateNetworkRequest>,
    ) -> TonicResult<Response<CreateNetworkResponse>> {
        Ok(Response::new(CreateNetworkResponse {
            id: "network_123".to_string(),
            warning: String::new(),
        }))
    }

    async fn list_networks(
        &self,
        _request: Request<ListNetworksRequest>,
    ) -> TonicResult<Response<ListNetworksResponse>> {
        Ok(Response::new(ListNetworksResponse {
            networks: vec![],
        }))
    }

    async fn remove_network(
        &self,
        _request: Request<RemoveNetworkRequest>,
    ) -> TonicResult<Response<RemoveNetworkResponse>> {
        Ok(Response::new(RemoveNetworkResponse {}))
    }

    async fn inspect_network(
        &self,
        _request: Request<InspectNetworkRequest>,
    ) -> TonicResult<Response<InspectNetworkResponse>> {
        Err(Status::not_found("Network not found"))
    }

    async fn connect_container(
        &self,
        _request: Request<ConnectContainerRequest>,
    ) -> TonicResult<Response<ConnectContainerResponse>> {
        Ok(Response::new(ConnectContainerResponse {}))
    }

    async fn disconnect_container(
        &self,
        _request: Request<DisconnectContainerRequest>,
    ) -> TonicResult<Response<DisconnectContainerResponse>> {
        Ok(Response::new(DisconnectContainerResponse {}))
    }
}

// システムサービス実装
#[derive(Debug)]
pub struct SystemServiceImpl {
    #[allow(dead_code)]
    daemon: Arc<RwLock<NexusDaemon>>,
}

impl SystemServiceImpl {
    #[allow(dead_code)]
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>) -> Self {
        Self { daemon }
    }
}

#[tonic::async_trait]
impl SystemService for SystemServiceImpl {
    type GetEventsStream = tokio_stream::wrappers::ReceiverStream<Result<GetEventsResponse, Status>>;

    async fn get_version(
        &self,
        _request: Request<GetVersionRequest>,
    ) -> TonicResult<Response<GetVersionResponse>> {
        Ok(Response::new(GetVersionResponse {
            version: "0.1.0".to_string(),
            api_version: "1.41".to_string(),
            min_api_version: "1.12".to_string(),
            git_commit: "unknown".to_string(),
            go_version: "N/A".to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            kernel_version: "unknown".to_string(),
            build_time: "unknown".to_string(),
            experimental: false,
        }))
    }

    async fn get_info(
        &self,
        _request: Request<GetInfoRequest>,
    ) -> TonicResult<Response<GetInfoResponse>> {
        Ok(Response::new(GetInfoResponse {
            info: Some(SystemInfo {
                id: "nexus-daemon".to_string(),
                containers: 0,
                containers_running: 0,
                containers_paused: 0,
                containers_stopped: 0,
                images: 0,
                driver: "nexus".to_string(),
                driver_status: vec![],
                docker_root_dir: "/var/lib/nexusd".to_string(),
                system_status: vec![],
                plugins: Plugins {
                    volume: vec!["local".to_string()],
                    network: vec!["bridge".to_string(), "host".to_string()],
                    authorization: vec![],
                    log: vec!["json-file".to_string()],
                },
                memory_limit: true,
                swap_limit: true,
                kernel_memory: true,
                cpu_cfs_period: true,
                cpu_cfs_quota: true,
                cpu_shares: true,
                cpu_set: true,
                pids_limit: true,
                ipv4_forwarding: true,
                bridge_nf_iptables: true,
                bridge_nf_ip6tables: true,
                debug: false,
                nfd: 0,
                oom_kill_disable: false,
                n_goroutines: 0,
                system_time: chrono::Utc::now().to_rfc3339(),
                logging_driver: "json-file".to_string(),
                cgroup_driver: "systemd".to_string(),
                n_events_listener: 0,
                kernel_version: "unknown".to_string(),
                operating_system: "Linux".to_string(),
                os_type: "linux".to_string(),
                architecture: std::env::consts::ARCH.to_string(),
                index_server_address: "https://index.docker.io/v1/".to_string(),
                registry_config: RegistryServiceConfig {
                    allow_nondistributable_artifacts_cidrs: vec![],
                    allow_nondistributable_artifacts_hostnames: vec![],
                    insecure_registry_cidrs: vec![],
                    index_configs: HashMap::new(),
                    mirrors: vec![],
                },
                ncpu: num_cpus::get() as i32,
                mem_total: 0,
                generic_resources: vec![],
                docker_version: "N/A".to_string(),
                http_proxy: String::new(),
                https_proxy: String::new(),
                no_proxy: String::new(),
                name: "nexus-daemon".to_string(),
                labels: vec![],
                experimental_build: false,
                server_version: "0.1.0".to_string(),
                cluster_store: String::new(),
                cluster_advertise: String::new(),
                runtimes: HashMap::new(),
                default_runtime: "nexus".to_string(),
                swarm: SwarmInfo {
                    node_id: String::new(),
                    node_addr: String::new(),
                    local_node_state: "inactive".to_string(),
                    control_available: false,
                    error: String::new(),
                    remote_managers: vec![],
                    nodes: 0,
                    managers: 0,
                    cluster: ClusterInfo {
                        id: String::new(),
                        version: ObjectVersion { index: 0 },
                        created_at: String::new(),
                        updated_at: String::new(),
                        spec: SwarmSpec {
                            name: String::new(),
                            labels: HashMap::new(),
                            orchestration: OrchestrationConfig {
                                task_history_retention_limit: 0,
                            },
                            raft: RaftConfig {
                                snapshot_interval: 0,
                                keep_old_snapshots: 0,
                                log_entries_for_slow_followers: 0,
                                election_tick: 0,
                                heartbeat_tick: 0,
                            },
                            dispatcher: DispatcherConfig {
                                heartbeat_period: 0,
                            },
                            ca_config: CaConfig {
                                node_cert_expiry: 0,
                                external_cas: vec![],
                                signing_ca_cert: String::new(),
                                signing_ca_key: String::new(),
                                force_rotate: 0,
                            },
                            encryption_config: EncryptionConfig {
                                auto_lock_managers: false,
                            },
                            task_defaults: TaskDefaults {
                                log_driver: None,
                            },
                        },
                        tls_info: TlsInfo {
                            trust_root: String::new(),
                            cert_issuer_subject: String::new(),
                            cert_issuer_public_key: String::new(),
                        },
                        root_rotation_in_progress: false,
                        default_addr_pool: vec![],
                        subnet_size: 0,
                        data_path_port: 0,
                    },
                },
                live_restore_enabled: false,
                isolation: String::new(),
                init_binary: String::new(),
                containerd_commit: Commit {
                    id: String::new(),
                    expected: String::new(),
                },
                runc_commit: Commit {
                    id: String::new(),
                    expected: String::new(),
                },
                init_commit: Commit {
                    id: String::new(),
                    expected: String::new(),
                },
                security_options: vec![],
                product_license: String::new(),
                warnings: vec![],
            }),
        }))
    }

    async fn get_events(
        &self,
        _request: Request<GetEventsRequest>,
    ) -> TonicResult<Response<Self::GetEventsStream>> {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        
        tokio::spawn(async move {
            // 模擬イベントストリーム
            let _ = tx.send(Ok(GetEventsResponse {
                event: Some(SystemEvent {
                    event_type: "container".to_string(),
                    action: "start".to_string(),
                    actor: EventActor {
                        id: "test".to_string(),
                        attributes: HashMap::new(),
                    },
                    time: chrono::Utc::now().timestamp(),
                    time_nano: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                }),
            })).await;
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn ping(
        &self,
        _request: Request<PingRequest>,
    ) -> TonicResult<Response<PingResponse>> {
        Ok(Response::new(PingResponse {
            api_version: "1.41".to_string(),
            docker_experimental: false,
            os_type: "linux".to_string(),
        }))
    }

    async fn get_disk_usage(
        &self,
        _request: Request<GetDiskUsageRequest>,
    ) -> TonicResult<Response<GetDiskUsageResponse>> {
        Ok(Response::new(GetDiskUsageResponse {
            disk_usage: Some(DiskUsage {
                layers_size: 0,
                images: vec![],
                containers: vec![],
                volumes: vec![],
                build_cache: vec![],
            }),
        }))
    }

    async fn prune_system(
        &self,
        _request: Request<PruneSystemRequest>,
    ) -> TonicResult<Response<PruneSystemResponse>> {
        Ok(Response::new(PruneSystemResponse {
            containers_deleted: vec![],
            space_reclaimed: 0,
        }))
    }
} 