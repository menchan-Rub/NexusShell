use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::daemon::NexusDaemon;
use crate::container_manager::{ContainerConfig};

#[derive(Debug)]
pub struct HttpServer {
    daemon: Arc<RwLock<NexusDaemon>>,
    addr: std::net::SocketAddr,
}

impl HttpServer {
    pub fn new(daemon: Arc<RwLock<NexusDaemon>>, addr: std::net::SocketAddr) -> Self {
        Self { daemon, addr }
    }

    pub async fn serve(&self) -> anyhow::Result<()> {
        info!("Starting HTTP server on {}", self.addr);

        let app = Router::new()
            // システムAPI
            .route("/version", get(get_version))
            .route("/info", get(get_info))
            .route("/ping", get(ping))
            .route("/_ping", get(ping))
            .route("/events", get(get_events))
            .route("/system/df", get(get_disk_usage))
            .route("/system/prune", post(prune_system))
            
            // コンテナAPI
            .route("/containers/json", get(list_containers))
            .route("/containers/create", post(create_container))
            .route("/containers/:id/start", post(start_container))
            .route("/containers/:id/stop", post(stop_container))
            .route("/containers/:id/restart", post(restart_container))
            .route("/containers/:id/kill", post(kill_container))
            .route("/containers/:id/pause", post(pause_container))
            .route("/containers/:id/unpause", post(unpause_container))
            .route("/containers/:id/remove", delete(remove_container))
            .route("/containers/:id/json", get(inspect_container))
            .route("/containers/:id/logs", get(get_container_logs))
            .route("/containers/:id/stats", get(get_container_stats))
            .route("/containers/:id/exec", post(exec_container))
            .route("/containers/:id/update", post(update_container))
            
            // イメージAPI
            .route("/images/json", get(list_images))
            .route("/images/create", post(pull_image))
            .route("/images/:name/push", post(push_image))
            .route("/images/:name/json", get(inspect_image))
            .route("/images/:name", delete(remove_image))
            .route("/images/:name/tag", post(tag_image))
            .route("/build", post(build_image))
            
            // ボリュームAPI
            .route("/volumes", get(list_volumes))
            .route("/volumes/create", post(create_volume))
            .route("/volumes/:name", get(inspect_volume))
            .route("/volumes/:name", delete(remove_volume))
            .route("/volumes/prune", post(prune_volumes))
            
            // ネットワークAPI
            .route("/networks", get(list_networks))
            .route("/networks/create", post(create_network))
            .route("/networks/:id", get(inspect_network))
            .route("/networks/:id", delete(remove_network))
            .route("/networks/:id/connect", post(connect_network))
            .route("/networks/:id/disconnect", post(disconnect_network))
            
            .with_state(self.daemon.clone());

        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

// リクエスト/レスポンス型定義
#[derive(Deserialize)]
struct CreateContainerRequest {
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "Cmd")]
    cmd: Option<Vec<String>>,
    #[serde(rename = "Env")]
    env: Option<Vec<String>>,
    #[serde(rename = "WorkingDir")]
    working_dir: Option<String>,
    #[serde(rename = "User")]
    user: Option<String>,
    #[serde(rename = "Hostname")]
    hostname: Option<String>,
    #[serde(rename = "Entrypoint")]
    entrypoint: Option<Vec<String>>,
    #[serde(rename = "ExposedPorts")]
    exposed_ports: Option<HashMap<String, Value>>,
    #[serde(rename = "Volumes")]
    _volumes: Option<HashMap<String, Value>>,
    #[serde(rename = "HostConfig")]
    _host_config: Option<Value>,
    #[serde(rename = "NetworkingConfig")]
    _networking_config: Option<Value>,
}

#[derive(Serialize)]
struct CreateContainerResponse {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Warnings")]
    warnings: Vec<String>,
}

#[derive(Deserialize)]
struct ContainerQuery {
    all: Option<bool>,
    _limit: Option<i32>,
    size: Option<bool>,
    _filters: Option<String>,
}

#[derive(Deserialize)]
struct StopQuery {
    t: Option<u64>, // timeout
}

#[derive(Deserialize)]
struct RemoveQuery {
    force: Option<bool>,
    v: Option<bool>, // remove volumes
}

#[derive(Deserialize)]
struct LogsQuery {
    follow: Option<bool>,
    _stdout: Option<bool>,
    _stderr: Option<bool>,
    _since: Option<i64>,
    _until: Option<i64>,
    _timestamps: Option<bool>,
    tail: Option<String>,
}

// システムAPI実装
async fn get_version() -> Json<Value> {
    Json(serde_json::json!({
        "Version": env!("CARGO_PKG_VERSION"),
        "ApiVersion": "1.0",
        "MinAPIVersion": "1.0",
        "GitCommit": "unknown",
        "GoVersion": "N/A",
        "Os": std::env::consts::OS,
        "Arch": std::env::consts::ARCH,
        "KernelVersion": "unknown",
        "BuildTime": "unknown",
        "Experimental": false
    }))
}

async fn get_info(State(daemon): State<Arc<RwLock<NexusDaemon>>>) -> Json<Value> {
    let _daemon = daemon.read().await;
    
    Json(serde_json::json!({
        "ID": "nexus-daemon",
        "Containers": 0,
        "ContainersRunning": 0,
        "ContainersPaused": 0,
        "ContainersStopped": 0,
        "Images": 0,
        "Driver": "nexus",
        "DriverStatus": [],
        "SystemStatus": [],
        "Plugins": {
            "Volume": [],
            "Network": [],
            "Authorization": [],
            "Log": []
        },
        "MemoryLimit": true,
        "SwapLimit": true,
        "KernelMemory": true,
        "CpuCfsPeriod": true,
        "CpuCfsQuota": true,
        "CPUShares": true,
        "CPUSet": true,
        "PidsLimit": true,
        "IPv4Forwarding": true,
        "BridgeNfIptables": true,
        "BridgeNfIp6tables": true,
        "Debug": false,
        "NFd": 0,
        "OomKillDisable": true,
        "NGoroutines": 0,
        "SystemTime": chrono::Utc::now().to_rfc3339(),
        "LoggingDriver": "json-file",
        "CgroupDriver": "systemd",
        "CgroupVersion": "2",
        "NEventsListener": 0,
        "KernelVersion": "unknown",
        "OperatingSystem": std::env::consts::OS,
        "OSVersion": "unknown",
        "OSType": std::env::consts::OS,
        "Architecture": std::env::consts::ARCH,
        "NCPU": num_cpus::get(),
        "MemTotal": 0,
        "IndexServerAddress": "",
        "RegistryConfig": {},
        "GenericResources": [],
        "HttpProxy": "",
        "HttpsProxy": "",
        "NoProxy": "",
        "Name": "nexus-daemon",
        "Labels": [],
        "ExperimentalBuild": false,
        "ServerVersion": env!("CARGO_PKG_VERSION"),
        "Runtimes": {
            "nexus": {
                "path": "nexus-runtime"
            }
        },
        "DefaultRuntime": "nexus",
        "Swarm": {
            "NodeID": "",
            "NodeAddr": "",
            "LocalNodeState": "inactive",
            "ControlAvailable": false,
            "Error": "",
            "RemoteManagers": []
        },
        "LiveRestoreEnabled": false,
        "Isolation": "",
        "InitBinary": "",
        "ContainerdCommit": {},
        "RuncCommit": {},
        "InitCommit": {},
        "SecurityOptions": [],
        "ProductLicense": "Apache-2.0",
        "DefaultAddressPools": [],
        "Warnings": []
    }))
}

async fn ping() -> &'static str {
    "OK"
}

async fn get_events() -> &'static str {
    // TODO: Server-Sent Events実装
    "Events not implemented yet"
}

async fn get_disk_usage() -> Json<Value> {
    Json(serde_json::json!({
        "LayersSize": 0,
        "Images": [],
        "Containers": [],
        "Volumes": [],
        "BuildCache": []
    }))
}

async fn prune_system() -> Json<Value> {
    Json(serde_json::json!({
        "ContainersDeleted": [],
        "SpaceReclaimed": 0,
        "VolumesDeleted": [],
        "ImagesDeleted": []
    }))
}

// コンテナAPI実装
async fn list_containers(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Query(params): Query<ContainerQuery>,
) -> Result<Json<Vec<Value>>, StatusCode> {
    let daemon = daemon.read().await;
    
    let all = params.all.unwrap_or(false);
    let size = params.size.unwrap_or(false);
    let filters = HashMap::new(); // TODO: フィルター解析
    
    match daemon.container_manager.list_containers(all, size, filters).await {
        Ok(containers) => {
            let response: Vec<Value> = containers
                .into_iter()
                .map(|c| serde_json::json!({
                    "Id": c.id,
                    "Names": [format!("/{}", c.name)],
                    "Image": c.image,
                    "ImageID": "",
                    "Command": c.config.cmd.join(" "),
                    "Created": c.created.timestamp(),
                    "State": c.state,
                    "Status": format!("Up"),
                    "Ports": [],
                    "Labels": c.labels,
                    "SizeRw": 0,
                    "SizeRootFs": 0,
                    "HostConfig": {
                        "NetworkMode": "default"
                    },
                    "NetworkSettings": {
                        "Networks": {}
                    },
                    "Mounts": []
                }))
                .collect();
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn create_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Query(params): Query<HashMap<String, String>>,
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<CreateContainerResponse>, StatusCode> {
    let daemon = daemon.read().await;
    
    let name = params.get("name").cloned().unwrap_or_else(|| {
        format!("container_{}", &uuid::Uuid::new_v4().to_string()[..8])
    });
    
    let config = ContainerConfig {
        env: payload.env.unwrap_or_default(),
        cmd: payload.cmd.unwrap_or_default(),
        working_dir: payload.working_dir,
        user: payload.user,
        hostname: payload.hostname,
        entrypoint: payload.entrypoint.unwrap_or_default(),
        exposed_ports: payload.exposed_ports
            .map(|ports| ports.keys().cloned().collect())
            .unwrap_or_default(),
        volumes: HashMap::new(), // TODO: ボリューム設定
    };
    
    match daemon.container_manager.create_container(&name, &payload.image, Some(config)).await {
        Ok(container_id) => Ok(Json(CreateContainerResponse {
            id: container_id,
            warnings: vec![],
        })),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn start_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let daemon = daemon.read().await;
    
    match daemon.container_manager.start_container(&id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn stop_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
    Query(params): Query<StopQuery>,
) -> Result<StatusCode, StatusCode> {
    let daemon = daemon.read().await;
    let timeout = params.t.unwrap_or(10);
    
    if (daemon.container_manager.stop_container(&id, timeout).await).is_err() {
        log::warn!("Failed to stop container {} gracefully, forcing stop", id);
        let _ = daemon.container_manager.kill_container(&id).await;
    }
    
    Ok(StatusCode::NO_CONTENT)
}

async fn restart_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
    Query(params): Query<StopQuery>,
) -> Result<StatusCode, StatusCode> {
    let daemon = daemon.read().await;
    let timeout = params.t.unwrap_or(10);
    
    // 停止してから開始
    if let Err(_) = daemon.container_manager.stop_container(&id, timeout).await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    match daemon.container_manager.start_container(&id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn kill_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let daemon = daemon.read().await;
    
    // 強制停止（timeout=0）
    match daemon.container_manager.stop_container(&id, 0).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn pause_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let daemon = daemon.read().await;
    
    match daemon.container_manager.pause_container(&id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn unpause_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let daemon = daemon.read().await;
    
    match daemon.container_manager.unpause_container(&id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn remove_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
    Query(params): Query<RemoveQuery>,
) -> Result<StatusCode, StatusCode> {
    let daemon = daemon.read().await;
    let force = params.force.unwrap_or(false);
    let remove_volumes = params.v.unwrap_or(false);
    
    match daemon.container_manager.remove_container(&id, force, remove_volumes).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn inspect_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let daemon = daemon.read().await;
    
    match daemon.container_manager.inspect_container(&id, false).await {
        Ok(metadata) => {
            let response = serde_json::json!({
                "Id": metadata.id,
                "Created": metadata.created.to_rfc3339(),
                "Path": metadata.config.entrypoint.first().cloned().unwrap_or_default(),
                "Args": metadata.config.cmd,
                "State": {
                    "Status": metadata.state,
                    "Running": metadata.state == "running",
                    "Paused": metadata.state == "paused",
                    "Restarting": false,
                    "OOMKilled": false,
                    "Dead": false,
                    "Pid": metadata.pid.unwrap_or(0),
                    "ExitCode": metadata.exit_code.unwrap_or(0),
                    "Error": "",
                    "StartedAt": metadata.created.to_rfc3339(),
                    "FinishedAt": "",
                    "Health": null
                },
                "Image": metadata.image,
                "ResolvConfPath": "",
                "HostnamePath": "",
                "HostsPath": "",
                "LogPath": metadata.log_path.to_string_lossy(),
                "Name": format!("/{}", metadata.name),
                "RestartCount": 0,
                "Driver": "",
                "Platform": "",
                "MountLabel": "",
                "ProcessLabel": "",
                "AppArmorProfile": "",
                "ExecIDs": [],
                "HostConfig": {},
                "GraphDriver": {},
                "SizeRw": 0,
                "SizeRootFs": 0,
                "Mounts": [],
                "Config": {
                    "Hostname": metadata.config.hostname,
                    "Domainname": "",
                    "User": metadata.config.user,
                    "AttachStdin": false,
                    "AttachStdout": true,
                    "AttachStderr": true,
                    "ExposedPorts": {},
                    "Tty": false,
                    "OpenStdin": false,
                    "StdinOnce": false,
                    "Env": metadata.config.env,
                    "Cmd": metadata.config.cmd,
                    "Image": metadata.image,
                    "Volumes": {},
                    "WorkingDir": metadata.config.working_dir,
                    "Entrypoint": metadata.config.entrypoint,
                    "OnBuild": [],
                    "Labels": metadata.labels
                },
                "NetworkSettings": {
                    "Bridge": "",
                    "SandboxID": "",
                    "HairpinMode": false,
                    "LinkLocalIPv6Address": "",
                    "LinkLocalIPv6PrefixLen": 0,
                    "Ports": {},
                    "SandboxKey": "",
                    "SecondaryIPAddresses": [],
                    "SecondaryIPv6Addresses": [],
                    "EndpointID": "",
                    "Gateway": "",
                    "GlobalIPv6Address": "",
                    "GlobalIPv6PrefixLen": 0,
                    "IPAddress": "",
                    "IPPrefixLen": 0,
                    "IPv6Gateway": "",
                    "MacAddress": "",
                    "Networks": {}
                }
            });
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_container_logs(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
    Query(params): Query<LogsQuery>,
) -> Result<String, StatusCode> {
    let daemon = daemon.read().await;
    
    let follow = params.follow.unwrap_or(false);
    let tail = params.tail.and_then(|t| t.parse().ok());
    
    match daemon.container_manager.get_container_logs(&id, follow, tail).await {
        Ok(logs) => Ok(logs.join("\n")),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_container_stats(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let daemon = daemon.read().await;
    
    match daemon.container_manager.get_container_stats(&id).await {
        Ok(_stats) => {
            let response = serde_json::json!({
                "read": chrono::Utc::now().to_rfc3339(),
                "preread": "",
                "pids_stats": {},
                "blkio_stats": {},
                "num_procs": 0,
                "storage_stats": {},
                "cpu_stats": {},
                "precpu_stats": {},
                "memory_stats": {},
                "name": format!("/{}", id),
                "id": id,
                "networks": {}
            });
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn exec_container(
    State(daemon): State<Arc<RwLock<NexusDaemon>>>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let daemon = daemon.read().await;
    
    let cmd = payload["Cmd"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    
    let env = payload["Env"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    
    let workdir = payload["WorkingDir"].as_str().map(|s| s.to_string());
    
    match daemon.container_manager.exec_in_container(&id, cmd, env, workdir).await {
        Ok(exec_id) => Ok(Json(serde_json::json!({
            "Id": exec_id
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn update_container(
    Path(_id): Path<String>,
    Json(_payload): Json<Value>,
) -> Json<Value> {
    Json(serde_json::json!({
        "Warnings": ["Update not implemented yet"]
    }))
}

// イメージAPI実装（スタブ）
async fn list_images() -> Json<Vec<Value>> {
    Json(vec![])
}

async fn pull_image() -> Json<Value> {
    Json(serde_json::json!({
        "status": "Pull not implemented yet"
    }))
}

async fn push_image() -> Json<Value> {
    Json(serde_json::json!({
        "status": "Push not implemented yet"
    }))
}

async fn inspect_image() -> Result<Json<Value>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

async fn remove_image() -> Json<Vec<Value>> {
    Json(vec![])
}

async fn tag_image() -> StatusCode {
    StatusCode::CREATED
}

async fn build_image() -> Json<Value> {
    Json(serde_json::json!({
        "stream": "Build not implemented yet"
    }))
}

// ボリュームAPI実装（スタブ）
async fn list_volumes() -> Json<Value> {
    Json(serde_json::json!({
        "Volumes": [],
        "Warnings": []
    }))
}

async fn create_volume() -> Json<Value> {
    Json(serde_json::json!({}))
}

async fn inspect_volume() -> Result<Json<Value>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

async fn remove_volume() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn prune_volumes() -> Json<Value> {
    Json(serde_json::json!({
        "VolumesDeleted": [],
        "SpaceReclaimed": 0
    }))
}

// ネットワークAPI実装（スタブ）
async fn list_networks() -> Json<Vec<Value>> {
    Json(vec![])
}

async fn create_network() -> Json<Value> {
    Json(serde_json::json!({
        "Id": "network-id",
        "Warning": ""
    }))
}

async fn inspect_network() -> Result<Json<Value>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

async fn remove_network() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn connect_network() -> StatusCode {
    StatusCode::OK
}

async fn disconnect_network() -> StatusCode {
    StatusCode::OK
} 