use crate::server::extractors::user::RouterType;
use crate::server::routers::webpush::WebPushRouter;
use crate::server::routers::Router;
use crate::server::ServerState;
use actix_web::dev::{Payload, PayloadStream};
use actix_web::web::Data;
use actix_web::{FromRequest, HttpRequest};
use futures::future;

/// Holds the various notification routers. The routers use resources from the
/// server state, which is why `Routers` is an extractor.
pub struct Routers {
    pub webpush: WebPushRouter,
}

impl FromRequest for Routers {
    type Error = ();
    type Future = future::Ready<Result<Self, ()>>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload<PayloadStream>) -> Self::Future {
        let state = Data::<ServerState>::extract(&req)
            .into_inner()
            .expect("No server state found");

        future::ok(Routers {
            webpush: WebPushRouter {
                ddb: state.ddb.clone(),
                metrics: state.metrics.clone(),
                http: state.http.clone(),
            },
        })
    }
}

impl Routers {
    /// Get the router which handles the router type
    pub fn get(&self, router_type: RouterType) -> &dyn Router {
        match router_type {
            RouterType::WebPush => &self.webpush,
            RouterType::GCM => unimplemented!(),
            RouterType::FCM => unimplemented!(),
            RouterType::APNS => unimplemented!(),
            RouterType::ADM => unimplemented!(),
        }
    }
}
