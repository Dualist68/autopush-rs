extern crate slog;
#[macro_use]
extern crate slog_scope;
#[macro_use]
extern crate serde_derive;

use std::{env, sync::Arc, vec::Vec};

use actix::{Actor, ActorContext, StreamHandler};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use docopt::Docopt;

use autopush_common::errors::{ApiErrorKind, ApiResult};
use settings::Settings;

mod server;
mod settings;

struct AutoConnect;

const USAGE: &str = "
Usage: autopush_rs [options]

Options:
    -h, --help                          Show this message.
    --config-connection=CONFIGFILE      Connection configuration file path.
    --config-shared=CONFIGFILE          Common configuration file path.
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_config_connection: Option<String>,
    flag_config_shared: Option<String>,
}

impl Actor for AutoConnect {
    type Context = ws::WebsocketContext<Self>;
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for AutoConnect {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                // TODO: Add megaphone handling
                ctx.pong(&msg);
            }
            Ok(ws::Message::Text(msg)) => {
                // TODO: self.process_message(msg)
                info!("{:?}", msg);
            }
            _ => {
                error!("Unsupported socket message: {:?}", msg);
                ctx.stop();
                return;
            }
        }
    }
}

async fn ws_handler(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    info!("Starting connection...");
    let resp = ws::start(AutoConnect {}, &req, stream);
    info!("Shutting down: {:?}", &resp);
    resp
}

/// Modify the reported sentry event.
fn before_send(
    mut _event: sentry::protocol::Event<'static>,
) -> Option<sentry::protocol::Event<'static>> {
    Some(_event.clone())
}

#[actix_web::main]
async fn main() -> ApiResult<()> {
    env_logger::init();

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());
    let mut filenames = Vec::new();
    if let Some(shared_filename) = args.flag_config_shared {
        filenames.push(shared_filename);
    }
    if let Some(config_filename) = args.flag_config_connection {
        filenames.push(config_filename);
    }
    let settings = Settings::with_env_and_config_files(&filenames)
        .map_err(|e| ApiErrorKind::ConfigError(e))?;
    // TODO: move this into the DbClient setup
    if let Some(ref ddb_local) = settings.db_dsn {
        if autopush_common::db::StorageType::from_dsn(&ddb_local)
            == autopush_common::db::StorageType::DYNAMODB
        {
            env::set_var("AWS_LOCAL_DYNAMODB", ddb_local);
        }
    }

    // Sentry uses the environment variable "SENTRY_DSN".
    if env::var("SENTRY_DSN")
        .unwrap_or_else(|_| "".to_owned())
        .is_empty()
    {
        print!("SENTRY_DSN not set. Logging disabled.");
    }

    let _guard = sentry::init(sentry::ClientOptions {
        release: sentry::release_name!(),
        session_mode: sentry::SessionMode::Request, // new session per request
        auto_session_tracking: true,                // new session per request
        // attach_stacktrace: true, // attach a stack trace to ALL messages (not just exceptions)
        // send_default_pii: false, // do not include PII in message
        before_send: Some(Arc::new(Box::new(before_send))),
        ..Default::default()
    });

    let server_opts = server::options::ServerOptions::from_settings(&settings)?;

    HttpServer::new(move || {
        App::new()
            .app_data(server_opts.clone())
            //.wrap(ErrorHandlers::new().handler(StatusCode::NOT_FOUND, ApiError::render_404))
            .wrap(sentry_actix::Sentry::new()) // Use the default wrapper
            .route("/ws/", web::get().to(ws_handler))
            // Add router info
            .service(web::resource("/status").route(web::get().to(server::health::status_route)))
            .service(web::resource("/health").route(web::get().to(server::health::health_route)))
            .service(web::resource("/v1/err").route(web::get().to(server::health::log_check)))
            // standardized
            .service(web::resource("/__error__").route(web::get().to(server::health::log_check)))
            // Dockerflow
            .service(
                web::resource("/__heartbeat__").route(web::get().to(server::health::health_route)),
            )
            .service(
                web::resource("/__lbheartbeat__")
                    .route(web::get().to(server::health::lb_heartbeat_route)),
            )
            .service(
                web::resource("/__version__").route(web::get().to(server::health::version_route)),
            )
    })
    .bind(("0.0.0.0", settings.port))?
    .run()
    .await
    .map_err(|e| e.into())
}
