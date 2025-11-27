use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use s3::creds::Credentials;
use s3::region::Region;
use s3::{Bucket, BucketConfiguration};

mod config;
mod exoscale;
mod scaleway;

use config::{Config, ProviderConfig, ScalewayProviderConfig, ExoscaleProviderConfig};

#[derive(Parser)]
#[command(name = "proprion")]
#[command(about = "CLI tool for managing Proprion app credentials")]
#[command(version)]
struct Cli {
    /// Path to config file (default: OS-specific config directory)
    #[arg(short, long, global = true)]
    config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum AddProviderCommand {
    /// Add Scaleway provider
    Scaleway {
        /// Provider name (your choice, e.g., "my-scaleway")
        #[arg(short, long)]
        name: String,

        /// Access key
        #[arg(long)]
        access_key: String,

        /// Secret key
        #[arg(long)]
        secret_key: String,

        /// Region (e.g., fr-par, nl-ams, pl-waw)
        #[arg(long)]
        region: String,

        /// Bucket name
        #[arg(long)]
        bucket: String,

        /// Organization ID
        #[arg(long)]
        organization_id: String,

        /// Project ID
        #[arg(long)]
        project_id: String,
    },

    /// Add Exoscale provider
    Exoscale {
        /// Provider name (your choice, e.g., "my-exoscale")
        #[arg(short, long)]
        name: String,

        /// API key
        #[arg(long)]
        api_key: String,

        /// API secret
        #[arg(long)]
        api_secret: String,

        /// Zone (e.g., ch-gva-2, de-fra-1, ch-dk-2)
        #[arg(long)]
        zone: String,

        /// Bucket name
        #[arg(long)]
        bucket: String,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new provider configuration
    #[command(name = "add-provider")]
    AddProvider {
        #[command(subcommand)]
        provider: AddProviderCommand,
    },

    /// List configured providers
    #[command(name = "list-providers")]
    ListProviders,

    /// Remove a provider configuration
    #[command(name = "remove-provider")]
    RemoveProvider {
        /// Provider name to remove
        #[arg(short, long)]
        name: String,
    },

    /// Show config file path
    #[command(name = "config-path")]
    ConfigPath,

    /// Create credentials for a new app
    #[command(name = "create-app")]
    CreateApp {
        /// Provider name (from config)
        #[arg(short, long)]
        provider: String,

        /// App name
        #[arg(short, long)]
        name: String,

        /// App description
        #[arg(short, long)]
        description: String,
    },

    /// List existing apps
    #[command(name = "list-apps")]
    ListApps {
        /// Provider name (from config)
        #[arg(short, long)]
        provider: String,
    },

    /// Delete an app and its credentials
    #[command(name = "delete-app")]
    DeleteApp {
        /// Provider name (from config)
        #[arg(short, long)]
        provider: String,

        /// Application ID to delete
        #[arg(short, long)]
        app_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::AddProvider { provider } => {
            let (name, provider_config) = match provider {
                AddProviderCommand::Scaleway {
                    name,
                    access_key,
                    secret_key,
                    region,
                    bucket,
                    organization_id,
                    project_id,
                } => {
                    let config = ProviderConfig::Scaleway(ScalewayProviderConfig {
                        access_key,
                        secret_key,
                        organization_id,
                        project_id,
                        region,
                        bucket,
                    });
                    (name, config)
                }
                AddProviderCommand::Exoscale {
                    name,
                    api_key,
                    api_secret,
                    zone,
                    bucket,
                } => {
                    let config = ProviderConfig::Exoscale(ExoscaleProviderConfig {
                        api_key,
                        api_secret,
                        zone,
                        bucket,
                    });
                    (name, config)
                }
            };

            let mut config = Config::load(cli.config.as_ref())?;
            config.set_provider(name.clone(), provider_config);
            config.save(cli.config.as_ref())?;

            println!("Provider '{}' added successfully.", name);
            println!("Config saved to: {}", Config::path(cli.config.as_ref())?.display());
        }

        Commands::ListProviders => {
            let config = Config::load(cli.config.as_ref())?;

            if config.providers.is_empty() {
                println!("No providers configured.");
                println!("Add one with: proprion add-provider --help");
            } else {
                println!("Configured providers:");
                for (name, provider) in &config.providers {
                    let type_name = match provider {
                        ProviderConfig::Scaleway(cfg) => format!("scaleway ({})", cfg.region),
                        ProviderConfig::Exoscale(cfg) => format!("exoscale ({})", cfg.zone),
                    };
                    println!("  - {} [{}]", name, type_name);
                }
            }
        }

        Commands::RemoveProvider { name } => {
            let mut config = Config::load(cli.config.as_ref())?;

            if config.remove_provider(&name).is_some() {
                config.save(cli.config.as_ref())?;
                println!("Provider '{}' removed.", name);
            } else {
                println!("Provider '{}' not found.", name);
            }
        }

        Commands::ConfigPath => {
            println!("{}", Config::path(cli.config.as_ref())?.display());
        }

        Commands::CreateApp {
            provider,
            name,
            description,
        } => {
            let config = Config::load(cli.config.as_ref())?;
            let provider_config = config
                .get_provider(&provider)
                .with_context(|| format!("Provider '{}' not found. Run 'proprion list-providers' to see configured providers.", provider))?;

            match provider_config {
                ProviderConfig::Scaleway(cfg) => {
                    create_scaleway_app(cfg, &name, &description).await?;
                }
                ProviderConfig::Exoscale(cfg) => {
                    create_exoscale_app(cfg, &name, &description).await?;
                }
            }
        }

        Commands::ListApps { provider } => {
            let config = Config::load(cli.config.as_ref())?;
            let provider_config = config
                .get_provider(&provider)
                .with_context(|| format!("Provider '{}' not found.", provider))?;

            match provider_config {
                ProviderConfig::Scaleway(cfg) => {
                    list_scaleway_apps(cfg).await?;
                }
                ProviderConfig::Exoscale(cfg) => {
                    list_exoscale_apps(cfg).await?;
                }
            }
        }

        Commands::DeleteApp { provider, app_id } => {
            let config = Config::load(cli.config.as_ref())?;
            let provider_config = config
                .get_provider(&provider)
                .with_context(|| format!("Provider '{}' not found.", provider))?;

            match provider_config {
                ProviderConfig::Scaleway(cfg) => {
                    delete_scaleway_app(cfg, &app_id).await?;
                }
                ProviderConfig::Exoscale(cfg) => {
                    delete_exoscale_app(cfg, &app_id).await?;
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// Scaleway Implementation
// ============================================================================

async fn create_scaleway_app(cfg: &ScalewayProviderConfig, name: &str, description: &str) -> Result<()> {
    let client = scaleway::Client::new(cfg.secret_key.clone());
    let app_prefix = format!("apps/{}", name);

    println!("Creating app '{}' on Scaleway...", name);

    // Step 1: Create bucket if needed
    println!("  [1/5] Checking/creating bucket '{}'...", cfg.bucket);
    ensure_bucket_exists(&cfg.access_key, &cfg.secret_key, &cfg.region, &cfg.bucket, "scaleway").await?;
    println!("        Bucket ready");

    // Step 2: Create application
    println!("  [2/5] Creating IAM application...");
    let app = client
        .create_application(name, description, &cfg.organization_id)
        .await
        .context("Failed to create application")?;
    println!("        Application ID: {}", app.id);

    // Step 3: Create policy
    println!("  [3/5] Creating IAM policy...");
    let policy_name = format!("{}-policy", name);
    let policy = client
        .create_policy(&policy_name, &app.id, &cfg.organization_id, &cfg.project_id)
        .await
        .context("Failed to create policy")?;
    println!("        Policy ID: {}", policy.id);

    // Step 4: Create API key
    println!("  [4/5] Creating API key...");
    let api_key = client
        .create_api_key(&app.id, &format!("API key for {}", name), Some(&cfg.project_id))
        .await
        .context("Failed to create API key")?;
    println!("        Access Key: {}", api_key.access_key);

    // Step 5: Apply bucket policy
    println!("  [5/5] Applying bucket policy for prefix '{}'...", app_prefix);
    apply_scaleway_bucket_policy(
        &cfg.access_key,
        &cfg.secret_key,
        &cfg.region,
        &cfg.bucket,
        &app.id,
        name,
        &app_prefix,
    ).await?;
    println!("        Bucket policy applied");

    // Output credentials
    println!();
    println!("=== App Created Successfully ===");
    println!();
    println!("S3 Credentials for '{}':", name);
    println!();

    let creds = serde_json::json!({
        "access_key": api_key.access_key,
        "secret_key": api_key.secret_key,
        "endpoint": cfg.endpoint(),
        "region": cfg.region,
        "bucket": cfg.bucket,
        "prefix": app_prefix
    });

    println!("{}", serde_json::to_string_pretty(&creds)?);
    println!();
    println!("IMPORTANT: Save the secret_key now - it cannot be retrieved later!");
    println!();
    println!("Application ID: {} (save this to delete the app later)", app.id);
    println!();
    println!("This app can ONLY access: s3://{}/{}/", cfg.bucket, app_prefix);

    Ok(())
}

async fn list_scaleway_apps(cfg: &ScalewayProviderConfig) -> Result<()> {
    let client = scaleway::Client::new(cfg.secret_key.clone());

    println!("Fetching applications...");
    let apps = client
        .list_applications(&cfg.organization_id)
        .await
        .context("Failed to list applications")?;

    if apps.is_empty() {
        println!("No applications found.");
    } else {
        println!();
        println!("Applications:");
        for app in apps {
            println!("  - {} (ID: {})", app.name, app.id);
            if let Some(desc) = &app.description {
                if !desc.is_empty() {
                    println!("    {}", desc);
                }
            }
        }
    }

    Ok(())
}

async fn delete_scaleway_app(cfg: &ScalewayProviderConfig, app_id: &str) -> Result<()> {
    let client = scaleway::Client::new(cfg.secret_key.clone());

    println!("Deleting application {}...", app_id);

    // Just delete the application directly
    // Scaleway should cascade delete associated resources
    client
        .delete_application(app_id)
        .await
        .context("Failed to delete application")?;

    println!("Application deleted successfully.");
    println!();
    println!("Note: You may want to manually update the bucket policy to remove this app's statement.");

    Ok(())
}

// ============================================================================
// Exoscale Implementation
// ============================================================================

async fn create_exoscale_app(cfg: &ExoscaleProviderConfig, name: &str, description: &str) -> Result<()> {
    let client = exoscale::Client::new(cfg.api_key.clone(), cfg.api_secret.clone(), &cfg.zone);
    let app_prefix = format!("apps/{}/", name);

    println!("Creating app '{}' on Exoscale...", name);

    // Step 1: Create bucket if needed
    println!("  [1/3] Checking/creating bucket '{}'...", cfg.bucket);
    ensure_bucket_exists(&cfg.api_key, &cfg.api_secret, &cfg.zone, &cfg.bucket, "exoscale").await?;
    println!("        Bucket ready");

    // Step 2: Create IAM role with scoped policy
    println!("  [2/3] Creating IAM role with scoped policy...");
    let role_name = format!("proprion-{}", name);
    let role = client
        .create_role(&role_name, description, &cfg.bucket, &app_prefix)
        .await
        .context("Failed to create IAM role")?;
    println!("        Role ID: {}", role.id);

    // Wait for role to propagate (Exoscale async operations need time)
    println!("        Waiting for role to propagate...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Step 3: Create API key attached to role
    println!("  [3/3] Creating API key...");
    let key_name = format!("proprion-{}-key", name);
    let api_key = client
        .create_api_key(&key_name, &role.id)
        .await
        .context("Failed to create API key")?;
    let access_key = &api_key.key;
    let secret_key = api_key.secret
        .as_ref()
        .context("API key response missing secret")?;
    println!("        Access Key: {}", access_key);

    // Output credentials
    println!();
    println!("=== App Created Successfully ===");
    println!();
    println!("S3 Credentials for '{}':", name);
    println!();

    let creds = serde_json::json!({
        "access_key": access_key,
        "secret_key": secret_key,
        "endpoint": cfg.endpoint(),
        "zone": cfg.zone,
        "bucket": cfg.bucket,
        "prefix": app_prefix
    });

    println!("{}", serde_json::to_string_pretty(&creds)?);
    println!();
    println!("IMPORTANT: Save the secret_key now - it cannot be retrieved later!");
    println!();
    println!("Role ID: {} (save this to delete the app later)", role.id);
    println!();
    println!("This app can ONLY access: s3://{}/{}", cfg.bucket, app_prefix);

    Ok(())
}

async fn list_exoscale_apps(cfg: &ExoscaleProviderConfig) -> Result<()> {
    let client = exoscale::Client::new(cfg.api_key.clone(), cfg.api_secret.clone(), &cfg.zone);

    println!("Fetching IAM roles...");
    let roles = client
        .list_roles()
        .await
        .context("Failed to list roles")?;

    // Filter to only show roles created by Proprion (have "proprion-" prefix)
    let proprion_roles: Vec<_> = roles
        .iter()
        .filter(|r| {
            r.name
                .as_ref()
                .map(|n| n.starts_with("proprion-"))
                .unwrap_or(false)
        })
        .collect();

    if proprion_roles.is_empty() {
        println!("No Proprion apps found.");
    } else {
        println!();
        println!("Proprion Apps (Exoscale IAM roles):");
        for role in proprion_roles {
            let name = role.name.as_deref().unwrap_or("unknown");
            let app_name = name.strip_prefix("proprion-").unwrap_or(name);
            let desc = role.description.as_deref().unwrap_or("");
            println!("  - {} (Role ID: {})", app_name, role.id);
            if !desc.is_empty() {
                println!("    {}", desc);
            }
        }
    }

    Ok(())
}

async fn delete_exoscale_app(cfg: &ExoscaleProviderConfig, role_id: &str) -> Result<()> {
    let client = exoscale::Client::new(cfg.api_key.clone(), cfg.api_secret.clone(), &cfg.zone);

    println!("Deleting IAM role {}...", role_id);

    // First, list and delete API keys associated with this role
    let api_keys = client
        .list_api_keys()
        .await
        .context("Failed to list API keys")?;

    for key in api_keys {
        if key.role_id.as_deref() == Some(role_id) {
            println!("  Deleting API key {}...", key.key);
            client.delete_api_key(&key.key).await.ok();
        }
    }

    // Then delete the role
    client
        .delete_role(role_id)
        .await
        .context("Failed to delete role")?;

    println!("Role and associated API keys deleted successfully.");

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn ensure_bucket_exists(
    access_key: &str,
    secret_key: &str,
    region: &str,
    bucket_name: &str,
    provider: &str,
) -> Result<()> {
    let endpoint = match provider {
        "scaleway" => format!("https://s3.{}.scw.cloud", region),
        "exoscale" => format!("https://sos-{}.exo.io", region),
        _ => anyhow::bail!("Unknown provider: {}", provider),
    };

    let s3_region = Region::Custom {
        region: region.to_string(),
        endpoint: endpoint.clone(),
    };

    let credentials = Credentials::new(Some(access_key), Some(secret_key), None, None, None)
        .context("Failed to create S3 credentials")?;

    // Try to access the bucket
    let bucket = Bucket::new(bucket_name, s3_region.clone(), credentials.clone())
        .context("Failed to create bucket reference")?
        .with_path_style();

    match bucket.list("".to_string(), Some("/".to_string())).await {
        Ok(_) => Ok(()),
        Err(_) => {
            // Create bucket
            Bucket::create_with_path_style(
                bucket_name,
                s3_region,
                credentials,
                BucketConfiguration::default(),
            )
            .await
            .context("Failed to create bucket")?;
            Ok(())
        }
    }
}

async fn apply_scaleway_bucket_policy(
    access_key: &str,
    secret_key: &str,
    region: &str,
    bucket: &str,
    application_id: &str,
    app_name: &str,
    app_prefix: &str,
) -> Result<()> {
    use std::io::Write;
    use std::process::Command;

    let endpoint = format!("https://s3.{}.scw.cloud", region);
    let aws_env = [
        ("AWS_ACCESS_KEY_ID", access_key),
        ("AWS_SECRET_ACCESS_KEY", secret_key),
    ];

    // Get existing policy
    let get_output = Command::new("aws")
        .args(["s3api", "get-bucket-policy", "--bucket", bucket, "--endpoint-url", &endpoint, "--output", "json"])
        .envs(aws_env.clone())
        .output()
        .context("Failed to execute aws CLI")?;

    let mut policy: serde_json::Value = if get_output.status.success() {
        let output_str = String::from_utf8_lossy(&get_output.stdout);
        let wrapper: serde_json::Value = serde_json::from_str(&output_str).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(policy_str) = wrapper.get("Policy").and_then(|p| p.as_str()) {
            serde_json::from_str(policy_str).unwrap_or_else(|_| create_empty_policy())
        } else {
            create_empty_policy()
        }
    } else {
        create_empty_policy()
    };

    // Add new statement
    let new_statement = serde_json::json!({
        "Sid": format!("proprion-{}", app_name),
        "Effect": "Allow",
        "Principal": { "SCW": format!("application_id:{}", application_id) },
        "Action": ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
        "Resource": format!("{}/{}/*", bucket, app_prefix)
    });

    if let Some(statements) = policy.get_mut("Statement") {
        if let Some(arr) = statements.as_array_mut() {
            arr.retain(|s| {
                s.get("Sid")
                    .and_then(|sid| sid.as_str())
                    .map(|sid| sid != format!("proprion-{}", app_name))
                    .unwrap_or(true)
            });
            arr.push(new_statement);
        }
    }

    // Write and apply
    let policy_str = serde_json::to_string(&policy)?;
    let mut temp_file = tempfile::NamedTempFile::new()?;
    temp_file.write_all(policy_str.as_bytes())?;
    let temp_path = temp_file.path().to_string_lossy().to_string();

    let put_output = Command::new("aws")
        .args(["s3api", "put-bucket-policy", "--bucket", bucket, "--policy", &format!("file://{}", temp_path), "--endpoint-url", &endpoint])
        .envs(aws_env)
        .output()
        .context("Failed to execute aws CLI")?;

    if !put_output.status.success() {
        let stderr = String::from_utf8_lossy(&put_output.stderr);
        anyhow::bail!("Failed to apply bucket policy: {}", stderr);
    }

    Ok(())
}

fn create_empty_policy() -> serde_json::Value {
    serde_json::json!({
        "Version": "2023-04-17",
        "Statement": []
    })
}
