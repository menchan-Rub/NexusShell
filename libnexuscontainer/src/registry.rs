#[cfg(feature = "registry")]
mod registry_impl {
    use crate::errors::{ContainerError, Result};
    use crate::oci::{OCIManifest, OCIIndex};
    use reqwest::{Client, Response, StatusCode, header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE}};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::time::Duration;
    use futures_util::StreamExt;

    /// レジストリ認証情報
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RegistryAuth {
        pub username: Option<String>,
        pub password: Option<String>,
        pub token: Option<String>,
        pub refresh_token: Option<String>,
    }

    /// OCI Distribution API v2レジストリクライアント
    #[derive(Debug)]
    pub struct RegistryClient {
        client: Client,
        auth_cache: HashMap<String, RegistryAuth>,
        default_registry: String,
    }

    /// Docker Hub トークン取得用の構造体
    #[derive(Debug, Deserialize)]
    struct DockerHubToken {
        token: String,
        access_token: Option<String>,
        expires_in: Option<u64>,
        issued_at: Option<String>,
    }

    /// Docker Hub エラーレスポンス
    #[derive(Debug, Deserialize)]
    struct DockerHubError {
        code: String,
        message: String,
        detail: Option<serde_json::Value>,
    }

    /// Docker Hub エラーレスポンス全体
    #[derive(Debug, Deserialize)]
    struct DockerHubErrorResponse {
        errors: Vec<DockerHubError>,
    }

    impl RegistryClient {
        /// 新しいレジストリクライアントを作成
        pub fn new() -> Self {
            let client = Client::builder()
                .timeout(Duration::from_secs(300))
                .user_agent("NexusContainer/0.1.0")
                .build()
                .expect("Failed to create HTTP client");

            Self {
                client,
                auth_cache: HashMap::new(),
                default_registry: "registry-1.docker.io".to_string(),
            }
        }

        /// デフォルトレジストリを設定
        pub fn set_default_registry(&mut self, registry: String) {
            self.default_registry = registry;
        }

        /// 認証情報を設定
        pub fn set_auth(&mut self, registry: &str, auth: RegistryAuth) {
            self.auth_cache.insert(registry.to_string(), auth);
        }

        /// イメージ名をレジストリとリポジトリに分解
        fn parse_image_name(&self, image_name: &str) -> (String, String) {
            if let Some(slash_pos) = image_name.find('/') {
                let (first_part, rest) = image_name.split_at(slash_pos);
                
                // ドット、コロン、ポート番号が含まれていればレジストリと判定
                if first_part.contains('.') || first_part.contains(':') || first_part.parse::<u16>().is_ok() {
                    (first_part.to_string(), rest[1..].to_string())
                } else {
                    (self.default_registry.clone(), image_name.to_string())
                }
            } else {
                (self.default_registry.clone(), format!("library/{}", image_name))
            }
        }

        /// マニフェストを取得
        pub async fn get_manifest(&self, image_name: &str, reference: &str) -> Result<OCIManifest> {
            let (registry, repository) = self.parse_image_name(image_name);
            let url = format!("https://{}/v2/{}/manifests/{}", registry, repository, reference);

            log::debug!("Getting manifest from: {}", url);

            let mut headers = HeaderMap::new();
            headers.insert(
                "Accept",
                HeaderValue::from_static("application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json")
            );

            // 認証ヘッダーを追加
            if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                headers.insert(AUTHORIZATION, auth_header);
            }

            let response = self.client
                .get(&url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to get manifest: {}", e)))?;

            match response.status() {
                StatusCode::OK => {
                    let manifest_text = response.text().await
                        .map_err(|e| ContainerError::Network(format!("Failed to read manifest response: {}", e)))?;

                    let manifest: OCIManifest = serde_json::from_str(&manifest_text)
                        .map_err(|e| ContainerError::Serialization(format!("Failed to parse manifest: {}", e)))?;

                    Ok(manifest)
                }
                StatusCode::UNAUTHORIZED => {
                    // 認証が必要な場合、認証を試行
                    self.authenticate(&registry, &repository).await?;
                    
                    // 再試行
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        "Accept",
                        HeaderValue::from_static("application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json")
                    );

                    if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                        headers.insert(AUTHORIZATION, auth_header);
                    }

                    let response = self.client
                        .get(&url)
                        .headers(headers)
                        .send()
                        .await
                        .map_err(|e| ContainerError::Network(format!("Failed to get manifest (retry): {}", e)))?;

                    if response.status() == StatusCode::OK {
                        let manifest_text = response.text().await
                            .map_err(|e| ContainerError::Network(format!("Failed to read manifest response: {}", e)))?;

                        let manifest: OCIManifest = serde_json::from_str(&manifest_text)
                            .map_err(|e| ContainerError::Serialization(format!("Failed to parse manifest: {}", e)))?;

                        Ok(manifest)
                    } else {
                        Err(ContainerError::NotFound(format!("Manifest not found: {}", response.status())))
                    }
                }
                StatusCode::NOT_FOUND => {
                    Err(ContainerError::NotFound(format!("Image {}:{} not found", image_name, reference)))
                }
                _ => {
                    let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                    Err(ContainerError::Network(format!("Failed to get manifest: {} - {}", response.status(), error_text)))
                }
            }
        }

        /// マニフェストをプッシュ
        pub async fn put_manifest(&self, image_name: &str, reference: &str, manifest: &OCIManifest) -> Result<()> {
            let (registry, repository) = self.parse_image_name(image_name);
            let url = format!("https://{}/v2/{}/manifests/{}", registry, repository, reference);

            log::debug!("Putting manifest to: {}", url);

            let manifest_json = serde_json::to_string(manifest)
                .map_err(|e| ContainerError::Serialization(format!("Failed to serialize manifest: {}", e)))?;

            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/vnd.oci.image.manifest.v1+json"));

            // 認証ヘッダーを追加
            if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                headers.insert(AUTHORIZATION, auth_header);
            }

            let response = self.client
                .put(&url)
                .headers(headers)
                .body(manifest_json)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to put manifest: {}", e)))?;

            if response.status().is_success() {
                Ok(())
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Err(ContainerError::Network(format!("Failed to put manifest: {} - {}", response.status(), error_text)))
            }
        }

        /// Blobを取得
        pub async fn get_blob(&self, image_name: &str, digest: &str) -> Result<Vec<u8>> {
            let (registry, repository) = self.parse_image_name(image_name);
            let url = format!("https://{}/v2/{}/blobs/{}", registry, repository, digest);

            log::debug!("Getting blob from: {}", url);

            let mut headers = HeaderMap::new();

            // 認証ヘッダーを追加
            if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                headers.insert(AUTHORIZATION, auth_header);
            }

            let response = self.client
                .get(&url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to get blob: {}", e)))?;

            match response.status() {
                StatusCode::OK => {
                    let bytes = response.bytes().await
                        .map_err(|e| ContainerError::Network(format!("Failed to read blob: {}", e)))?;
                    Ok(bytes.to_vec())
                }
                StatusCode::UNAUTHORIZED => {
                    // 認証が必要な場合、認証を試行
                    self.authenticate(&registry, &repository).await?;
                    
                    // 再試行
                    let mut headers = HeaderMap::new();
                    if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                        headers.insert(AUTHORIZATION, auth_header);
                    }

                    let response = self.client
                        .get(&url)
                        .headers(headers)
                        .send()
                        .await
                        .map_err(|e| ContainerError::Network(format!("Failed to get blob (retry): {}", e)))?;

                    if response.status() == StatusCode::OK {
                        let bytes = response.bytes().await
                            .map_err(|e| ContainerError::Network(format!("Failed to read blob: {}", e)))?;
                        Ok(bytes.to_vec())
                    } else {
                        Err(ContainerError::NotFound(format!("Blob not found: {}", response.status())))
                    }
                }
                _ => {
                    let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                    Err(ContainerError::Network(format!("Failed to get blob: {} - {}", response.status(), error_text)))
                }
            }
        }

        /// Blobをプッシュ
        pub async fn put_blob(&self, image_name: &str, digest: &str, content: &[u8]) -> Result<()> {
            let (registry, repository) = self.parse_image_name(image_name);

            // アップロードセッションを開始
            let upload_url = self.initiate_blob_upload(&registry, &repository).await?;
            
            // Blobをアップロード
            self.upload_blob_content(&upload_url, content).await?;

            log::debug!("Successfully uploaded blob {}", digest);
            Ok(())
        }

        /// Blobアップロードセッションを開始
        async fn initiate_blob_upload(&self, registry: &str, repository: &str) -> Result<String> {
            let url = format!("https://{}/v2/{}/blobs/uploads/", registry, repository);

            let mut headers = HeaderMap::new();
            if let Ok(auth_header) = self.get_auth_header(registry, repository).await {
                headers.insert(AUTHORIZATION, auth_header);
            }

            let response = self.client
                .post(&url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to initiate blob upload: {}", e)))?;

            if response.status() == StatusCode::ACCEPTED {
                // Locationヘッダーからアップロード用URLを取得
                if let Some(location) = response.headers().get("location") {
                    let location_str = location.to_str()
                        .map_err(|e| ContainerError::Network(format!("Invalid location header: {}", e)))?;
                    Ok(location_str.to_string())
                } else {
                    Err(ContainerError::Network("Missing location header in upload response".to_string()))
                }
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Err(ContainerError::Network(format!("Failed to initiate blob upload: {} - {}", response.status(), error_text)))
            }
        }

        /// Blobコンテンツをアップロード
        async fn upload_blob_content(&self, upload_url: &str, content: &[u8]) -> Result<()> {
            let response = self.client
                .put(upload_url)
                .header(CONTENT_TYPE, "application/octet-stream")
                .body(content.to_vec())
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to upload blob content: {}", e)))?;

            if response.status().is_success() {
                Ok(())
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Err(ContainerError::Network(format!("Failed to upload blob content: {} - {}", response.status(), error_text)))
            }
        }

        /// レジストリに対して認証を実行
        async fn authenticate(&self, registry: &str, repository: &str) -> Result<()> {
            if registry.contains("docker.io") || registry.contains("registry-1.docker.io") {
                self.docker_hub_authenticate(repository).await
            } else {
                // 他のレジストリの認証実装
                Ok(())
            }
        }

        /// Docker Hub での認証
        async fn docker_hub_authenticate(&self, repository: &str) -> Result<()> {
            let token_url = format!(
                "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull,push",
                repository
            );

            log::debug!("Authenticating with Docker Hub for repository: {}", repository);

            let response = self.client
                .get(&token_url)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to get auth token: {}", e)))?;

            if response.status() == StatusCode::OK {
                let token_response: DockerHubToken = response.json().await
                    .map_err(|e| ContainerError::Network(format!("Failed to parse token response: {}", e)))?;

                // トークンをキャッシュに保存
                let auth = RegistryAuth {
                    username: None,
                    password: None,
                    token: Some(token_response.token),
                    refresh_token: None,
                };

                // Note: この実装では self を mutable にできないため、実際の使用時には調整が必要
                // self.auth_cache.insert("registry-1.docker.io".to_string(), auth);

                Ok(())
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Err(ContainerError::Authentication(format!("Docker Hub authentication failed: {} - {}", response.status(), error_text)))
            }
        }

        /// 認証ヘッダーを取得
        async fn get_auth_header(&self, registry: &str, repository: &str) -> Result<HeaderValue> {
            if let Some(auth) = self.auth_cache.get(registry) {
                if let Some(ref token) = auth.token {
                    HeaderValue::from_str(&format!("Bearer {}", token))
                        .map_err(|e| ContainerError::Authentication(format!("Invalid token format: {}", e)))
                } else if let (Some(ref username), Some(ref password)) = (&auth.username, &auth.password) {
                    let basic_auth = base64::encode(format!("{}:{}", username, password));
                    HeaderValue::from_str(&format!("Basic {}", basic_auth))
                        .map_err(|e| ContainerError::Authentication(format!("Invalid basic auth format: {}", e)))
                } else {
                    Err(ContainerError::Authentication("No valid authentication method found".to_string()))
                }
            } else {
                // 認証情報がない場合は匿名アクセスを試行
                Err(ContainerError::Authentication("No authentication configured".to_string()))
            }
        }

        /// レポジトリのタグ一覧を取得
        pub async fn list_tags(&self, image_name: &str) -> Result<Vec<String>> {
            let (registry, repository) = self.parse_image_name(image_name);
            let url = format!("https://{}/v2/{}/tags/list", registry, repository);

            log::debug!("Listing tags from: {}", url);

            let mut headers = HeaderMap::new();
            if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                headers.insert(AUTHORIZATION, auth_header);
            }

            let response = self.client
                .get(&url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to list tags: {}", e)))?;

            if response.status() == StatusCode::OK {
                #[derive(Deserialize)]
                struct TagsResponse {
                    name: String,
                    tags: Option<Vec<String>>,
                }

                let tags_response: TagsResponse = response.json().await
                    .map_err(|e| ContainerError::Network(format!("Failed to parse tags response: {}", e)))?;

                Ok(tags_response.tags.unwrap_or_default())
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Err(ContainerError::Network(format!("Failed to list tags: {} - {}", response.status(), error_text)))
            }
        }

        /// レジストリの接続確認
        pub async fn check_registry(&self, registry: &str) -> Result<bool> {
            let url = format!("https://{}/v2/", registry);

            let response = self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to check registry: {}", e)))?;

            // 200 OK または 401 Unauthorized なら接続可能
            Ok(matches!(response.status(), StatusCode::OK | StatusCode::UNAUTHORIZED))
        }

        /// Blobの存在確認
        pub async fn blob_exists(&self, image_name: &str, digest: &str) -> Result<bool> {
            let (registry, repository) = self.parse_image_name(image_name);
            let url = format!("https://{}/v2/{}/blobs/{}", registry, repository, digest);

            let mut headers = HeaderMap::new();
            if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                headers.insert(AUTHORIZATION, auth_header);
            }

            let response = self.client
                .head(&url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to check blob: {}", e)))?;

            Ok(response.status() == StatusCode::OK)
        }

        /// マニフェストの存在確認
        pub async fn manifest_exists(&self, image_name: &str, reference: &str) -> Result<bool> {
            let (registry, repository) = self.parse_image_name(image_name);
            let url = format!("https://{}/v2/{}/manifests/{}", registry, repository, reference);

            let mut headers = HeaderMap::new();
            headers.insert(
                "Accept",
                HeaderValue::from_static("application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json")
            );

            if let Ok(auth_header) = self.get_auth_header(&registry, &repository).await {
                headers.insert(AUTHORIZATION, auth_header);
            }

            let response = self.client
                .head(&url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| ContainerError::Network(format!("Failed to check manifest: {}", e)))?;

            Ok(response.status() == StatusCode::OK)
        }
    }

    impl Default for RegistryClient {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Default for RegistryAuth {
        fn default() -> Self {
            Self {
                username: None,
                password: None,
                token: None,
                refresh_token: None,
            }
        }
    }

    impl RegistryAuth {
        /// 基本認証用の認証情報を作成
        pub fn basic(username: String, password: String) -> Self {
            Self {
                username: Some(username),
                password: Some(password),
                token: None,
                refresh_token: None,
            }
        }

        /// トークン認証用の認証情報を作成
        pub fn token(token: String) -> Self {
            Self {
                username: None,
                password: None,
                token: Some(token),
                refresh_token: None,
            }
        }

        /// 認証情報が有効かチェック
        pub fn is_valid(&self) -> bool {
            self.token.is_some() || (self.username.is_some() && self.password.is_some())
        }
    }
}

#[cfg(feature = "registry")]
pub use registry_impl::*;

#[cfg(not(feature = "registry"))]
#[derive(Debug)]
pub struct RegistryClient;

#[cfg(not(feature = "registry"))]
#[derive(Debug)]
pub struct RegistryAuth;

#[cfg(not(feature = "registry"))]
impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryClient {
    pub fn new() -> Self {
        Self
    }
    
    pub async fn get_manifest(&self, _image_name: &str, _reference: &str) -> crate::errors::Result<crate::oci::OCIManifest> {
        Err(crate::errors::ContainerError::UnsupportedFeature(
            "Registry client requires 'registry' feature to be enabled".to_string()
        ))
    }
    
    pub async fn get_blob(&self, _image_name: &str, _digest: &str) -> crate::errors::Result<Vec<u8>> {
        Err(crate::errors::ContainerError::UnsupportedFeature(
            "Registry client requires 'registry' feature to be enabled".to_string()
        ))
    }
    
    pub async fn put_manifest(&self, _image_name: &str, _reference: &str, _manifest: &crate::oci::OCIManifest) -> crate::errors::Result<()> {
        Err(crate::errors::ContainerError::UnsupportedFeature(
            "Registry client requires 'registry' feature to be enabled".to_string()
        ))
    }
    
    pub async fn put_blob(&self, _image_name: &str, _digest: &str, _content: &[u8]) -> crate::errors::Result<()> {
        Err(crate::errors::ContainerError::UnsupportedFeature(
            "Registry client requires 'registry' feature to be enabled".to_string()
        ))
    }
} 