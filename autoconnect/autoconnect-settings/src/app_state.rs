use std::sync::Arc;

use cadence::StatsdClient;
use fernet::{Fernet, MultiFernet};

use autoconnect_common::registry::ClientRegistry;
use autopush_common::db::{client::DbClient, dynamodb::DdbClientImpl, DbSettings, StorageType};
use autopush_common::{
    errors::{ApcErrorKind, Result},
    metrics::new_metrics,
};

use crate::{Settings, ENV_PREFIX};

#[derive(Clone)]
pub struct AppState {
    /// Handle to the data storage object
    pub db: Box<dyn DbClient>,
    pub metrics: Arc<StatsdClient>,

    /// Encryption object for the endpoint URL
    pub fernet: MultiFernet,
    /// The connected WebSocket clients
    pub clients: Arc<ClientRegistry>,

    pub settings: Settings,
    pub router_url: String,
    pub endpoint_url: String,
}

impl AppState {
    pub fn from_settings(settings: Settings) -> Result<Self> {
        let crypto_key = &settings.crypto_key;
        if !(crypto_key.starts_with('[') && crypto_key.ends_with(']')) {
            return Err(
                ApcErrorKind::ConfigError(config::ConfigError::Message(format!(
                    "Invalid {}_CRYPTO_KEY",
                    ENV_PREFIX
                )))
                .into(),
            );
        }
        let crypto_key = &crypto_key[1..crypto_key.len() - 1];
        debug!("Fernet keys: {:?}", &crypto_key);
        let fernets: Vec<Fernet> = crypto_key
            .split(',')
            .map(|s| s.trim().to_string())
            .map(|key| {
                Fernet::new(&key).unwrap_or_else(|| panic!("Invalid {}_CRYPTO_KEY", ENV_PREFIX))
            })
            .collect();
        let fernet = MultiFernet::new(fernets);
        let metrics = Arc::new(new_metrics(
            settings.statsd_host.clone(),
            settings.statsd_port,
        )?);

        let db_settings = DbSettings {
            dsn: settings.db_dsn.clone(),
            db_settings: settings.db_settings.clone(),
        };
        let db = match StorageType::from_dsn(&db_settings.dsn) {
            StorageType::DynamoDb => Box::new(DdbClientImpl::new(metrics.clone(), &db_settings)?),
            StorageType::INVALID => panic!("Invalid Storage type. Check {}_DB_DSN.", ENV_PREFIX),
        };

        let router_url = settings.router_url();
        let endpoint_url = settings.endpoint_url();

        Ok(Self {
            db,
            metrics,
            fernet,
            clients: Arc::new(ClientRegistry::default()),
            settings,
            router_url,
            endpoint_url,
        })
    }
}

/// For tests
impl Default for AppState {
    fn default() -> Self {
        Self::from_settings(Default::default()).unwrap()
    }
}
