//use futures::select;
use futures_util::{
    future::{self, Either},
    FutureExt, StreamExt,
};
use std::time::{Duration, Instant};
use tokio::{pin, select, time::interval};

use autoconnect_protocol::ClientMessage;
use autoconnect_ws_sm::{
    UnidentifiedClient,
};

use crate::session::{Session, SessionImpl};

pub async fn webpush_ws(
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
    user_agent: String,
) {
    let mut session = SessionImpl::new(&session);
    /*
    let mut client = WebPushClientState::Unidentified(Unidentified::new(
        Box::new(session.clone()),
        user_agent,
    ));
     */
    //let mut client = UnidentifiedClient::new(
    //_webpush_ws(client, &mut session, msg_stream).await.expect("XXX");
    panic!();
}

//async fn _webpush_ws(mut client: WebPushClientState, mut msg_stream: actix_ws::MessageStream) -> Result<(), String> {
async fn _webpush_ws(
    mut client: UnidentifiedClient,
    session: &mut impl Session,
    mut msg_stream: impl futures::Stream<Item = Result<actix_ws::Message, actix_ws::ProtocolError>>
        + Unpin,
) -> Result<(), String> {
    let Some(Ok(msg)) = msg_stream.next().await else {
        return Err("doh".to_owned());
    };
    eprintln!("MSG1: {:#?}", msg);

    /*
    use futures::channel::mpsc::channel;
    let (mut tx, mut rx) = channel(1024);
    tx.try_send("ServerNotification".to_owned()).unwrap();
    */

    client = client.on_client_message(ClientMessage::Hello)?;
    // The first ClientMessage succeeded: we're in the Identified state
    let mut interval = interval(Duration::from_secs(3));
    let _reason = loop {
        let tick = interval.tick();
        // XXX: when do we break? do i want select_next_some here? i
        // kinda doubt it, None should signal end of connection.
        // XXX: arguably tx.close_channel never happens.. so maybe we don't care?
        select! {
            //msg = msg_stream.next().fuse() => {
            maybe_msg = msg_stream.next() => {
                let Some(msg) = maybe_msg else {
                    eprintln!("MESSAGE: break");
                    break;
                };
                eprintln!("MESSAGE: {:#?}", msg);
                // Should break when None?
                //client.on_client_message(msg)?;
                /*
                if let Some(msg) = maybe_msg {
                    eprintln!("MESSAGE: {:#?}", msg);
                    // Should break when None?
                    //client.on_client_message(msg)?;
                } else {
                    eprintln!("MESSAGE: break");
                    break;
                }
                */
            },
            //notif = rx.select_next_some() => {
            //notif = client.notifs_stream().unwrap().select_next_some() => {
            maybe_notif = client.notifs_stream().unwrap().next() => {
                if let Some(notif) = maybe_notif {
                    eprintln!("NOTIF: {:#?}", notif);
                    //client.on_server_notif(notif)?;
                } else {
                    eprintln!("NOTIF: break");
                    // Shouldn't ever happen, we never close the other side
                    break;
                }
            }
            //_ = tick.fuse() => {
            _ = tick => {
                session.ping(&[]).await;
                eprintln!("TICK");
            }
        }
        /*
        //let tasks = vec![msg_stream.next(), tick];
        use std::pin::Pin;
        pin!(tick);
        //let next = Box::pin(msg_stream.next());
        let tasks: Vec<Pin<Box<dyn futures::Future<Output = ()>>>> = vec![next, tick];
        let (result, idx, _) = future::select_all(tasks).await;
        */
    };

    // attempt to close connection gracefully
    //let _ = session.close(reason).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use async_stream::stream;
    use futures::stream;
    use futures_util::pin_mut;
    use tokio;

    use autoconnect_ws_sm::{
        session::{MockSession, SessionImpl},
        sm::{ClientMessage, Unidentified, WebPushClientState},
    };

    use super::*;

    #[actix_web::test]
    async fn it_works() {
        let session = MockSession::new();
        let mut client = WebPushClientState::Unidentified(Unidentified::new(
            Box::new(session),
            "foo".to_owned(),
        ));

        let stream = stream::iter(vec![Ok(actix_ws::Message::Nop), Ok(actix_ws::Message::Nop)]);

        let mut session2 = MockSession::new();
        _webpush_ws(client, &mut session2, stream).await.expect("foo");
    }

    #[actix_web::test]
    async fn ping() {
        // XXX: into_boxed_arc could also probably solve this? I think it requires Arc<Session>
        let smsession = MockSession::new();
        //let mut client = WebPushClient::new(<no sesion>, "foo")
        let mut client = WebPushClientState::Unidentified(Unidentified::new(
            Box::new(smsession),
            "foo".to_owned(),
        ));

        let mut hsession = MockSession::new();
        /*
        let stream1 = stream::once(Box::pin(async {
            Ok(actix_ws::Message::Close(None))
        }));
        let stream2 = stream::once(Box::pin(async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(actix_ws::Message::Nop)
        }));
        _webpush_ws(client, session2, stream::select(stream1, stream2)).await.expect("foo");
         */
        let s = stream! {
            //yield Ok(actix_ws::Message::Close(None));
            yield Ok(actix_ws::Message::Text("{}".into()));
            tokio::time::sleep(Duration::from_secs(1)).await;
            yield Ok(actix_ws::Message::Nop);
        };
        pin_mut!(s);
        hsession.expect_ping()
            .times(1)
            .returning(|_| Ok(()));
        // client::webpush_ws(session, s);
        // client.ws(session, s);
        _webpush_ws(client, &mut hsession, s).await.expect("foo");
        hsession.checkpoint();
    }
}
