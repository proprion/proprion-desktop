# Proprion Desktop

> ⚠️ **PROTOTYPE** -- This is an early prototype for demonstration purposes only. Do not use credentials from production or sensitive accounts.

An app for end-users to manage their personal cloud storage and grant scoped access to applications.

1. End-user gets credentials from their cloud provider (Scaleway, Exoscale)
2. End-user adds those credentials to Proprion Desktop
3. Proprion Desktop creates scoped credentials for each app
4. Each app can only access its own folder -- apps cannot see each other's data

**Your data stays in YOUR cloud account. App developers never touch your files.**

## Desktop App

🚧 To be done -- will be a GUI application built with Tauri + React.

## CLI (Prototype)

### Step 1: Create a Cloud Account

Sign up at [Scaleway](https://scaleway.com) or [Exoscale](https://exoscale.com). Both are European providers with no minimum fees.

### Step 2: Get Your API Credentials

In the cloud provider's console, create an API key. You'll get:
- **Scaleway**: Access Key, Secret Key, Organization ID, Project ID
- **Exoscale**: API Key, API Secret

### Step 3: Add Provider to Proprion

```bash
$ proprion add-provider exoscale \
    --name my-cloud \
    --api-key EXO6b596aa... \
    --api-secret gSMMqoFz... \
    --zone de-fra-1 \
    --bucket my-apps-data

Provider 'my-cloud' added successfully.
```

### Step 4: Create Storage for Your First App

You install a fitness tracking app. It needs cloud storage. Create scoped credentials:

```bash
$ proprion create-app \
    --provider my-cloud \
    --name fitness-app \
    --description "Fitness tracker data"

Creating app 'fitness-app' on Exoscale...
  [1/3] Checking/creating bucket 'my-apps-data'...
  [2/3] Creating IAM role with scoped policy...
  [3/3] Creating API key...

=== App Created Successfully ===

{
  "access_key": "EXO61b352c720c8fd7ef733088b",
  "secret_key": "IYLUhy359KRwZShS-7ghTgH0pEgv...",
  "endpoint": "https://sos-de-fra-1.exo.io",
  "bucket": "my-apps-data",
  "prefix": "apps/fitness-app/"
}

This app can ONLY access: s3://my-apps-data/apps/fitness-app/
```

You paste these credentials into the fitness app's settings.

### Step 5: Create Storage for Another App

Now you install a photo sync app:

```bash
$ proprion create-app \
    --provider my-cloud \
    --name photo-sync \
    --description "Photo backup"

{
  "access_key": "EXO2833e3866ea041e07b2c705b",
  "secret_key": "Du40m05-T1znVl8A2fs9wZRi...",
  "endpoint": "https://sos-de-fra-1.exo.io",
  "bucket": "my-apps-data",
  "prefix": "apps/photo-sync/"
}

This app can ONLY access: s3://my-apps-data/apps/photo-sync/
```

### Apps Are Sandboxed

Both apps use the same bucket, but:
- Fitness app can only read/write `apps/fitness-app/*`
- Photo sync can only read/write `apps/photo-sync/*`
- If fitness app tries to access `apps/photo-sync/` → **403 Forbidden**

This is enforced by cloud provider IAM, not by trusting the apps.

### Other Commands

```bash
# List configured providers
$ proprion list-providers
  - my-cloud [exoscale (de-fra-1)]

# List apps
$ proprion list-apps --provider my-cloud
  - fitness-app (Role ID: 1ed07899-80f8-4106-8415-c1bd3aaa57b0)
  - photo-sync (Role ID: 9dbb944b-3b44-44ca-bd1e-137d28f39fca)

# Delete an app
$ proprion delete-app --provider my-cloud --app-id 1ed07899...
  Role and associated API keys deleted successfully.

# Show config file location
$ proprion config-path
  ~/Library/Application Support/org.proprion.proprion/config.toml
```

### Supported Providers

| Provider | Regions | Notes |
|----------|---------|-------|
| Scaleway | fr-par, nl-ams, pl-waw | French/EU |
| Exoscale | de-fra-1, ch-gva-2, ch-dk-2 | Swiss |

## Building from Source

```bash
cargo build --release
./target/release/proprion --help
```

## License

MIT OR Apache-2.0
