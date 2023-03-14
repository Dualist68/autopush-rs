use actix_web::{
    http::header::{HeaderValue, USER_AGENT},
    middleware::Logger,
    web, App, Error, HttpRequest, HttpResponse, HttpServer,
};
use actix_ws::Message;
use futures_util::StreamExt;

use autoconnect_ws_sm::{
    session::SessionImpl,
    sm::{Unidentified, WebPushClientState},
};

//pub mod session;
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
    ////let client = WebPushClient::new(session, msg_stream, ua);
    ////////let client = WebPushClient<Unidentified>::new(session, msg_stream, ua);
    /// Can the generic be a default?
    //let client = WebPushClient::new(&ua);
    //client::spawn_webpush_ws(session, msg_stream);
    //actix_rt::spawn(client::webpush_ws(session, msg_stream)) actually better I think

    // Need the:
    // - session
    // - user-agent
    // - db
    // - metrics
    // - eventually settings..
    // *- can't clone msg_stream, can the SM respond with what it wants sent to it?
    // *- broadcast_subs come from Hello
    /*
    let client = WebPushClientState::Unidentified(Unidentified::new(
        Box::new(SessionImpl::new(&session)),
        ua,
    ));

    actix_rt::spawn(async move {
        let Some(Ok(msg)) = msg_stream.next().await else {

        } else {
            // otherwise close
        }

        /*
        while let Some(Ok(msg)) = msg_stream.next().await {
            let Message::Text(txt) = msg else {
                // XXX: signal an error and return?
                return;
            };
        }
        */

        let _ = session.close(None).await;
    });
     */
    actix_rt::spawn(handler::webpush_ws(session, msg_stream, ua));

    Ok(response)
}
