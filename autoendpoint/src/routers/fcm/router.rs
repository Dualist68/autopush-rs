use crate::db::client::DbClient;
use crate::error::{ApiError, ApiResult};
use crate::extractors::notification::Notification;
use crate::extractors::router_data_input::RouterDataInput;
use crate::routers::fcm::client::FcmClient;
use crate::routers::fcm::error::FcmError;
use crate::routers::fcm::settings::{FcmCredential, FcmSettings};
use crate::routers::{Router, RouterError, RouterResponse};
use async_trait::async_trait;
use autopush_common::util::InsertOpt;
use cadence::{Counted, StatsdClient};
use serde_json::Value;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use url::Url;
use uuid::Uuid;

/// 28 days
const MAX_TTL: usize = 28 * 24 * 60 * 60;

/// Firebase Cloud Messaging router
pub struct FcmRouter {
    settings: FcmSettings,
    endpoint_url: Url,
    metrics: StatsdClient,
    ddb: Box<dyn DbClient>,
    /// A map from application ID to an authenticated FCM client
    clients: HashMap<String, FcmClient>,
}

impl FcmRouter {
    /// Create a new `FcmRouter`
    pub async fn new(
        settings: FcmSettings,
        endpoint_url: Url,
        http: reqwest::Client,
        metrics: StatsdClient,
        ddb: Box<dyn DbClient>,
    ) -> Result<Self, FcmError> {
        let credentials = settings.credentials()?;
        let clients = Self::create_clients(&settings, credentials, http.clone())
            .await
            .map_err(FcmError::OAuthClientBuild)?;

        Ok(Self {
            settings,
            endpoint_url,
            metrics,
            ddb,
            clients,
        })
    }

    /// Create FCM clients for each application
    async fn create_clients(
        settings: &FcmSettings,
        credentials: HashMap<String, FcmCredential>,
        http: reqwest::Client,
    ) -> std::io::Result<HashMap<String, FcmClient>> {
        let mut clients = HashMap::new();

        for (profile, credential) in credentials {
            clients.insert(
                profile,
                FcmClient::new(&settings, credential, http.clone()).await?,
            );
        }

        Ok(clients)
    }

    /// Convert a notification into a WebPush message
    fn build_message_data(
        &self,
        notification: &Notification,
    ) -> ApiResult<HashMap<&'static str, String>> {
        let mut message_data = HashMap::new();
        message_data.insert("chid", notification.subscription.channel_id.to_string());

        // Only add the other headers if there's data
        if let Some(data) = &notification.data {
            if data.len() > self.settings.max_data {
                // Too much data. Tell the client how many bytes extra they had.
                return Err(FcmError::TooMuchData(data.len() - self.settings.max_data).into());
            }

            // Add the body and headers
            message_data.insert("body", data.clone());
            message_data.insert_opt("con", notification.headers.encoding.as_ref());
            message_data.insert_opt("enc", notification.headers.encryption.as_ref());
            message_data.insert_opt("cryptokey", notification.headers.crypto_key.as_ref());
            message_data.insert_opt("enckey", notification.headers.encryption_key.as_ref());
        }

        Ok(message_data)
    }

    /// Handle an error by logging, updating metrics, etc
    async fn handle_error(&self, error: FcmError, uaid: Uuid) -> ApiError {
        match &error {
            FcmError::FcmAuthentication => {
                error!("FCM authentication error");
                self.incr_error_metric("authentication");
            }
            FcmError::FcmRequestTimeout => {
                warn!("FCM timeout");
                self.incr_error_metric("timeout");
            }
            FcmError::FcmConnect(e) => {
                warn!("FCM unavailable: {error}", error = e.to_string());
                self.incr_error_metric("connection_unavailable");
            }
            FcmError::FcmNotFound => {
                debug!("FCM recipient not found, removing user");
                self.incr_error_metric("recipient_gone");

                if let Err(e) = self.ddb.remove_user(uaid).await {
                    warn!("Error while removing user due to FCM 404: {}", e);
                }
            }
            FcmError::FcmUpstream { .. } | FcmError::FcmUnknown => {
                warn!("{}", error.to_string());
                self.incr_error_metric("server_error");
            }
            _ => {
                warn!(
                    "Unknown error while sending FCM request: {error}",
                    error = error.to_string()
                );
                self.incr_error_metric("unknown");
            }
        }

        ApiError::from(error)
    }

    /// Update metrics after successfully routing the notification
    fn incr_success_metrics(&self, notification: &Notification) {
        self.metrics
            .incr_with_tags("notification.bridge.sent")
            .with_tag("platform", "fcmv1")
            .send();
        self.metrics
            .count_with_tags(
                "notification.message_data",
                notification.data.as_ref().map(String::len).unwrap_or(0) as i64,
            )
            .with_tag("platform", "fcmv1")
            .with_tag("destination", "Direct")
            .send();
    }

    /// Increment `notification.bridge.error` with a reason
    fn incr_error_metric(&self, reason: &str) {
        self.metrics
            .incr_with_tags("notification.bridge.error")
            .with_tag("platform", "fcmv1")
            .with_tag("reason", reason)
            .send();
    }
}

#[async_trait(?Send)]
impl Router for FcmRouter {
    fn register(
        &self,
        router_data_input: &RouterDataInput,
        app_id: &str,
    ) -> Result<HashMap<String, Value, RandomState>, RouterError> {
        if !self.clients.contains_key(app_id) {
            return Err(FcmError::InvalidAppId.into());
        }

        let mut router_data = HashMap::new();
        router_data.insert(
            "token".to_string(),
            serde_json::to_value(&router_data_input.token).unwrap(),
        );
        router_data.insert("app_id".to_string(), serde_json::to_value(app_id).unwrap());

        Ok(router_data)
    }

    async fn route_notification(&self, notification: &Notification) -> ApiResult<RouterResponse> {
        debug!(
            "Sending FCM notification to UAID {}",
            notification.subscription.user.uaid
        );
        trace!("Notification = {:?}", notification);

        let router_data = notification
            .subscription
            .user
            .router_data
            .as_ref()
            .ok_or(FcmError::NoRegistrationToken)?;
        let fcm_token = router_data
            .get("token")
            .and_then(Value::as_str)
            .ok_or(FcmError::NoRegistrationToken)?;
        let app_id = router_data
            .get("app_id")
            .and_then(Value::as_str)
            .ok_or(FcmError::NoAppId)?;
        let ttl = MAX_TTL.min(self.settings.ttl.max(notification.headers.ttl as usize));
        let message_data = self.build_message_data(notification)?;

        // Send the notification to FCM
        let client = self.clients.get(app_id).ok_or(FcmError::InvalidAppId)?;
        trace!("Sending message to FCM: {:?}", message_data);
        if let Err(e) = client.send(message_data, fcm_token.to_string(), ttl).await {
            return Err(self
                .handle_error(e, notification.subscription.user.uaid)
                .await);
        }

        // Sent successfully, update metrics and make response
        trace!("FCM request was successful");
        self.incr_success_metrics(notification);

        Ok(RouterResponse::success(
            self.endpoint_url
                .join(&format!("/m/{}", notification.message_id))
                .expect("Message ID is not URL-safe")
                .to_string(),
            notification.headers.ttl as usize,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::db::client::DbClient;
    use crate::db::mock::MockDbClient;
    use crate::error::ApiErrorKind;
    use crate::extractors::notification::Notification;
    use crate::extractors::notification_headers::NotificationHeaders;
    use crate::extractors::routers::RouterType;
    use crate::extractors::subscription::Subscription;
    use crate::routers::fcm::client::tests::{
        make_service_file, mock_fcm_endpoint_builder, mock_token_endpoint, PROJECT_ID,
    };
    use crate::routers::fcm::error::FcmError;
    use crate::routers::fcm::router::FcmRouter;
    use crate::routers::fcm::settings::FcmSettings;
    use crate::routers::RouterError;
    use crate::routers::{Router, RouterResponse};
    use autopush_common::db::DynamoDbUser;
    use cadence::StatsdClient;
    use mockall::predicate;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use url::Url;
    use uuid::Uuid;

    const FCM_TOKEN: &str = "test-token";
    const CHANNEL_ID: &str = "deadbeef-13f9-4639-87f9-2ff731824f34";

    /// Get the test channel ID as a Uuid
    fn channel_id() -> Uuid {
        Uuid::parse_str(CHANNEL_ID).unwrap()
    }

    /// Create a router for testing, using the given service auth file
    async fn make_router(auth_file: PathBuf, ddb: Box<dyn DbClient>) -> FcmRouter {
        FcmRouter::new(
            FcmSettings {
                fcm_url: Url::parse(&mockito::server_url()).unwrap(),
                credentials: format!(
                    r#"{{ "dev": {{ "project_id": "{}", "auth_file": "{}" }} }}"#,
                    PROJECT_ID,
                    auth_file.to_string_lossy()
                ),
                ..Default::default()
            },
            Url::parse("http://localhost:8080/").unwrap(),
            reqwest::Client::new(),
            StatsdClient::from_sink("autopush", cadence::NopMetricSink),
            ddb,
        )
        .await
        .unwrap()
    }

    /// Create default user router data
    fn default_router_data() -> HashMap<String, serde_json::Value> {
        let mut map = HashMap::new();
        map.insert(
            "token".to_string(),
            serde_json::to_value(FCM_TOKEN).unwrap(),
        );
        map.insert("app_id".to_string(), serde_json::to_value("dev").unwrap());
        map
    }

    /// Create a notification
    fn make_notification(
        router_data: HashMap<String, serde_json::Value>,
        data: Option<String>,
    ) -> Notification {
        Notification {
            message_id: "test-message-id".to_string(),
            subscription: Subscription {
                user: DynamoDbUser {
                    router_data: Some(router_data),
                    ..Default::default()
                },
                channel_id: channel_id(),
                router_type: RouterType::FCM,
                vapid: None,
            },
            headers: NotificationHeaders {
                ttl: 0,
                topic: Some("test-topic".to_string()),
                encoding: Some("test-encoding".to_string()),
                encryption: Some("test-encryption".to_string()),
                encryption_key: Some("test-encryption-key".to_string()),
                crypto_key: Some("test-crypto-key".to_string()),
            },
            timestamp: 0,
            data,
        }
    }

    /// A notification with no data is sent to FCM
    #[tokio::test]
    async fn successful_routing_no_data() {
        let service_file = make_service_file();
        let ddb = MockDbClient::new().into_boxed_arc();
        let router = make_router(service_file.path().to_owned(), ddb).await;
        let _token_mock = mock_token_endpoint();
        let fcm_mock = mock_fcm_endpoint_builder()
            .match_body(
                serde_json::json!({
                    "message": {
                        "android": {
                            "data": {
                                "chid": CHANNEL_ID
                            },
                            "ttl": "60s"
                        },
                        "token": "test-token"
                    }
                })
                .to_string()
                .as_str(),
            )
            .create();
        let notification = make_notification(default_router_data(), None);

        let result = router.route_notification(&notification).await;
        assert!(result.is_ok(), "result = {:?}", result);
        assert_eq!(
            result.unwrap(),
            RouterResponse::success("http://localhost:8080/m/test-message-id".to_string(), 0)
        );
        fcm_mock.assert();
    }

    /// A notification with data is sent to FCM
    #[tokio::test]
    async fn successful_routing_with_data() {
        let service_file = make_service_file();
        let ddb = MockDbClient::new().into_boxed_arc();
        let router = make_router(service_file.path().to_owned(), ddb).await;
        let _token_mock = mock_token_endpoint();
        let fcm_mock = mock_fcm_endpoint_builder()
            .match_body(
                serde_json::json!({
                    "message": {
                        "android": {
                            "data": {
                                "chid": CHANNEL_ID,
                                "body": "test-data",
                                "con": "test-encoding",
                                "enc": "test-encryption",
                                "cryptokey": "test-crypto-key",
                                "enckey": "test-encryption-key"
                            },
                            "ttl": "60s"
                        },
                        "token": "test-token"
                    }
                })
                .to_string()
                .as_str(),
            )
            .create();
        let data = "test-data".to_string();
        let notification = make_notification(default_router_data(), Some(data));

        let result = router.route_notification(&notification).await;
        assert!(result.is_ok(), "result = {:?}", result);
        assert_eq!(
            result.unwrap(),
            RouterResponse::success("http://localhost:8080/m/test-message-id".to_string(), 0)
        );
        fcm_mock.assert();
    }

    /// If there is no client for the user's app ID, an error is returned and
    /// the FCM request is not sent.
    #[tokio::test]
    async fn missing_client() {
        let service_file = make_service_file();
        let ddb = MockDbClient::new().into_boxed_arc();
        let router = make_router(service_file.path().to_owned(), ddb).await;
        let _token_mock = mock_token_endpoint();
        let fcm_mock = mock_fcm_endpoint_builder().expect(0).create();
        let mut router_data = default_router_data();
        router_data.insert(
            "app_id".to_string(),
            serde_json::to_value("unknown-app-id").unwrap(),
        );
        let notification = make_notification(router_data, None);

        let result = router.route_notification(&notification).await;
        assert!(result.is_err());
        assert!(
            matches!(
                result.as_ref().unwrap_err().kind,
                ApiErrorKind::Router(RouterError::Fcm(FcmError::InvalidAppId))
            ),
            "result = {:?}",
            result
        );
        fcm_mock.assert();
    }

    /// If the FCM user no longer exists (404), we drop the user from our database
    #[tokio::test]
    async fn no_fcm_user() {
        let notification = make_notification(default_router_data(), None);
        let mut ddb = MockDbClient::new();
        ddb.expect_remove_user()
            .with(predicate::eq(notification.subscription.user.uaid))
            .times(1)
            .return_once(move |_| Ok(()));

        let service_file = make_service_file();
        let router = make_router(service_file.path().to_owned(), ddb.into_boxed_arc()).await;
        let _token_mock = mock_token_endpoint();
        let _fcm_mock = mock_fcm_endpoint_builder()
            .with_status(404)
            .with_body(r#"{"error":{"status":"NOT_FOUND","message":"test-message"}}"#)
            .create();

        let result = router.route_notification(&notification).await;
        assert!(result.is_err());
        assert!(
            matches!(
                result.as_ref().unwrap_err().kind,
                ApiErrorKind::Router(RouterError::Fcm(FcmError::FcmNotFound))
            ),
            "result = {:?}",
            result
        );
    }
}
