use actix_web::{
    http::header::{HeaderValue, USER_AGENT},
    middleware::Logger,
    web, App, Error, HttpRequest, HttpResponse, HttpServer,
};
use actix_ws::Message;
use futures_util::StreamExt;

use autoconnect_ws_sm::{
//    session::SessionImpl,
//    sm::{Unidentified, WebPushClientState},
};

pub mod session;
pub mod handler;

async fn ws(req: HttpRequest, body: web::Payload) -> Result<HttpResponse, Error> {
    let ua = req
        .headers()
        .get(USER_AGENT)
        .unwrap_or(&HeaderValue::from_static(""))
        .to_str()
        .unwrap_or_default()
        .to_owned();
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?;
    actix_rt::spawn(handler::webpush_ws(session, msg_stream, ua));
    Ok(response)
}
