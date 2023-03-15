use std::sync::Arc;

use actix_web::{
    http::header::{HeaderValue, USER_AGENT},
    middleware::Logger,
    web, App, Error, HttpRequest, HttpResponse, HttpServer,
};
use actix_ws::Message;
use futures_util::StreamExt;

use autoconnect_settings::options::ServerOptions;

mod handler;
mod session;

async fn ws(req: HttpRequest, body: web::Payload) -> Result<HttpResponse, Error> {
    let state = req.app_data::<ServerOptions>().unwrap().clone();
    let db = state.db_client.clone();
    let metrics = Arc::clone(&state.metrics);
    let registry = Arc::clone(&state.registry);
    let ua = req
        .headers()
        .get(USER_AGENT)
        .unwrap_or(&HeaderValue::from_static(""))
        .to_str()
        .unwrap_or_default()
        .to_owned();

    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?;
    actix_rt::spawn(handler::webpush_ws(
        db, metrics, registry, session, msg_stream, ua,
    ));
    Ok(response)
}
