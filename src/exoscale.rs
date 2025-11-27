//! Exoscale API client for managing IAM roles and API keys.

use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

#[derive(Error, Debug)]
pub enum ExoscaleError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("API error: {message} (status: {status})")]
    Api { status: u16, message: String },

    #[error("Signature error: {0}")]
    Signature(String),
}

pub type Result<T> = std::result::Result<T, ExoscaleError>;

/// Exoscale API client
pub struct Client {
    http: reqwest::Client,
    api_key: String,
    api_secret: String,
    api_base: String,
}

// API Response types

#[derive(Debug, Deserialize)]
pub struct IamRole {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Response from async operations like create-iam-role
#[derive(Debug, Deserialize)]
pub struct OperationResponse {
    pub id: String,
    pub state: String,
    pub reference: Option<OperationReference>,
}

#[derive(Debug, Deserialize)]
pub struct OperationReference {
    pub id: String,
    pub link: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IamRolesResponse {
    pub iam_roles: Vec<IamRole>,
}

#[derive(Debug, Deserialize)]
pub struct ApiKey {
    pub name: String,
    pub key: String,
    pub secret: Option<String>,
    #[serde(rename = "role-id")]
    pub role_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ApiKeysResponse {
    pub api_keys: Vec<ApiKey>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiError {
    message: Option<String>,
}

// Request payloads

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CreateRoleRequest {
    name: String,
    description: String,
    editable: bool,
    policy: RolePolicy,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct RolePolicy {
    default_service_strategy: String,
    services: RolePolicyServices,
}

#[derive(Serialize)]
struct RolePolicyServices {
    sos: ServicePolicy,
}

#[derive(Serialize)]
struct ServicePolicy {
    #[serde(rename = "type")]
    policy_type: String,
    rules: Option<Vec<PolicyRule>>,
}

#[derive(Serialize)]
struct PolicyRule {
    action: String,
    expression: String,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CreateApiKeyRequest {
    name: String,
    role_id: String,
}

impl Client {
    /// Create a new Exoscale API client.
    pub fn new(api_key: String, api_secret: String, zone: &str) -> Self {
        let http = reqwest::Client::new();
        // Base URL without /v2 - we add it to each path for signing
        let api_base = format!("https://api-{}.exoscale.com", zone);
        Self {
            http,
            api_key,
            api_secret,
            api_base,
        }
    }

    /// Generate the EXO2-HMAC-SHA256 authorization header.
    fn sign_request(&self, method: &str, path: &str, body: &str) -> Result<String> {
        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ExoscaleError::Signature(e.to_string()))?
            .as_secs()
            + 600; // 10 minutes from now

        // Message format: 5 parts joined by newlines:
        // 1. "{method} {path}"
        // 2. body (or empty)
        // 3. query params (empty for us)
        // 4. headers (empty for us)
        // 5. expires timestamp
        let message = format!("{} {}\n{}\n\n\n{}", method, path, body, expires);

        let mut mac = HmacSha256::new_from_slice(self.api_secret.as_bytes())
            .map_err(|e| ExoscaleError::Signature(e.to_string()))?;
        mac.update(message.as_bytes());
        let signature = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            mac.finalize().into_bytes(),
        );

        Ok(format!(
            "EXO2-HMAC-SHA256 credential={},expires={},signature={}",
            self.api_key, expires, signature
        ))
    }

    fn headers(&self, auth: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(auth).expect("Invalid auth header"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }

    async fn check_response(&self, response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            Ok(response)
        } else {
            let body = response.text().await.unwrap_or_default();
            let message = if let Ok(error) = serde_json::from_str::<ApiError>(&body) {
                error.message.unwrap_or(body)
            } else {
                body
            };
            Err(ExoscaleError::Api {
                status: status.as_u16(),
                message,
            })
        }
    }

    /// Create an IAM role with SOS access scoped to a bucket prefix.
    pub async fn create_role(
        &self,
        name: &str,
        description: &str,
        bucket: &str,
        prefix: &str,
    ) -> Result<IamRole> {
        let path = "/v2/iam-role";
        let url = format!("{}{}", self.api_base, path);

        // Create policy that only allows SOS operations on specific bucket/prefix
        // Operations: get-object, put-object, delete-object, head-object, list-objects
        // Resources: resources.bucket for bucket, parameters.key for object key
        let payload = CreateRoleRequest {
            name: name.to_string(),
            description: description.to_string(),
            editable: false,
            policy: RolePolicy {
                default_service_strategy: "deny".to_string(),
                services: RolePolicyServices {
                    sos: ServicePolicy {
                        policy_type: "rules".to_string(),
                        rules: Some(vec![
                            // Allow listing objects in the bucket (needed for navigation)
                            PolicyRule {
                                action: "allow".to_string(),
                                expression: format!(
                                    "operation == 'list-objects' && resources.bucket == '{}'",
                                    bucket
                                ),
                            },
                            // Allow object operations only on the app's prefix
                            PolicyRule {
                                action: "allow".to_string(),
                                expression: format!(
                                    "operation in ['get-object', 'put-object', 'delete-object', 'head-object'] && resources.bucket == '{}' && parameters.key.startsWith('{}')",
                                    bucket, prefix
                                ),
                            },
                        ]),
                    },
                },
            },
        };

        let body = serde_json::to_string(&payload)
            .map_err(|e| ExoscaleError::Signature(e.to_string()))?;

        let auth = self.sign_request("POST", path, &body)?;

        let response = self
            .http
            .post(&url)
            .headers(self.headers(&auth))
            .body(body)
            .send()
            .await?;

        let response = self.check_response(response).await?;
        let body = response.text().await?;

        // Parse the async operation response and extract the actual role ID from reference
        let op: OperationResponse = serde_json::from_str(&body)
            .map_err(|e| ExoscaleError::Signature(format!("Failed to parse operation response: {}", e)))?;

        let role_id = op.reference
            .ok_or_else(|| ExoscaleError::Api {
                status: 500,
                message: "No reference in operation response".to_string(),
            })?
            .id;

        Ok(IamRole {
            id: role_id,
            name: Some(name.to_string()),
            description: Some(description.to_string()),
        })
    }

    /// List all IAM roles.
    pub async fn list_roles(&self) -> Result<Vec<IamRole>> {
        let path = "/v2/iam-role";
        let url = format!("{}{}", self.api_base, path);

        let auth = self.sign_request("GET", path, "")?;

        let response = self.http.get(&url).headers(self.headers(&auth)).send().await?;

        let response = self.check_response(response).await?;
        let roles: IamRolesResponse = response.json().await?;
        Ok(roles.iam_roles)
    }

    /// Delete an IAM role.
    pub async fn delete_role(&self, role_id: &str) -> Result<()> {
        let path = format!("/v2/iam-role/{}", role_id);
        let url = format!("{}{}", self.api_base, path);

        let auth = self.sign_request("DELETE", &path, "")?;

        let response = self
            .http
            .delete(&url)
            .headers(self.headers(&auth))
            .send()
            .await?;

        self.check_response(response).await?;
        Ok(())
    }

    /// Create an API key attached to a role.
    pub async fn create_api_key(&self, name: &str, role_id: &str) -> Result<ApiKey> {
        let path = "/v2/api-key";
        let url = format!("{}{}", self.api_base, path);

        let payload = CreateApiKeyRequest {
            name: name.to_string(),
            role_id: role_id.to_string(),
        };

        let body = serde_json::to_string(&payload)
            .map_err(|e| ExoscaleError::Signature(e.to_string()))?;

        let auth = self.sign_request("POST", path, &body)?;

        let response = self
            .http
            .post(&url)
            .headers(self.headers(&auth))
            .body(body)
            .send()
            .await?;

        let response = self.check_response(response).await?;
        let api_key: ApiKey = response.json().await?;
        Ok(api_key)
    }

    /// List all API keys.
    pub async fn list_api_keys(&self) -> Result<Vec<ApiKey>> {
        let path = "/v2/api-key";
        let url = format!("{}{}", self.api_base, path);

        let auth = self.sign_request("GET", path, "")?;

        let response = self.http.get(&url).headers(self.headers(&auth)).send().await?;

        let response = self.check_response(response).await?;
        let keys: ApiKeysResponse = response.json().await?;
        Ok(keys.api_keys)
    }

    /// Delete an API key.
    pub async fn delete_api_key(&self, key: &str) -> Result<()> {
        let path = format!("/v2/api-key/{}", key);
        let url = format!("{}{}", self.api_base, path);

        let auth = self.sign_request("DELETE", &path, "")?;

        let response = self
            .http
            .delete(&url)
            .headers(self.headers(&auth))
            .send()
            .await?;

        self.check_response(response).await?;
        Ok(())
    }
}
