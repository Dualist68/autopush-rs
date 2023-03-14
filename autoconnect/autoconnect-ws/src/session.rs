use actix_http::ws::CloseReason;
use actix_web::web::Bytes;
use actix_ws::Closed;
use async_trait::async_trait;
use mockall::automock;

use autoconnect_protocol::ClientMessage;
//use crate::sm::ClientMessage;
//use autopush::src::server::protocol::ServerMessage;

/// Trait wrapping [`actix_ws::Session`] so it can be replaced by e.g. a mock.
#[automock]
#[async_trait]
pub trait Session {
    /// See [`actix_ws::Session::text`]
    //#[mockall::concretize]
    //async fn text<T: Into<String>>(&mut self, msg: T) -> Result<(), Closed>;
    //async fn text<T: Into<String> + Send + 'static>(&mut self, msg: T) -> Result<(), Closed>;
    /*
    async fn text<T>(&mut self, msg: T) -> Result<(), Closed>
    where
        T: Into<String> + Send + 'static;
    */
    async fn text(&mut self, msg: ClientMessage) -> Result<(), Closed>;

    /// See [`actix_ws::Session::binary`]
    //#[mockall::concretize]
    //async fn binary<T: Into<Bytes>>(&mut self, msg: T) -> Result<(), Closed>;
    //async fn binary<T: Into<Bytes> + Send + 'static>(&mut self, msg: T) -> Result<(), Closed>;
    /*
    async fn binary<T>(&mut self, msg: T) -> Result<(), Closed>
    where
       T: Into<Bytes> + Send + 'static;
    */
    async fn binary(&mut self, msg: ClientMessage) -> Result<(), Closed>;

    /// See [`actix_ws::Session::ping`]
    async fn ping(&mut self, msg: &[u8]) -> Result<(), Closed>;

    /// See [`actix_ws::Session::pong`]
    async fn pong(&mut self, msg: &[u8]) -> Result<(), Closed>;

    /// See [`actix_ws::Session::close`]
    async fn close(mut self, reason: Option<CloseReason>) -> Result<(), Closed>;
}

#[derive(Clone)]
pub struct SessionImpl {
    inner: actix_ws::Session,
}

impl SessionImpl {
    pub fn new(session: &actix_ws::Session) -> Self {
        SessionImpl {
            inner: session.clone(),
        }
    }
}

#[async_trait]
impl Session for SessionImpl {
    async fn text(&mut self, msg: ClientMessage) -> Result<(), Closed> {
        self.inner.text(msg).await
    }

    async fn binary(&mut self, msg: ClientMessage) -> Result<(), Closed> {
        self.inner.binary(msg).await
    }

    async fn ping(&mut self, msg: &[u8]) -> Result<(), Closed> {
        self.inner.ping(msg).await
    }

    async fn pong(&mut self, msg: &[u8]) -> Result<(), Closed> {
        self.inner.pong(msg).await
    }

    async fn close(mut self, reason: Option<CloseReason>) -> Result<(), Closed> {
        self.inner.close(reason).await
    }

}
