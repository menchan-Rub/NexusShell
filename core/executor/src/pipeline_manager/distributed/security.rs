/**
 * 分散セキュリティモジュール
 * 
 * 分散パイプライン実行におけるセキュリティ機能を担当するモジュール
 */

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use ring::{digest, hmac, rand, signature};
use serde::{Serialize, Deserialize};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn, error};
use uuid::Uuid;
use argon2::{
    password_hash::{
        rand_core::OsRng,
        PasswordHasher, SaltString
    },
    Argon2, Algorithm, Version, Params
};
use x509_parser;
use sha2;
use reqwest;
use std::io::Cursor;
use aes_gcm;
use x25519_dalek;
use hkdf;
use ed25519_dalek;
use key_store::KeyStore;
use chacha20poly1305;
use der::{
    asn1::{Ia5String, OctetString, BitString},
    Decodable, Decoder, Encodable, Sequence, Tag,
};
use ocsp_rs;

use super::node::{NodeId, NodeInfo, NodeStatus};
use super::communication::{MessageType, DistributedMessage, CommunicationManager};

/// セキュリティ設定
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// 認証を有効化するか
    pub enable_authentication: bool,
    /// 暗号化を有効化するか
    pub enable_encryption: bool,
    /// セキュリティポリシー
    pub security_policy: SecurityPolicy,
    /// 認証タイムアウト（秒）
    pub auth_timeout_sec: u64,
    /// 認証トークンの有効期間（秒）
    pub token_validity_sec: u64,
    /// 証明書ファイルパス
    pub cert_path: Option<PathBuf>,
    /// 秘密鍵ファイルパス
    pub key_path: Option<PathBuf>,
    /// 認証局証明書パス
    pub ca_cert_path: Option<PathBuf>,
    /// サーバー検証を有効化するか
    pub verify_server: bool,
    /// クライアント検証を有効化するか
    pub verify_client: bool,
    /// 暗号化アルゴリズム
    pub encryption_algorithm: Option<String>,
    /// 整合性ハッシュを追加するか
    pub add_integrity_hash: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_authentication: true,
            enable_encryption: true,
            security_policy: SecurityPolicy::default(),
            auth_timeout_sec: 30,
            token_validity_sec: 86400, // 24時間
            cert_path: None,
            key_path: None,
            ca_cert_path: None,
            verify_server: true,
            verify_client: true,
            encryption_algorithm: None,
            add_integrity_hash: true,
        }
    }
}

/// セキュリティポリシー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityPolicy {
    /// 最小パスワード長
    pub min_password_length: usize,
    /// パスワード複雑さ要件
    pub password_complexity: PasswordComplexity,
    /// 最大認証失敗回数
    pub max_auth_failures: u32,
    /// トークン更新間隔（秒）
    pub token_refresh_sec: u64,
    /// アクセス制御ポリシー
    pub access_control: AccessControlPolicy,
    /// TLSバージョン
    pub tls_version: TlsVersion,
    /// 暗号スイート
    pub cipher_suites: Vec<String>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            min_password_length: 12,
            password_complexity: PasswordComplexity::Strong,
            max_auth_failures: 5,
            token_refresh_sec: 3600, // 1時間
            access_control: AccessControlPolicy::RoleBased,
            tls_version: TlsVersion::Tls13,
            cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
            ],
        }
    }
}

/// パスワード複雑さ要件
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordComplexity {
    /// 基本（文字種混在なし）
    Basic,
    /// 中程度（少なくとも2種類の文字種）
    Medium,
    /// 強力（少なくとも3種類の文字種）
    Strong,
}

/// アクセス制御ポリシー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessControlPolicy {
    /// なし
    None,
    /// シンプル（読み取り/書き込み権限のみ）
    Simple,
    /// ロールベース
    RoleBased,
    /// ACLベース
    AclBased,
}

/// TLSバージョン
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    /// TLS 1.2
    Tls12,
    /// TLS 1.3
    Tls13,
}

/// 認証方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    /// パスワード認証
    Password,
    /// 証明書認証
    Certificate,
    /// トークン認証
    Token,
    /// マルチファクター認証
    MultiFactorAuth,
}

/// 認証情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCredentials {
    /// ノードID
    pub node_id: String,
    /// 認証方式
    pub method: AuthMethod,
    /// ユーザー名
    pub username: Option<String>,
    /// パスワード
    pub password: Option<String>,
    /// 証明書指紋
    pub cert_fingerprint: Option<String>,
    /// 認証トークン
    pub token: Option<String>,
    /// タイムスタンプ（ミリ秒）
    pub timestamp: u64,
    /// ノンス
    pub nonce: String,
}

/// 認証結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    /// 認証成功フラグ
    pub success: bool,
    /// エラーメッセージ
    pub error_message: Option<String>,
    /// セッショントークン
    pub session_token: Option<String>,
    /// トークン有効期限（ミリ秒）
    pub token_expiry: Option<u64>,
    /// リフレッシュトークン
    pub refresh_token: Option<String>,
    /// ユーザーロール
    pub roles: Vec<String>,
    /// ノードID
    pub node_id: String,
    /// タイムスタンプ（ミリ秒）
    pub timestamp: u64,
    /// サーバーノンス
    pub server_nonce: String,
}

/// ノードの認証状態
#[derive(Debug, Clone)]
pub struct NodeAuthState {
    /// ノードID
    pub node_id: NodeId,
    /// 認証状態
    pub authenticated: bool,
    /// セッショントークン
    pub session_token: Option<String>,
    /// トークン有効期限
    pub token_expiry: Option<DateTime<Utc>>,
    /// 認証方式
    pub auth_method: AuthMethod,
    /// 認証失敗回数
    pub auth_failures: u32,
    /// 最終認証時刻
    pub last_auth_time: Option<DateTime<Utc>>,
    /// ロール
    pub roles: Vec<String>,
}

impl NodeAuthState {
    /// 新しいノード認証状態を作成
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            authenticated: false,
            session_token: None,
            token_expiry: None,
            auth_method: AuthMethod::Password,
            auth_failures: 0,
            last_auth_time: None,
            roles: Vec::new(),
        }
    }
    
    /// 認証トークンが有効か確認
    pub fn is_token_valid(&self) -> bool {
        match (self.authenticated, &self.token_expiry) {
            (true, Some(expiry)) => *expiry > Utc::now(),
            _ => false,
        }
    }
    
    /// 新しいトークンで認証状態を更新
    pub fn update_token(&mut self, token: String, expiry: DateTime<Utc>) {
        self.authenticated = true;
        self.session_token = Some(token);
        self.token_expiry = Some(expiry);
        self.auth_failures = 0;
        self.last_auth_time = Some(Utc::now());
    }
    
    /// 認証を無効化
    pub fn invalidate(&mut self) {
        self.authenticated = false;
        self.session_token = None;
    }
    
    /// 認証失敗を記録
    pub fn record_failure(&mut self) {
        self.auth_failures += 1;
    }
}

/// アクセス許可
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// 読み取り
    Read,
    /// 書き込み
    Write,
    /// 実行
    Execute,
    /// 管理
    Admin,
}

/// リソース種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// ノード
    Node,
    /// タスク
    Task,
    /// データ
    Data,
    /// パイプライン
    Pipeline,
    /// 設定
    Configuration,
}

/// アクセス制御エントリ
#[derive(Debug, Clone)]
pub struct AccessControlEntry {
    /// ロール
    pub role: String,
    /// リソース種別
    pub resource_type: ResourceType,
    /// リソースID（ワイルドカードの場合は"*"）
    pub resource_id: String,
    /// 許可されたアクセス権限
    pub permissions: Vec<Permission>,
}

/// セキュリティマネージャー
pub struct SecurityManager {
    /// セキュリティ設定
    config: SecurityConfig,
    /// 通信マネージャー
    comm_manager: Arc<CommunicationManager>,
    /// ノード認証状態
    node_auth_states: Arc<RwLock<HashMap<NodeId, NodeAuthState>>>,
    /// アクセス制御リスト
    access_control_list: Arc<RwLock<Vec<AccessControlEntry>>>,
    /// ユーザークレデンシャル
    credentials: Arc<RwLock<HashMap<String, String>>>,
    /// 有効なトークン
    valid_tokens: Arc<RwLock<HashMap<String, (NodeId, DateTime<Utc>)>>>,
    /// ローカルノードID
    local_node_id: NodeId,
    /// 署名キーペア
    signing_key: Option<SigningKey>,
}

impl SecurityManager {
    /// 新しいセキュリティマネージャーを作成
    pub fn new(local_node_id: NodeId, comm_manager: Arc<CommunicationManager>, config: SecurityConfig) -> Self {
        Self {
            config,
            comm_manager,
            node_auth_states: Arc::new(RwLock::new(HashMap::new())),
            access_control_list: Arc::new(RwLock::new(Vec::new())),
            credentials: Arc::new(RwLock::new(HashMap::new())),
            valid_tokens: Arc::new(RwLock::new(HashMap::new())),
            local_node_id,
            signing_key: None,
        }
    }
    
    /// ノードを認証
    pub async fn authenticate_node(&self, node_id: &NodeId, credentials: &AuthCredentials) -> Result<AuthResult> {
        debug!("ノード {} の認証試行", node_id);
        
        // 認証しない設定の場合は常に成功
        if !self.config.enable_authentication {
            return Ok(self.create_success_auth_result(node_id));
        }
        
        // ノード認証状態を取得または作成
        let mut node_state = {
            let mut states = self.node_auth_states.write().await;
            states.entry(node_id.clone())
                .or_insert_with(|| NodeAuthState::new(node_id.clone()))
                .clone()
        };
        
        // 認証失敗回数が上限を超えているか確認
        if node_state.auth_failures >= self.config.security_policy.max_auth_failures {
            debug!("ノード {} は認証失敗回数の上限に達しています", node_id);
            
            // 認証失敗結果を返す
            return Ok(AuthResult {
                success: false,
                error_message: Some("認証失敗回数の上限に達しています".to_string()),
                session_token: None,
                token_expiry: None,
                refresh_token: None,
                roles: Vec::new(),
                node_id: node_id.to_string(),
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                server_nonce: generate_nonce(),
            });
        }
        
        // 認証方式に基づいて認証
        let success = match credentials.method {
            AuthMethod::Password => {
                // パスワード認証
                if let (Some(username), Some(password)) = (&credentials.username, &credentials.password) {
                    self.authenticate_with_password(username, password).await
                } else {
                    false
                }
            },
            AuthMethod::Token => {
                // トークン認証
                if let Some(token) = &credentials.token {
                    self.validate_token(token, node_id).await
                } else {
                    false
                }
            },
            AuthMethod::Certificate => {
                // 証明書認証
                if let Some(certificate_pem) = &credentials.cert_fingerprint {
                    self.authenticate_with_certificate(certificate_pem).await
                } else {
                    false
                }
            },
            AuthMethod::MultiFactorAuth => {
                // マルチファクター認証（未実装）
                debug!("マルチファクター認証は未実装です");
                false
            },
        };
        
        if success {
            debug!("ノード {} の認証に成功", node_id);
            
            // 認証成功時の処理
            let token = generate_token();
            let expiry = Utc::now() + chrono::Duration::seconds(self.config.token_validity_sec as i64);
            
            // ノード認証状態を更新
            node_state.update_token(token.clone(), expiry);
            
            // 有効なトークンを保存
            {
                let mut tokens = self.valid_tokens.write().await;
                tokens.insert(token.clone(), (node_id.clone(), expiry));
            }
            
            // 認証状態を保存
            {
                let mut states = self.node_auth_states.write().await;
                states.insert(node_id.clone(), node_state.clone());
            }
            
            // 認証成功結果を生成
            Ok(AuthResult {
                success: true,
                error_message: None,
                session_token: Some(token),
                token_expiry: Some(expiry.timestamp_millis() as u64),
                refresh_token: Some(generate_token()),
                roles: node_state.roles,
                node_id: node_id.to_string(),
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                server_nonce: generate_nonce(),
            })
        } else {
            debug!("ノード {} の認証に失敗", node_id);
            
            // 認証失敗時の処理
            node_state.record_failure();
            
            // 認証状態を保存
            {
                let mut states = self.node_auth_states.write().await;
                states.insert(node_id.clone(), node_state);
            }
            
            // 認証失敗結果を生成
            Ok(AuthResult {
                success: false,
                error_message: Some("認証に失敗しました".to_string()),
                session_token: None,
                token_expiry: None,
                refresh_token: None,
                roles: Vec::new(),
                node_id: node_id.to_string(),
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                server_nonce: generate_nonce(),
            })
        }
    }
    
    /// 認証成功結果を作成
    fn create_success_auth_result(&self, node_id: &NodeId) -> AuthResult {
        let token = generate_token();
        let expiry = Utc::now() + chrono::Duration::seconds(self.config.token_validity_sec as i64);
        
        AuthResult {
            success: true,
            error_message: None,
            session_token: Some(token),
            token_expiry: Some(expiry.timestamp_millis() as u64),
            refresh_token: Some(generate_token()),
            roles: vec!["node".to_string()],
            node_id: node_id.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            server_nonce: generate_nonce(),
        }
    }
    
    /// パスワードで認証
    async fn authenticate_with_password(&self, username: &str, password: &str) -> bool {
        let credentials = self.credentials.read().await;
        
        if let Some(stored_password) = credentials.get(username) {
            // パスワードハッシュの検証
            verify_password(password, stored_password)
        } else {
            false
        }
    }
    
    /// 証明書で認証
    async fn authenticate_with_certificate(&self, certificate_pem: &str) -> bool {
        use x509_parser::{
            certificate::X509Certificate,
            prelude::*,
            error::X509Error,
            extensions::GeneralName
        };
        use sha2::{Sha256, Digest};
        use reqwest;
        use std::io::Cursor;
        
        debug!("証明書認証を開始");
        
        // PEM形式の証明書を解析
        let parse_result = match x509_parser::pem::parse_x509_pem(certificate_pem.as_bytes()) {
            Ok(pem) => pem.1.parse_x509(),
            Err(e) => {
                error!("証明書のPEM解析に失敗: {}", e);
                return false;
            }
        };
        
        // 証明書オブジェクトを取得
        let certificate = match parse_result {
            Ok(cert) => cert,
            Err(e) => {
                error!("X509証明書の解析に失敗: {}", e);
                return false;
            }
        };
        
        debug!("証明書が正常に解析されました: {:?}", certificate.subject);
        
        // 1. 有効期限チェック
        let now = time::OffsetDateTime::now_utc();
        if now < certificate.validity.not_before.to_datetime() || 
           now > certificate.validity.not_after.to_datetime() {
            warn!("証明書が期限切れまたは未有効: {:?}", certificate.subject);
            return false;
        }
        
        debug!("証明書の有効期限チェックに合格");
        
        // 2. 発行者が信頼できるかチェック
        // データベースから信頼できる発行者リストを取得
        let trusted_issuers = match self.get_trusted_issuers().await {
            Ok(issuers) => issuers,
            Err(e) => {
                error!("信頼できる発行者リストの取得に失敗: {}", e);
                return false;
            }
        };
        
        let cert_issuer = certificate.issuer.to_string();
        if !trusted_issuers.contains(&cert_issuer) {
            warn!("証明書の発行者が信頼できません: {}", cert_issuer);
            return false;
        }
        
        debug!("証明書発行者の検証に合格: {}", cert_issuer);
        
        // 3. フィンガープリントを計算して許可リストと照合
        let cert_der = match certificate.to_der() {
            Ok(der) => der,
            Err(e) => {
                error!("証明書のDER形式への変換に失敗: {}", e);
                return false;
            }
        };
        
        let mut hasher = Sha256::new();
        hasher.update(&cert_der);
        let fingerprint = hasher.finalize();
        let fingerprint_hex = fingerprint.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<String>>()
            .join(":");
        
        // データベースから信頼できるフィンガープリントリストを取得
        let trusted_fingerprints = match self.get_trusted_fingerprints().await {
            Ok(fingerprints) => fingerprints,
            Err(e) => {
                error!("信頼できるフィンガープリントリストの取得に失敗: {}", e);
                return false;
            }
        };
        
        if !trusted_fingerprints.contains(&fingerprint_hex) {
            warn!("証明書のフィンガープリントが信頼できません: {}", fingerprint_hex);
            return false;
        }
        
        debug!("証明書フィンガープリントの検証に合格: {}", fingerprint_hex);
        
        // 4. 失効チェック（CRL）
        if let Some(crl_dp_ext) = certificate.extensions().iter()
            .find(|ext| ext.oid == oid_registry::OID_X509_EXT_CRL_DISTRIBUTION_POINTS) {
            
            if let Ok(crl_dp) = crl_dp_ext.parsed_extension() {
                if let ParsedExtension::CRLDistributionPoints(points) = crl_dp {
                    for dp in points.iter() {
                        for name in dp.distribution_point.iter().flatten() {
                            if let GeneralName::URI(uri) = name {
                                debug!("CRL配布ポイントを確認中: {}", uri);
                                
                                // CRLをダウンロード
                                let crl_response = match reqwest::blocking::get(uri.to_string()) {
                                    Ok(response) => response,
                                    Err(e) => {
                                        warn!("CRLのダウンロードに失敗: {}", e);
                                        continue;
                                    }
                                };
                                
                                if !crl_response.status().is_success() {
                                    warn!("CRLの取得に失敗: ステータスコード {}", crl_response.status());
                                    continue;
                                }
                                
                                let crl_data = match crl_response.bytes() {
                                    Ok(data) => data,
                                    Err(e) => {
                                        warn!("CRLデータの読み取りに失敗: {}", e);
                                        continue;
                                    }
                                };
                                
                                // CRLを解析
                                match x509_parser::parse_x509_crl(&crl_data) {
                                    Ok((_, crl)) => {
                                        debug!("CRLを正常に解析: 発行者 {}", crl.tbs_cert_list.issuer);
                                        
                                        // 証明書のシリアル番号がCRLに含まれているかチェック
                                        let serial = certificate.serial;
                                        for revoked in crl.iter_revoked_certificates() {
                                            if revoked.raw_serial_as_slice() == serial.as_slice() {
                                                warn!("証明書が失効しています: {:?}, シリアル番号: {}", 
                                                     certificate.subject, serial);
                                                return false;
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        warn!("CRLの解析に失敗: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        debug!("失効チェックに合格");
        
        // 5. OCSP（オンライン証明書状態プロトコル）チェック
        if let Some(aia_ext) = certificate.extensions().iter()
            .find(|ext| ext.oid == oid_registry::OID_PKIX_AUTHORITY_INFO_ACCESS) {
            
            if let Ok(aia) = aia_ext.parsed_extension() {
                if let ParsedExtension::AuthorityInfoAccess(access_descriptions) = aia {
                    for ad in access_descriptions.iter() {
                        if ad.access_method == oid_registry::OID_PKIX_OCSP {
                            if let GeneralName::URI(uri) = &ad.access_location {
                                debug!("OCSPレスポンダーのURL: {}", uri);
                                
                                // OCSP要求の構築
                                let ocsp_req = match self.build_ocsp_request(&certificate) {
                                    Ok(req) => req,
                                    Err(e) => {
                                        warn!("OCSP要求の構築に失敗: {}", e);
                                        continue;
                                    }
                                };
                                
                                // OCSPサーバーに要求を送信
                                let ocsp_response = match reqwest::blocking::Client::new()
                                    .post(uri.to_string())
                                    .header("Content-Type", "application/ocsp-request")
                                    .body(ocsp_req)
                                    .send() {
                                    Ok(response) => response,
                                    Err(e) => {
                                        warn!("OCSP要求の送信に失敗: {}", e);
                                        continue;
                                    }
                                };
                                
                                if !ocsp_response.status().is_success() {
                                    warn!("OCSP応答の取得に失敗: ステータスコード {}", ocsp_response.status());
                                    continue;
                                }
                                
                                let ocsp_data = match ocsp_response.bytes() {
                                    Ok(data) => data,
                                    Err(e) => {
                                        warn!("OCSP応答データの読み取りに失敗: {}", e);
                                        continue;
                                    }
                                };
                                
                                // OCSP応答を検証
                                match self.verify_ocsp_response(&ocsp_data, &certificate) {
                                    Ok(status) => {
                                        if !status {
                                            warn!("OCSP検証失敗: 証明書が失効しています");
                                            return false;
                                        }
                                    },
                                    Err(e) => {
                                        warn!("OCSP応答の検証に失敗: {}", e);
                                        // OCSP検証失敗は必ずしも認証失敗とは限らない
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // 6. 証明書の使用目的をチェック
        let has_client_auth = certificate.extensions().iter()
            .filter_map(|ext| {
                if ext.oid == oid_registry::OID_X509_EXT_KEY_USAGE ||
                   ext.oid == oid_registry::OID_X509_EXT_EXTENDED_KEY_USAGE {
                    ext.parsed_extension().ok()
                } else {
                    None
                }
            })
            .any(|parsed_ext| {
                match parsed_ext {
                    ParsedExtension::KeyUsage(usage) => {
                        usage.digital_signature() || usage.key_agreement()
                    },
                    ParsedExtension::ExtendedKeyUsage(usage) => {
                        usage.client_auth
                    },
                    _ => false
                }
            });
        
        if !has_client_auth {
            warn!("証明書にクライアント認証の使用目的が含まれていません");
            return false;
        }
        
        // 7. 証明書のサブジェクトと要求されたユーザー名の照合
        let subject_cn = certificate.subject.iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .unwrap_or("");
        
        // ノードIDから期待されるユーザー名を取得
        let expected_username = match self.get_expected_username_for_node().await {
            Ok(username) => username,
            Err(e) => {
                error!("ノードのユーザー名取得エラー: {}", e);
                return false;
            }
        };
        
        if subject_cn != expected_username {
            warn!("証明書のCN ({}) が期待されるユーザー名 ({}) と一致しません", 
                  subject_cn, expected_username);
            return false;
        }
        
        // すべてのチェックに合格
        info!("証明書認証成功: {:?}", certificate.subject);
        true
    }
    
    /// OCSP要求を構築
    fn build_ocsp_request(&self, certificate: &x509_parser::certificate::X509Certificate) -> Result<Vec<u8>, anyhow::Error> {
        use asn1_rs::{Der, FromDer, ToDer};
        use sha1::{Sha1, Digest};
        
        debug!("OCSP要求を構築しています");
        
        // 発行者証明書を取得
        let issuer_cert = self.get_issuer_certificate(certificate)
            .map_err(|e| anyhow!("発行者証明書の取得に失敗: {}", e))?;
        
        // 発行者の名前とキーのハッシュを計算
        let mut name_hasher = Sha1::new();
        name_hasher.update(issuer_cert.subject.as_raw());
        let name_hash = name_hasher.finalize();
        
        let issuer_key = issuer_cert.public_key();
        let mut key_hasher = Sha1::new();
        key_hasher.update(issuer_key.raw);
        let key_hash = key_hasher.finalize();
        
        // OCSP要求を構築
        let mut ocsp_request = vec![
            // OCSP Request シーケンス
            0x30, 0x00, // 長さは後で更新
        ];
        
        // TBSRequest
        let mut tbs_request = vec![
            0x30, 0x00, // 長さは後で更新
            
            // バージョン (v1)
            0xA0, 0x03, 0x02, 0x01, 0x00,
            
            // requestorName (省略)
            
            // requestList
            0x30, 0x00, // 長さは後で更新
        ];
        
        // 単一のRequest
        let mut single_request = vec![
            0x30, 0x00, // 長さは後で更新
            
            // certID
            0x30, 0x00, // 長さは後で更新
        ];
        
        // hashAlgorithm (SHA-1)
        let hash_algorithm = vec![
            0x30, 0x09, // シーケンス長
            0x06, 0x05, 0x2B, 0x0E, 0x03, 0x02, 0x1A, // SHA-1 OID
            0x05, 0x00  // NULL
        ];
        
        single_request.extend_from_slice(&hash_algorithm);
        
        // issuerNameHash
        let mut issuer_name_hash = vec![0x04, name_hash.len() as u8];
        issuer_name_hash.extend_from_slice(&name_hash);
        single_request.extend_from_slice(&issuer_name_hash);
        
        // issuerKeyHash
        let mut issuer_key_hash = vec![0x04, key_hash.len() as u8];
        issuer_key_hash.extend_from_slice(&key_hash);
        single_request.extend_from_slice(&issuer_key_hash);
        
        // serialNumber
        let serial = certificate.serial();
        let mut serial_bytes = vec![0x02, serial.as_slice().len() as u8];
        serial_bytes.extend_from_slice(serial.as_slice());
        single_request.extend_from_slice(&serial_bytes);
        
        // 長さを更新
        self.update_sequence_length(&mut single_request, 3);
        
        // requestList に追加
        tbs_request.extend_from_slice(&single_request);
        self.update_sequence_length(&mut tbs_request, 9);
        
        // TBSRequest に追加
        ocsp_request.extend_from_slice(&tbs_request);
        self.update_sequence_length(&mut ocsp_request, 1);
        
        debug!("OCSP要求を構築しました ({}バイト)", ocsp_request.len());
        Ok(ocsp_request)
    }
    
    /// 発行者証明書を取得
    fn get_issuer_certificate(&self, certificate: &x509_parser::certificate::X509Certificate) -> Result<x509_parser::certificate::X509Certificate, anyhow::Error> {
        use std::collections::HashMap;
        use std::sync::Arc;
        use x509_parser::prelude::*;
        
        // データベースから発行者証明書を検索
        let issuer_name = certificate.issuer.to_string();
        let issuer_cert_bytes = self.db_client.query_single(
            "SELECT cert_data FROM trusted_certificates WHERE subject = $1",
            &[&issuer_name]
        ).await?.ok_or_else(|| anyhow!("発行者証明書が見つかりません: {}", issuer_name))?;
        
        let cert_data: Vec<u8> = issuer_cert_bytes.get(0);
        let (_, issuer_cert) = x509_parser::parse_x509_certificate(&cert_data)
            .map_err(|e| anyhow!("発行者証明書の解析に失敗: {}", e))?;
        
        Ok(issuer_cert)
    }
    
    /// シーケンスの長さを更新
    fn update_sequence_length(&self, data: &mut Vec<u8>, pos: usize) {
        let length = data.len() - pos - 2;
        data[pos + 1] = length as u8;
    }

    /// OCSP応答を検証
    fn verify_ocsp_response(&self, ocsp_data: &[u8], certificate: &x509_parser::certificate::X509Certificate) -> Result<bool, anyhow::Error> {
        use asn1_rs::{Der, FromDer};
        use sha1::{Sha1, Digest};
        
        debug!("OCSP応答を検証しています ({}バイト)", ocsp_data.len());
        
        // OCSP応答を解析
        if ocsp_data.len() < 2 || ocsp_data[0] != 0x30 {
            return Err(anyhow!("無効なOCSP応答形式"));
        }
        
        // responseStatus (必須)
        let status_offset = 4; // 基本的なASN.1ヘッダーの後
        if ocsp_data.len() < status_offset + 3 || ocsp_data[status_offset] != 0x0A {
            return Err(anyhow!("無効なOCSP応答ステータス形式"));
        }
        
        // ステータスコードを取得 (0 = 成功)
        let response_status = ocsp_data[status_offset + 2];
        if response_status != 0 {
            return Err(anyhow!("OCSP応答が成功ではありません: {}", response_status));
        }
        
        // ResponseBytes を解析 (通常は offset 7から)
        let resp_bytes_offset = status_offset + 3;
        if ocsp_data.len() < resp_bytes_offset + 5 || ocsp_data[resp_bytes_offset] != 0xA0 {
            return Err(anyhow!("無効なOCSP ResponseBytes形式"));
        }
        
        // BasicOCSPResponse のOIDを確認
        let oid_offset = resp_bytes_offset + 5;
        if ocsp_data.len() < oid_offset + 10 || ocsp_data[oid_offset + 2] != 0x06 {
            return Err(anyhow!("無効なOCSP ResponseType OID形式"));
        }
        
        // 証明書のシリアル番号を取得
        let cert_serial = certificate.serial();
        let cert_serial_bytes = cert_serial.as_slice();
        
        // SingleResponse を検索して証明書のステータスを確認
        let response_data_offset = self.find_response_data(ocsp_data)?;
        let single_responses_offset = self.find_single_responses(ocsp_data, response_data_offset)?;
        
        // 対象の証明書に対する応答を検索
        let cert_status = self.find_certificate_status(ocsp_data, single_responses_offset, cert_serial_bytes)?;
        
        match cert_status {
            0 => {
                // good
                debug!("証明書は有効です: {}", certificate.subject);
                Ok(true)
            },
            1 => {
                // revoked
                warn!("証明書は失効しています: {}", certificate.subject);
                Ok(false)
            },
            2 => {
                // unknown
                warn!("証明書の状態は不明です: {}", certificate.subject);
                // ポリシーに応じて不明を許可するかどうか決定
                Ok(self.config.security_policy.allow_unknown_cert_status)
            },
            _ => Err(anyhow!("不明な証明書ステータス値: {}", cert_status))
        }
    }
    
    /// OCSP応答からResponseDataを検索
    fn find_response_data(&self, ocsp_data: &[u8]) -> Result<usize, anyhow::Error> {
        // 厳密なASN.1パーサーを使用してOCSP応答を解析
        use der::{
            asn1::{Ia5String, OctetString, BitString},
            Decodable, Decoder, Encodable, Sequence, Tag,
        };
        use x509_parser::prelude::*;
        use chrono::{DateTime, TimeZone, Utc};

        // OCSPResponseは以下の構造:
        // OCSPResponse ::= SEQUENCE {
        //     responseStatus         OCSPResponseStatus,
        //     responseBytes          [0] EXPLICIT ResponseBytes OPTIONAL }
        debug!("OCSP ResponseDataを厳密に解析中...");
        
        // OCSPResponseの解析
        let ocsp_response = ocsp_rs::OcspResponse::parse(ocsp_data)
            .map_err(|e| anyhow!("OCSP応答のパースに失敗: {}", e))?;
        
        // レスポンスステータスの確認
        if ocsp_response.response_status != ocsp_rs::OcspResponseStatus::Successful {
            return Err(anyhow!("OCSP応答が成功ではありません: {:?}", ocsp_response.response_status));
        }
        
        // ResponseBytesの取得
        let response_bytes = ocsp_response.response_bytes
            .ok_or_else(|| anyhow!("OCSP応答にResponseBytesがありません"))?;
        
        // ResponseTypeがid-pkix-ocsp-basicである確認
        if response_bytes.response_type != oid_registry::OID_PKIX_OCSP_BASIC {
            return Err(anyhow!("OCSP応答タイプが不正です: {:?}", response_bytes.response_type));
        }
        
        // BasicOCSPResponseの解析
        let basic_ocsp = ocsp_rs::BasicOcspResponse::parse(&response_bytes.response)
            .map_err(|e| anyhow!("BasicOCSPResponseのパースに失敗: {}", e))?;
        
        // ResponseDataのオフセットをASN.1 DERパーサで厳密に計算
        let offset = asn1::parse_response_data_offset(&basic_ocsp.tbs_response_data)?;
        
        // 対象の証明書に対する応答を検索
        let cert_status = self.find_certificate_status(ocsp_data, offset, cert_serial_bytes)?;
        
        match cert_status {
            0 => {
                // good
                debug!("証明書は有効です: {}", certificate.subject);
                Ok(offset)
            },
            1 => {
                // revoked
                warn!("証明書は失効しています: {}", certificate.subject);
                Ok(offset)
            },
            2 => {
                // unknown
                warn!("証明書の状態は不明です: {}", certificate.subject);
                // ポリシーに応じて不明を許可するかどうか決定
                Ok(offset)
            },
            _ => Err(anyhow!("不明な証明書ステータス値: {}", cert_status))
        }
    }
    
    /// ResponseDataからSingleResponsesを検索
    fn find_single_responses(&self, ocsp_data: &[u8], response_data_offset: usize) -> Result<usize, anyhow::Error> {
        // ASN.1構造を正確にパース
        use asn1_rs::{FromDer, Set, Sequence};
        
        debug!("OCSP SingleResponsesを解析中...");
        
        // ResponseDataは以下の構造:
        // ResponseData ::= SEQUENCE {
        //    version              [0] EXPLICIT Version DEFAULT v1,
        //    responderID              ResponderID,
        //    producedAt               GeneralizedTime,
        //    responses                SEQUENCE OF SingleResponse,
        //    responseExtensions   [1] EXPLICIT Extensions OPTIONAL }
        
        // ResponseDataのシーケンスを解析
        let response_data = &ocsp_data[response_data_offset..];
        
        // ASN.1 DERパーサーを使用してシーケンスを解析
        let (seq_content, _) = asn1_rs::der::parse_der_sequence(response_data)
            .map_err(|e| anyhow!("ResponseDataシーケンス解析エラー: {}", e))?;
        
        // version [0] EXPLICIT Version DEFAULT v1 (Optional)
        let mut offset = 0;
        if seq_content[0] == 0xA0 {
            // versionが存在する場合はスキップ
            let len = seq_content[1] as usize + 2;
            offset += len;
        }
        
        // responderID ResponderID
        let (responder_id_len, _) = asn1_rs::der::parse_der_length(&seq_content[offset+1..])
            .map_err(|e| anyhow!("ResponderID長さ解析エラー: {}", e))?;
        offset += responder_id_len + 2; // タグ(1) + 長さバイト(1+) + 内容
        
        // producedAt GeneralizedTime
        if seq_content[offset] != 0x18 { // GeneralizedTime タグ
            return Err(anyhow!("GeneralizedTimeタグが見つかりません"));
        }
        let produced_at_len = seq_content[offset+1] as usize;
        offset += produced_at_len + 2; // タグ(1) + 長さ(1) + 内容
        
        // responses SEQUENCE OF SingleResponse
        if seq_content[offset] != 0x30 { // SEQUENCE タグ
            return Err(anyhow!("responsesシーケンスが見つかりません"));
        }
        
        // responsesシーケンスのオフセットを返す
        let abs_offset = response_data_offset + offset;
        debug!("SingleResponsesシーケンスを発見: オフセット={}", abs_offset);
        
        Ok(abs_offset)
    }
    
    /// 証明書のステータスを検索
    fn find_certificate_status(&self, ocsp_data: &[u8], single_responses_offset: usize, cert_serial: &[u8]) -> Result<u8, anyhow::Error> {
        // ASN.1構造を正確にパース
        use asn1_rs::{FromDer, Sequence};
        
        debug!("証明書ステータスを検索: シリアル={:?}", cert_serial);
        
        // SingleResponsesシーケンスを取得
        let responses_seq = &ocsp_data[single_responses_offset..];
        
        // シーケンスの長さを取得
        let (seq_len, len_bytes) = asn1_rs::der::parse_der_length(&responses_seq[1..])
            .map_err(|e| anyhow!("SingleResponsesシーケンス長解析エラー: {}", e))?;
        
        // シーケンスの内容を取得
        let seq_content_start = 1 + len_bytes;
        let seq_content_end = seq_content_start + seq_len;
        let seq_content = &responses_seq[seq_content_start..seq_content_end];
        
        // 各SingleResponseを解析
        let mut content_offset = 0;
        while content_offset < seq_content.len() {
            // SingleResponseシーケンスを取得
            if seq_content[content_offset] != 0x30 {
                // シーケンスでない場合はスキップ
                content_offset += 1;
                continue;
            }
            
            // SingleResponseの長さを取得
            let (resp_len, resp_len_bytes) = asn1_rs::der::parse_der_length(&seq_content[content_offset+1..])
                .map_err(|e| anyhow!("SingleResponse長解析エラー: {}", e))?;
            
            // SingleResponseの内容を取得
            let resp_content_start = content_offset + 1 + resp_len_bytes;
            let resp_content_end = resp_content_start + resp_len;
            let resp_content = &seq_content[resp_content_start..resp_content_end];
            
            // CertIDを解析
            if resp_content[0] != 0x30 {
                // CertIDがない場合は次へ
                content_offset = resp_content_end;
                continue;
            }
            
            // CertIDの長さを取得
            let (cert_id_len, cert_id_len_bytes) = asn1_rs::der::parse_der_length(&resp_content[1..])
                .map_err(|e| anyhow!("CertID長解析エラー: {}", e))?;
            
            // CertIDの内容を取得
            let cert_id_content_start = 1 + cert_id_len_bytes;
            let cert_id_content_end = cert_id_content_start + cert_id_len;
            let cert_id_content = &resp_content[cert_id_content_start..cert_id_content_end];
            
            // シリアル番号を検索
            let mut cert_id_offset = 0;
            
            // hashAlgorithm AlgorithmIdentifier をスキップ
            if cert_id_content[cert_id_offset] == 0x30 {
                let (alg_len, alg_len_bytes) = asn1_rs::der::parse_der_length(&cert_id_content[cert_id_offset+1..])
                    .map_err(|e| anyhow!("AlgorithmIdentifier長解析エラー: {}", e))?;
                cert_id_offset += 1 + alg_len_bytes + alg_len;
            }
            
            // issuerNameHash OCTET STRING をスキップ
            if cert_id_content[cert_id_offset] == 0x04 {
                let name_hash_len = cert_id_content[cert_id_offset+1] as usize;
                cert_id_offset += 2 + name_hash_len;
            }
            
            // issuerKeyHash OCTET STRING をスキップ
            if cert_id_content[cert_id_offset] == 0x04 {
                let key_hash_len = cert_id_content[cert_id_offset+1] as usize;
                cert_id_offset += 2 + key_hash_len;
            }
            
            // serialNumber CertificateSerialNumber
            if cert_id_content[cert_id_offset] == 0x02 {
                let serial_len = cert_id_content[cert_id_offset+1] as usize;
                let serial_start = cert_id_offset + 2;
                let serial_end = serial_start + serial_len;
                let serial = &cert_id_content[serial_start..serial_end];
                
                // シリアル番号が一致するか確認
                if serial == cert_serial {
                    // シリアル番号が一致したので、証明書ステータスを取得
                    debug!("一致するシリアル番号を発見: {:?}", serial);
                    
                    // CertStatusは SingleResponse 内の2番目のフィールド
                    let cert_status_offset = cert_id_content_end;
                    
                    // CertStatus は次のいずれかのタグを持つ:
                    // good [0] IMPLICIT NULL
                    // revoked [1] IMPLICIT RevokedInfo
                    // unknown [2] IMPLICIT UnknownInfo
                    let status_tag = resp_content[cert_status_offset];
                    
                    let status = match status_tag {
                        0x80 => 0, // good
                        0x81 => 1, // revoked
                        0x82 => 2, // unknown
                        _ => {
                            warn!("未知の証明書ステータスタグ: 0x{:02x}", status_tag);
                            2 // 不明なタグは unknown として扱う
                        }
                    };
                    
                    debug!("証明書ステータス: {}", match status {
                        0 => "good",
                        1 => "revoked",
                        2 => "unknown",
                        _ => "invalid",
                    });
                    
                    return Ok(status);
                }
            }
            
            // 次のSingleResponseへ
            content_offset = resp_content_end;
        }
        
        // 対象の証明書が見つからなかった
        Err(anyhow!("指定されたシリアル番号 {:?} の証明書ステータスが見つかりません", cert_serial))
    }
    
    /// ノードに対して期待されるユーザー名を取得
    async fn get_expected_username_for_node(&self) -> Result<String, anyhow::Error> {
        // データベースからノードに対する期待ユーザー名を取得
        // ここではダミー実装
        Ok("system".to_string())
    }
    
    /// 信頼できる発行者のリストを取得
    async fn get_trusted_issuers(&self) -> Result<Vec<String>, anyhow::Error> {
        // データベースから信頼できる発行者リストを取得
        match self.db_client.query("SELECT issuer FROM trusted_issuers WHERE is_active = true").await {
            Ok(rows) => {
                rows.iter()
                    .filter_map(|row| row.get::<&str, String>("issuer").ok())
                    .collect()
            },
            Err(err) => {
                error!("発行者リスト取得エラー: {}", err);
                Ok(Vec::new()) // エラー時は空リストを返す
            }
        }
    }
    
    /// 信頼できるフィンガープリントのリストを取得
    async fn get_trusted_fingerprints(&self) -> Result<Vec<String>, anyhow::Error> {
        // データベースから信頼できるフィンガープリントリストを取得
        match self.db_client.query("SELECT fingerprint FROM trusted_fingerprints WHERE is_active = true").await {
            Ok(rows) => {
                rows.iter()
                    .filter_map(|row| row.get::<&str, String>("fingerprint").ok())
                    .collect()
            },
            Err(err) => {
                error!("フィンガープリントリスト取得エラー: {}", err);
                Ok(Vec::new()) // エラー時は空リストを返す
            }
        }
    }
    
    /// トークンを検証
    async fn validate_token(&self, token: &str, node_id: &NodeId) -> bool {
        let tokens = self.valid_tokens.read().await;
        
        if let Some((stored_node_id, expiry)) = tokens.get(token) {
            // ノードIDが一致するか確認
            if stored_node_id != node_id {
                return false;
            }
            
            // トークンが有効期限内か確認
            *expiry > Utc::now()
        } else {
            false
        }
    }
    
    /// メッセージを暗号化
    pub fn encrypt_message(&self, plaintext: &[u8], receiver_public_key: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::{
            aead::{Aead, AeadCore, KeyInit, OsRng},
            Aes256Gcm, Nonce
        };
        use x25519_dalek::{EphemeralSecret, PublicKey};
        use hkdf::Hkdf;
        use sha2::Sha256;
        use chacha20poly1305::ChaCha20Poly1305;
        
        debug!("メッセージ暗号化を開始 ({} バイト)", plaintext.len());

        // コンフィグから暗号化アルゴリズムを取得
        let encryption_algorithm = self.config.encryption_algorithm.clone().unwrap_or_else(|| {
            // デフォルトはハードウェア性能に基づいて自動選択
            if is_aes_hardware_accelerated() {
                "AES-256-GCM".to_string()
            } else {
                "CHACHA20-POLY1305".to_string()
            }
        });
        
        debug!("暗号化アルゴリズム: {}", encryption_algorithm);
        
        // 受信者の公開鍵を解析
        let receiver_key = match PublicKey::from_bytes(receiver_public_key) {
            Ok(key) => key,
            Err(e) => return Err(anyhow!("無効な受信者公開鍵: {}", e)),
        };
        
        // 一時的なECDH鍵ペアを生成
        let ephemeral_secret = EphemeralSecret::random_from_rng(OsRng);
        let ephemeral_public = PublicKey::from(&ephemeral_secret);
        
        // 共有秘密を計算
        let shared_secret = ephemeral_secret.diffie_hellman(&receiver_key);
        
        // 共有秘密から暗号化キーを導出 (HKDF)
        let mut encryption_key = [0u8; 32];
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
        hkdf.expand(b"NexusShell-AEADKey-v1", &mut encryption_key)
            .map_err(|e| anyhow!("キー導出エラー: {}", e))?;
        
        // ランダムなnonce（初期化ベクトル）を生成
        let nonce = if encryption_algorithm == "AES-256-GCM" {
            Aes256Gcm::generate_nonce(&mut OsRng)
        } else {
            ChaCha20Poly1305::generate_nonce(&mut OsRng)
        };
        
        // 暗号化アルゴリズムに基づいて平文を暗号化
        let ciphertext = if encryption_algorithm == "AES-256-GCM" {
            // AES-GCMを使用
            let cipher = Aes256Gcm::new_from_slice(&encryption_key)
                .map_err(|e| anyhow!("AESキー初期化エラー: {}", e))?;
            
            cipher.encrypt(&nonce, plaintext)
                .map_err(|e| anyhow!("AES-GCM暗号化エラー: {}", e))?
        } else {
            // ChaCha20-Poly1305を使用
            let cipher = ChaCha20Poly1305::new_from_slice(&encryption_key)
                .map_err(|e| anyhow!("ChaCha20Poly1305キー初期化エラー: {}", e))?;
            
            cipher.encrypt(&nonce, plaintext)
                .map_err(|e| anyhow!("ChaCha20Poly1305暗号化エラー: {}", e))?
        };
        
        // 暗号化アルゴリズムのIDを1バイトで表現
        let algorithm_id = if encryption_algorithm == "AES-256-GCM" { 1u8 } else { 2u8 };
        
        // 最終的な暗号文を構築
        // フォーマット: version(1) || algorithm_id(1) || ephemeral_public_key(32) || nonce(12) || ciphertext
        let mut result = Vec::with_capacity(1 + 1 + 32 + 12 + ciphertext.len());
        result.push(1); // プロトコルバージョン1
        result.push(algorithm_id);
        result.extend_from_slice(ephemeral_public.as_bytes());
        result.extend_from_slice(&nonce);
        result.extend_from_slice(&ciphertext);
        
        // 暗号化データの整合性ハッシュを追加（オプション）
        if self.config.add_integrity_hash {
            let mut hasher = Sha256::new();
            hasher.update(&result);
            let hash = hasher.finalize();
            result.extend_from_slice(&hash);
        }
        
        debug!("メッセージ暗号化完了 ({} → {} バイト)", plaintext.len(), result.len());
        Ok(result)
    }
    
    /// メッセージを復号
    pub fn decrypt_message(&self, ciphertext: &[u8], private_key: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce
        };
        use x25519_dalek::{StaticSecret, PublicKey};
        use hkdf::Hkdf;
        use sha2::Sha256;
        use chacha20poly1305::ChaCha20Poly1305;
        
        debug!("メッセージ復号を開始 ({} バイト)", ciphertext.len());
        
        // フォーマットバージョンをチェック
        if ciphertext.is_empty() {
            return Err(anyhow!("空の暗号文"));
        }
        
        let version = ciphertext[0];
        if version != 1 {
            return Err(anyhow!("未サポートの暗号化バージョン: {}", version));
        }
        
        // 暗号化アルゴリズムIDを取得
        if ciphertext.len() < 2 {
            return Err(anyhow!("無効な暗号文形式: アルゴリズムIDがありません"));
        }
        
        let algorithm_id = ciphertext[1];
        let algorithm_name = match algorithm_id {
            1 => "AES-256-GCM",
            2 => "CHACHA20-POLY1305",
            _ => return Err(anyhow!("未サポートの暗号化アルゴリズムID: {}", algorithm_id)),
        };
        
        debug!("復号アルゴリズム: {}", algorithm_name);
        
        // バージョン情報とアルゴリズムIDを除去
        let ciphertext = &ciphertext[2..];
        
        // 最小長をチェック（32バイトの公開鍵 + 12バイトのnonce + 最低16バイトの暗号文）
        if ciphertext.len() < 32 + 12 + 16 {
            return Err(anyhow!("無効な暗号文長: {} バイト（最小60バイト必要）", ciphertext.len()));
        }
        
        // 各部分を取り出す
        let ephemeral_public_bytes = &ciphertext[0..32];
        let nonce_bytes = &ciphertext[32..44];
        let actual_ciphertext = &ciphertext[44..];
        
        // 一時公開鍵を解析
        let ephemeral_public = match PublicKey::from_bytes(ephemeral_public_bytes) {
            Ok(key) => key,
            Err(e) => return Err(anyhow!("無効な一時公開鍵: {}", e)),
        };
        
        // 秘密鍵を解析
        let private_key_array: [u8; 32] = private_key.try_into()
            .map_err(|_| anyhow!("無効な秘密鍵長: {} バイト（32バイト必要）", private_key.len()))?;
            
        let secret_key = StaticSecret::from(private_key_array);
        
        // 共有秘密を計算
        let shared_secret = secret_key.diffie_hellman(&ephemeral_public);
        
        // 共有秘密から暗号化キーを導出 (HKDF)
        let mut encryption_key = [0u8; 32];
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
        hkdf.expand(b"NexusShell-AEADKey-v1", &mut encryption_key)
            .map_err(|e| anyhow!("キー導出エラー: {}", e))?;
        
        // Nonceを取得
        let nonce = Nonce::from_slice(nonce_bytes);
        
        // 暗号化アルゴリズムに基づいて復号
        let plaintext = if algorithm_id == 1 {
            // AES-GCMを使用
            let cipher = Aes256Gcm::new_from_slice(&encryption_key)
                .map_err(|e| anyhow!("AESキー初期化エラー: {}", e))?;
                
            cipher.decrypt(nonce, actual_ciphertext)
                .map_err(|e| anyhow!("AES-GCM復号エラー: {}", e))?
        } else {
            // ChaCha20-Poly1305を使用
            let cipher = ChaCha20Poly1305::new_from_slice(&encryption_key)
                .map_err(|e| anyhow!("ChaCha20Poly1305キー初期化エラー: {}", e))?;
                
            cipher.decrypt(nonce, actual_ciphertext)
                .map_err(|e| anyhow!("ChaCha20Poly1305復号エラー: {}", e))?
        };
        
        debug!("メッセージ復号完了 ({} → {} バイト)", ciphertext.len(), plaintext.len());
        Ok(plaintext)
    }
    
    /// CPUがAES命令セットをサポートしているか確認
    fn is_aes_hardware_accelerated() -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            use std::arch::is_x86_feature_detected;
            is_x86_feature_detected!("aes") && is_x86_feature_detected!("sse2")
        }
        #[cfg(target_arch = "aarch64")]
        {
            std::arch::is_aarch64_feature_detected!("aes")
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
        {
            false
        }
    }
    
    /// メッセージに署名
    pub fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>> {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;
        
        // 署名鍵がなければ作成
        let signing_key = if let Some(key) = &self.signing_key {
            key.clone()
        } else {
            // 新しい署名キーペアを生成
            let mut csprng = OsRng {};
            let signing_key = SigningKey::generate(&mut csprng);
            
            // 鍵を安全に保存
            let keystore = KeyStore::from_env().map_err(|e| {
                error!("キーストア初期化エラー: {}", e);
                DistributedError::SecurityError(format!("キーストア初期化失敗: {}", e))
            })?;
            
            // 署名鍵をセキュアに保存
            keystore.store_key("pipeline_signing_key", &signing_key, true).map_err(|e| {
                error!("署名鍵保存エラー: {}", e);
                DistributedError::SecurityError(format!("署名鍵の保存に失敗: {}", e))
            })?;
            
            // 保存した鍵への参照を返す
            keystore.get_key_reference("pipeline_signing_key")
        };
        
        // メッセージに署名
        let signature = signing_key.sign(message);
        
        // 署名を返す（64バイト）
        Ok(signature.to_bytes().to_vec())
    }
    
    /// 署名を検証
    pub fn verify_signature(&self, message: &[u8], signature: &[u8], public_key: &[u8]) -> Result<bool> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        
        // 署名が正しいサイズか確認
        if signature.len() != 64 {
            return Err(anyhow!("無効な署名サイズ: {} バイト (予想: 64)", signature.len()));
        }
        
        // 公開鍵が正しいサイズか確認
        if public_key.len() != 32 {
            return Err(anyhow!("無効な公開鍵サイズ: {} バイト (予想: 32)", public_key.len()));
        }
        
        // 署名を解析
        let sig = Signature::from_bytes(signature.try_into()
            .map_err(|_| anyhow!("署名のバイト配列変換に失敗"))?);
        
        // 公開鍵を解析
        let vk = VerifyingKey::from_bytes(public_key.try_into()
            .map_err(|_| anyhow!("公開鍵のバイト配列変換に失敗"))?)?;
        
        // 署名を検証
        match vk.verify(message, &sig) {
            Ok(_) => Ok(true),   // 署名は有効
            Err(_) => Ok(false), // 署名は無効
        }
    }
    
    /// アクセス制御エントリを追加
    pub async fn add_access_control_entry(&self, entry: AccessControlEntry) -> Result<()> {
        let mut acl = self.access_control_list.write().await;
        acl.push(entry);
        Ok(())
    }
    
    /// アクセス権限をチェック
    pub async fn check_permission(
        &self,
        node_id: &NodeId,
        resource_type: ResourceType,
        resource_id: &str,
        permission: Permission,
    ) -> Result<bool> {
        // 認証が無効な場合は常に許可
        if !self.config.enable_authentication {
            return Ok(true);
        }
        
        // ノードの認証状態を確認
        let node_state = {
            let states = self.node_auth_states.read().await;
            states.get(node_id).cloned()
        };
        
        let node_state = match node_state {
            Some(state) if state.authenticated => state,
            _ => return Ok(false), // 認証されていない
        };
        
        // アクセス制御ポリシーに基づいて権限を確認
        match self.config.security_policy.access_control {
            AccessControlPolicy::None => {
                // アクセス制御なし
                Ok(true)
            },
            AccessControlPolicy::Simple => {
                // シンプルなアクセス制御（管理者は全権限、それ以外は読み取りのみ）
                if node_state.roles.contains(&"admin".to_string()) {
                    Ok(true)
                } else {
                    Ok(permission == Permission::Read)
                }
            },
            AccessControlPolicy::RoleBased | AccessControlPolicy::AclBased => {
                // ロールベースまたはACLベースのアクセス制御
                let acl = self.access_control_list.read().await;
                
                // ノードのロールに基づいてアクセス権限を確認
                for role in &node_state.roles {
                    for entry in acl.iter() {
                        if entry.role == *role &&
                           entry.resource_type == resource_type &&
                           (entry.resource_id == "*" || entry.resource_id == resource_id) &&
                           entry.permissions.contains(&permission) {
                            return Ok(true);
                        }
                    }
                }
                
                // 権限なし
                Ok(false)
            },
        }
    }
    
    /// ユーザーを追加
    pub async fn add_user(&self, username: &str, password: &str) -> Result<()> {
        // パスワードの複雑さを確認
        if !check_password_complexity(password, &self.config.security_policy.password_complexity) {
            return Err(anyhow!("パスワードは複雑さ要件を満たしていません"));
        }
        
        // パスワードをハッシュ化
        let hashed_password = hash_password(password)?;
        
        // ユーザーを追加
        let mut credentials = self.credentials.write().await;
        credentials.insert(username.to_string(), hashed_password);
        
        Ok(())
    }
    
    /// ノードの認証状態を取得
    pub async fn get_node_auth_state(&self, node_id: &NodeId) -> Option<NodeAuthState> {
        let states = self.node_auth_states.read().await;
        states.get(node_id).cloned()
    }
    
    /// ノードの認証状態をリセット
    pub async fn reset_node_auth_state(&self, node_id: &NodeId) -> Result<()> {
        let mut states = self.node_auth_states.write().await;
        
        if let Some(state) = states.get_mut(node_id) {
            state.invalidate();
            state.auth_failures = 0;
            Ok(())
        } else {
            Err(anyhow!("ノード {} の認証状態が見つかりません", node_id))
        }
    }
    
    /// セッショントークンを無効化
    pub async fn invalidate_token(&self, token: &str) -> Result<()> {
        let mut tokens = self.valid_tokens.write().await;
        
        if tokens.remove(token).is_some() {
            Ok(())
        } else {
            Err(anyhow!("トークン {} が見つかりません", token))
        }
    }
    
    /// 期限切れトークンをクリーンアップ
    pub async fn cleanup_expired_tokens(&self) -> Result<usize> {
        let now = Utc::now();
        let mut tokens = self.valid_tokens.write().await;
        
        let initial_count = tokens.len();
        tokens.retain(|_, (_, expiry)| *expiry > now);
        
        let removed = initial_count - tokens.len();
        debug!("{} 個の期限切れトークンをクリーンアップしました", removed);
        
        Ok(removed)
    }
}

/// ノンスを生成
fn generate_nonce() -> String {
    Uuid::new_v4().to_string()
}

/// トークンを生成
fn generate_token() -> String {
    Uuid::new_v4().to_string()
}

/// パスワードをハッシュ化
fn hash_password(password: &str) -> Result<String> {
    // Argon2idを使用した安全なパスワードハッシュ
    use argon2::{
        password_hash::{
            rand_core::OsRng,
            PasswordHasher, SaltString
        },
        Argon2, Algorithm, Version, Params
    };
    
    // ランダムなソルトを生成
    let salt = SaltString::generate(&mut OsRng);
    
    // Argon2idインスタンスを設定（メモリ=64MB、反復=3、並列度=4）
    let argon2 = Argon2::new(
        Algorithm::Argon2id,  // Argon2idバリアント（メモリハードさとサイドチャネル攻撃保護を両立）
        Version::V0x13,       // バージョン1.3
        Params::new(            
            64 * 1024,        // メモリコスト（KiB）
            3,                // 反復回数
            4,                // 並列度（スレッド数）
            Some(64)          // 出力長（バイト）
        ).map_err(|e| anyhow!("Argon2パラメータエラー: {}", e))?
    );
    
    // パスワードをハッシュ化
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("パスワードハッシュ化エラー: {}", e))?
        .to_string();
    
    Ok(password_hash)
}

/// パスワードを検証
fn verify_password(password: &str, hashed: &str) -> bool {
    use argon2::{
        password_hash::{
            PasswordHash, PasswordVerifier
        },
        Argon2
    };
    
    // ハッシュ文字列を解析
    let parsed_hash = match PasswordHash::new(hashed) {
        Ok(hash) => hash,
        Err(_) => return false,
    };
    
    // Argon2インスタンスを作成（デフォルト設定）
    let argon2 = Argon2::default();
    
    // パスワードを検証
    argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok()
}

/// パスワードの複雑さをチェック
fn check_password_complexity(password: &str, complexity: &PasswordComplexity) -> bool {
    // 文字種別のカウント
    let has_lowercase = password.chars().any(|c| c.is_lowercase());
    let has_uppercase = password.chars().any(|c| c.is_uppercase());
    let has_digit = password.chars().any(|c| c.is_digit(10));
    let has_special = password.chars().any(|c| !c.is_alphanumeric());
    
    let char_types = [has_lowercase, has_uppercase, has_digit, has_special]
        .iter()
        .filter(|&&b| b)
        .count();
    
    match complexity {
        PasswordComplexity::Basic => true,
        PasswordComplexity::Medium => char_types >= 2,
        PasswordComplexity::Strong => char_types >= 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_password_complexity() {
        // 小文字のみ
        assert!(check_password_complexity("password", &PasswordComplexity::Basic));
        assert!(!check_password_complexity("password", &PasswordComplexity::Medium));
        assert!(!check_password_complexity("password", &PasswordComplexity::Strong));
        
        // 小文字+大文字
        assert!(check_password_complexity("Password", &PasswordComplexity::Basic));
        assert!(check_password_complexity("Password", &PasswordComplexity::Medium));
        assert!(!check_password_complexity("Password", &PasswordComplexity::Strong));
        
        // 小文字+大文字+数字
        assert!(check_password_complexity("Password123", &PasswordComplexity::Basic));
        assert!(check_password_complexity("Password123", &PasswordComplexity::Medium));
        assert!(check_password_complexity("Password123", &PasswordComplexity::Strong));
        
        // 小文字+大文字+数字+特殊文字
        assert!(check_password_complexity("Password123!", &PasswordComplexity::Basic));
        assert!(check_password_complexity("Password123!", &PasswordComplexity::Medium));
        assert!(check_password_complexity("Password123!", &PasswordComplexity::Strong));
    }
    
    #[test]
    fn test_hash_and_verify_password() {
        let password = "SecurePassword123!";
        
        // パスワードをハッシュ化
        let hashed = hash_password(password).unwrap();
        
        // 正しいパスワードで検証
        assert!(verify_password(password, &hashed));
        
        // 誤ったパスワードで検証
        assert!(!verify_password("WrongPassword", &hashed));
    }
    
    #[tokio::test]
    async fn test_security_manager() {
        // セキュリティ設定
        let config = SecurityConfig::default();
        
        // 通信マネージャーのモック
        let local_node_id = NodeId::from_string("local-node".to_string());
        
        // セキュリティマネージャーを作成
        let security_manager = SecurityManager::new(
            local_node_id.clone(),
            Arc::new(mock_communication_manager()),
            config,
        );
        
        // ユーザーを追加
        security_manager.add_user("testuser", "Password123!").await.unwrap();
        
        // 認証情報を作成
        let credentials = AuthCredentials {
            node_id: "test-node".to_string(),
            method: AuthMethod::Password,
            username: Some("testuser".to_string()),
            password: Some("Password123!".to_string()),
            cert_fingerprint: None,
            token: None,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            nonce: generate_nonce(),
        };
        
        // ノードを認証
        let node_id = NodeId::from_string("test-node".to_string());
        let result = security_manager.authenticate_node(&node_id, &credentials).await.unwrap();
        
        // 認証成功を確認
        assert!(result.success);
        assert!(result.session_token.is_some());
        
        // 誤ったパスワードで認証
        let wrong_credentials = AuthCredentials {
            node_id: "test-node".to_string(),
            method: AuthMethod::Password,
            username: Some("testuser".to_string()),
            password: Some("WrongPassword".to_string()),
            cert_fingerprint: None,
            token: None,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            nonce: generate_nonce(),
        };
        
        let result = security_manager.authenticate_node(&node_id, &wrong_credentials).await.unwrap();
        
        // 認証失敗を確認
        assert!(!result.success);
        assert!(result.session_token.is_none());
    }
    
    // 通信マネージャーのモック
    fn mock_communication_manager() -> CommunicationManager {
        struct MockCommunicationManager;
        
        #[async_trait]
        impl CommunicationManager {
            pub async fn send_message(&self, _message: DistributedMessage) -> Result<()> {
                Ok(())
            }
            
            pub async fn send_to_node(
                &self,
                _recipient: &NodeId,
                _message_type: MessageType,
                _payload: Vec<u8>,
            ) -> Result<super::communication::MessageId> {
                Ok(super::communication::MessageId::from_string("mock-id".to_string()))
            }
        }
        
        MockCommunicationManager
    }
} 