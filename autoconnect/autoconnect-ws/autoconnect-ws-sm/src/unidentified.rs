use std::{fmt, sync::Arc};

use cadence::{CountedExt, StatsdClient};

use autoconnect_protocol::{ClientMessage, ServerMessage};
use autoconnect_settings::Settings;
use autopush_common::{
    db::{client::DbClient, HelloResponse},
    util::ms_since_epoch,
};
//use autopush_common::errors::{ApcError, ApcErrorKind, Result};

use crate::{error::ClientStateError};

type SMResult<T> = Result<T, ClientStateError>;

//#[derive(Debug)]
pub struct UnidentifiedClient {
    db: Box<dyn DbClient>,
    metrics: Arc<StatsdClient>,
    settings: Settings,
    /// Client's User-Agent
    ua: String, // XXX: why not borrow? could borrow other stuff..
}

impl fmt::Debug for UnidentifiedClient {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnidentifiedClient")
            .field("metrics", &self.metrics)
            .field("ua", &self.ua)
            .finish()
    }
}


impl UnidentifiedClient {
    pub fn new(db: Box<dyn DbClient>, metrics: Arc<StatsdClient>, settings: Settings, ua: &str) -> Self {
        UnidentifiedClient {
            db,
            metrics,
            settings,
            ua: ua.to_owned(),
        }
    }

    // self: UnidentifiedClient -> WebPushClient
    // -> ServerMessage should be -> impl Iterator<ServerMessage>
    // XXX: This could return the desired broadcasts as well
    pub async fn on_client_message(&self, msg: ClientMessage) -> SMResult<((), ServerMessage)> {
        let ClientMessage::Hello {
            uaid,
            use_webpush: Some(true),
            broadcasts,
            ..
        } = msg else {
            // XXX: error msg
            return Err(ClientStateError::InvalidClientMessage("".to_owned()));
        };

        // LOG::TRACE!
        let connected_at = ms_since_epoch();
        //let uaid = uaid.and_then(|uaid| Uuid::parse_str(uaid.as_str()).ok()),
        let uaid = uaid
            .as_deref()
            .map(uuid::Uuid::parse_str)
            .transpose()
            .map_err(|e| ClientStateError::InvalidClientMessage("XXX".to_owned()))?;
        // XXX: sending push messages too?

        let defer_registration = uaid.is_none();
        let hello_response = self.db.hello(
            connected_at,
            uaid.as_ref(),
            "https://cnn.com/",
            defer_registration,
        ).await?;

        let Some(uaid) = hello_response.uaid else {
            return Err(ClientStateError::AlreadyConnected);
        };
        self.metrics.incr("ua.command.hello");
        /*
        let HelloResponse {
            uaid: Some(uaid),
            message_month,
            check_storage,
            reset_uaid,
            rotate_message_table,
            connected_at,
            deferred_user_registration,
        } =
        */
        let smsg = ServerMessage::Hello {
            uaid: uaid.as_simple().to_string(),
            status: 200,
            use_webpush: Some(true),
            // XXX: broadcasts
            broadcasts: std::collections::HashMap::new(),
        };
        // XXX: WebPushClient vs BroacastClient?  BroadcastClient needs only
        // listen on ClientMessage and PingManager, not ServerMessages..  its
        // weird getting a ServerMessage stream from a BroadcastClient when it
        // evolves into a WebPushClient, though...
        //
        // could do:
        // struct IdentifiedClient {
        //     client: WebPushClient,
        //     ping_manager: PingManager, // OR -ws makes this itself (handles all the broadcast code? requested broadcasts are in the ClientMessage though..)..
        //     server_msgs: ServerMessages, // OR -ws makes this itself
        //     //broadcasts: Broadcasts, // BroadcastStream? PingManager?
        //
        // }
        Ok(((), smsg))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cadence::{StatsdClient, NopMetricSink};
    use uuid::Uuid;

    use autoconnect_protocol::ClientMessage;
    use autoconnect_settings::Settings;
    use autopush_common::db::{HelloResponse, mock::MockDbClient};

    use crate::{error::ClientStateError};

    use super::UnidentifiedClient;

    const UA: &str =
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:95.0) Gecko/20100101 Firefox/95.0";
    const DUMMY_UAID: &str = "deadbeef-0000-0000-abad-1dea00000000";
    const DUMMY_CHID: &str = "deadbeef00000000decafbad00000000";

    #[tokio::test]
    async fn reject_not_hello() {
        let client = UnidentifiedClient::new(
            MockDbClient::new().into_boxed_arc(),
            Arc::new(StatsdClient::builder("", NopMetricSink).build()),
            Settings::default(),
            UA,
        );
        assert!(client.on_client_message(ClientMessage::Ping).await.is_err());
        assert!(client.on_client_message(ClientMessage::Register {
            channel_id: DUMMY_CHID.to_owned(),
            key: None,
        }).await.is_err());
    }

    #[tokio::test]
    async fn hello_existing_user() {
        let mut db = MockDbClient::new();
        // XXX: hello should be in a sub trait
        db.expect_hello()
            .returning(|_, _, _, _| Ok(HelloResponse {
                uaid: Some(Uuid::try_parse(DUMMY_UAID).unwrap()),
                ..Default::default()
            }));
        let client = UnidentifiedClient::new(
            db.into_boxed_arc(),
            Arc::new(StatsdClient::builder("", NopMetricSink).build()),
            Settings::default(),
            UA,
        );
        let msg = ClientMessage::Hello {
            uaid: Some(DUMMY_UAID.to_owned()),
            channel_ids: None,
            use_webpush: Some(true),
            broadcasts: None,
        };
        let result = client.on_client_message(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn hello_new_user() {
        let mut db = MockDbClient::new();
        // Ensure no write to the db
        db.expect_hello()
            .withf(|_, _, _, defer_registration| defer_registration == &true)
            .returning(|_, _, _, _| Ok(HelloResponse {
                uaid: Some(Uuid::try_parse(DUMMY_UAID).unwrap()),
                ..Default::default()
            }));
        let client = UnidentifiedClient::new(
            db.into_boxed_arc(),
            Arc::new(StatsdClient::builder("", NopMetricSink).build()),
            Settings::default(),
            UA,
        );
        let msg = ClientMessage::Hello {
            uaid: None,
            channel_ids: None,
            use_webpush: Some(true),
            broadcasts: None,
        };
        let result = client.on_client_message(msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn hello_bad_user() {
    }

}
