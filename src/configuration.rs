use sqlx::postgres::{PgConnectOptions, PgSslMode};

pub enum Environment {
    Development,
    Production
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Development => "development",
            Environment::Production => "production"
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "development" => Ok(Environment::Development),
            "production" => Ok(Environment::Production),
            other => Err(format!("{} is not a supported environment", other))
        }
    }
}

#[derive(serde::Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
}

#[derive(serde::Deserialize)]
pub struct ApplicationSettings {
    pub http_bind_address: String,
    pub http_listen_port: u16,
}

#[derive(serde::Deserialize, Clone)]
pub struct DatabaseSettings {
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database_name: String,
    pub require_ssl: bool,
}

impl DatabaseSettings {
    pub fn connect_options(&self) -> PgConnectOptions {
        let ssl_mode = if self.require_ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };
        PgConnectOptions::new()
            .host(&self.hostname)
            .port(self.port)
            .username(&self.username)
            .password(&self.password)
            .database(&self.database_name)
            .ssl_mode(ssl_mode)
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    let base_path = std::env::current_dir().expect("Failed to determine current directory");
    let config_dir = base_path.join("configuration");

    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "development".to_string())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");
    let env_config_file = format!("{}.yaml", environment.as_str());

    let settings = config::Config::builder()
        .add_source(
            config::File::from(config_dir.join("base.yaml"))
        )
        .add_source(
            config::File::from(config_dir.join(env_config_file))
        )
        .add_source(
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("__")
        )
        .build()?;

    settings.try_deserialize::<Settings>()
}
