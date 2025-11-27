//! Scaleway IAM API client for managing applications, policies, and API keys.

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const IAM_API_BASE: &str = "https://api.scaleway.com/iam/v1alpha1";

#[derive(Error, Debug)]
pub enum ScalewayError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("API error: {message} (status: {status})")]
    Api { status: u16, message: String },

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

pub type Result<T> = std::result::Result<T, ScalewayError>;

/// Scaleway IAM API client
pub struct Client {
    http: reqwest::Client,
    secret_key: String,
}

// API Response types

#[derive(Debug, Deserialize)]
pub struct Application {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub organization_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApplicationsResponse {
    applications: Vec<Application>,
}

#[derive(Debug, Deserialize)]
pub struct Policy {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PoliciesResponse {
    policies: Vec<Policy>,
}

#[derive(Debug, Deserialize)]
pub struct ApiKey {
    pub access_key: String,
    pub secret_key: Option<String>,
    pub application_id: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiKeysResponse {
    api_keys: Vec<ApiKey>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiError {
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

// Request payloads

#[derive(Serialize)]
struct CreateApplicationRequest<'a> {
    name: &'a str,
    description: &'a str,
    organization_id: &'a str,
}

#[derive(Serialize)]
struct CreatePolicyRequest<'a> {
    name: &'a str,
    organization_id: &'a str,
    application_id: &'a str,
    rules: Vec<PolicyRule<'a>>,
}

#[derive(Serialize)]
struct PolicyRule<'a> {
    project_ids: Vec<&'a str>,
    permission_set_names: Vec<&'a str>,
}

#[derive(Serialize)]
struct CreateApiKeyRequest<'a> {
    application_id: &'a str,
    description: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_project_id: Option<&'a str>,
}

impl Client {
    /// Create a new Scaleway API client with the given secret key.
    pub fn new(secret_key: String) -> Self {
        let http = reqwest::Client::new();
        Self { http, secret_key }
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Auth-Token",
            HeaderValue::from_str(&self.secret_key).expect("Invalid secret key"),
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
            Err(ScalewayError::Api {
                status: status.as_u16(),
                message,
            })
        }
    }

    /// Create a new IAM application.
    pub async fn create_application(
        &self,
        name: &str,
        description: &str,
        organization_id: &str,
    ) -> Result<Application> {
        let url = format!("{}/applications", IAM_API_BASE);
        let payload = CreateApplicationRequest {
            name,
            description,
            organization_id,
        };

        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&payload)
            .send()
            .await?;

        let response = self.check_response(response).await?;
        let app: Application = response.json().await?;
        Ok(app)
    }

    /// List all applications in an organization.
    pub async fn list_applications(&self, organization_id: &str) -> Result<Vec<Application>> {
        let url = format!(
            "{}/applications?organization_id={}",
            IAM_API_BASE, organization_id
        );

        let response = self.http.get(&url).headers(self.headers()).send().await?;

        let response = self.check_response(response).await?;
        let apps: ApplicationsResponse = response.json().await?;
        Ok(apps.applications)
    }

    /// Delete an application.
    pub async fn delete_application(&self, application_id: &str) -> Result<()> {
        let url = format!("{}/applications/{}", IAM_API_BASE, application_id);

        let response = self
            .http
            .delete(&url)
            .headers(self.headers())
            .send()
            .await?;

        self.check_response(response).await?;
        Ok(())
    }

    /// Create a policy with scoped Object Storage permissions for a project.
    /// Uses ObjectStorageObjectsRead, ObjectStorageObjectsWrite, ObjectStorageObjectsDelete
    /// instead of ObjectStorageFullAccess for better security.
    pub async fn create_policy(
        &self,
        name: &str,
        application_id: &str,
        organization_id: &str,
        project_id: &str,
    ) -> Result<Policy> {
        let url = format!("{}/policies", IAM_API_BASE);
        let payload = CreatePolicyRequest {
            name,
            organization_id,
            application_id,
            rules: vec![PolicyRule {
                project_ids: vec![project_id],
                // Scoped permissions - only object operations, not bucket management
                permission_set_names: vec![
                    "ObjectStorageObjectsRead",
                    "ObjectStorageObjectsWrite",
                    "ObjectStorageObjectsDelete",
                ],
            }],
        };

        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&payload)
            .send()
            .await?;

        let response = self.check_response(response).await?;
        let policy: Policy = response.json().await?;
        Ok(policy)
    }

    /// List policies for an application.
    pub async fn list_policies(&self, application_id: &str) -> Result<Vec<Policy>> {
        let url = format!("{}/policies?application_id={}", IAM_API_BASE, application_id);

        let response = self.http.get(&url).headers(self.headers()).send().await?;

        let response = self.check_response(response).await?;
        let policies: PoliciesResponse = response.json().await?;
        Ok(policies.policies)
    }

    /// Delete a policy.
    pub async fn delete_policy(&self, policy_id: &str) -> Result<()> {
        let url = format!("{}/policies/{}", IAM_API_BASE, policy_id);

        let response = self
            .http
            .delete(&url)
            .headers(self.headers())
            .send()
            .await?;

        self.check_response(response).await?;
        Ok(())
    }

    /// Create an API key for an application.
    pub async fn create_api_key(
        &self,
        application_id: &str,
        description: &str,
        default_project_id: Option<&str>,
    ) -> Result<ApiKey> {
        let url = format!("{}/api-keys", IAM_API_BASE);
        let payload = CreateApiKeyRequest {
            application_id,
            description,
            default_project_id,
        };

        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&payload)
            .send()
            .await?;

        let response = self.check_response(response).await?;
        let api_key: ApiKey = response.json().await?;
        Ok(api_key)
    }

    /// List API keys for an application.
    pub async fn list_api_keys(&self, application_id: &str) -> Result<Vec<ApiKey>> {
        let url = format!("{}/api-keys?application_id={}", IAM_API_BASE, application_id);

        let response = self.http.get(&url).headers(self.headers()).send().await?;

        let response = self.check_response(response).await?;
        let keys: ApiKeysResponse = response.json().await?;
        Ok(keys.api_keys)
    }

    /// Delete an API key.
    pub async fn delete_api_key(&self, access_key: &str) -> Result<()> {
        let url = format!("{}/api-keys/{}", IAM_API_BASE, access_key);

        let response = self
            .http
            .delete(&url)
            .headers(self.headers())
            .send()
            .await?;

        self.check_response(response).await?;
        Ok(())
    }

}
