#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

#[macro_use]
extern crate slog_scope;

use std::collections::HashMap;
use std::sync::Arc;
use std::{env, vec::Vec};

use actix_http::StatusCode;
use actix_web::middleware::ErrorHandlers;
use actix_web::{web, App, HttpServer};
use autoconnect_settings::ENV_PREFIX;
use docopt::Docopt;
use serde::Deserialize;
use std::sync::RwLock;

use autoconnect_settings::{options::ServerOptions, Settings};
use autoconnect_web::{
    client::{Client, ClientChannels},
    dockerflow,
};
use autopush_common::errors::{render_404, ApcErrorKind, Result};

mod middleware;

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

#[actix_web::main]
async fn main() -> Result<()> {
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
    let settings =
        Settings::with_env_and_config_files(&filenames).map_err(ApcErrorKind::ConfigError)?;

    //TODO: Eventually this will match between the various storage engines that
    // we support. For now, it's just the one, DynamoDB.
    // Perform any app global storage initialization.
    match autopush_common::db::StorageType::from_dsn(&settings.db_dsn) {
        autopush_common::db::StorageType::DynamoDb => {
            env::set_var("AWS_LOCAL_DYNAMODB", settings.db_dsn.clone().unwrap())
        }
        autopush_common::db::StorageType::INVALID => {
            panic!("Invalid Storage type. Check DB_DSN.");
        }
    }

    // Sentry requires the environment variable "SENTRY_DSN".
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
        ..Default::default()
    });

    let server_opts = ServerOptions::from_settings(&settings)?;

    info!("Starting autoconnect on port {:?}", &settings.port);
    HttpServer::new(move || {
        let client_channels: ClientChannels = Arc::new(RwLock::new(HashMap::new()));
        let _sentry = sentry::init(sentry::ClientOptions {
            attach_stacktrace: true,
            release: sentry::release_name!(),
            ..Default::default()
        });

        App::new()
            // Actix4 recommends using the `web::Data` wrapper when storing app_data.
            // internally, it uses Arc
            .app_data(web::Data::new(server_opts.clone()))
            .app_data(web::Data::new(client_channels))
            .wrap(ErrorHandlers::new().handler(StatusCode::NOT_FOUND, render_404))
            .wrap(crate::middleware::sentry::SentryWrapper::new(
                server_opts.metrics.clone(),
                ENV_PREFIX.to_owned(),
            ))
            // Websocket Handler
            .route("/ws/", web::get().to(Client::ws_handler))
            // TODO: Internode Message handler
            //.service(web::resource("/push/{uaid}").route(web::push().to(autoconnect_web::route::InterNode::put))
            .service(web::resource("/status").route(web::get().to(dockerflow::status_route)))
            .service(web::resource("/health").route(web::get().to(dockerflow::health_route)))
            .service(web::resource("/v1/err").route(web::get().to(dockerflow::log_check)))
            // standardized
            .service(web::resource("/__error__").route(web::get().to(dockerflow::log_check)))
            // Dockerflow
            .service(web::resource("/__heartbeat__").route(web::get().to(dockerflow::health_route)))
            .service(
                web::resource("/__lbheartbeat__")
                    .route(web::get().to(dockerflow::lb_heartbeat_route)),
            )
            .service(web::resource("/__version__").route(web::get().to(dockerflow::version_route)))
    })
    .bind(("0.0.0.0", settings.port))?
    .run()
    .await
    .map_err(|e| e.into())
    .map(|v| {
        info!("Shutting down autoconnect");
        v
    })
}
