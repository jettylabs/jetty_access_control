//! Exploration of data access via Jetty
//!

#![deny(missing_docs)]

mod assets;
mod groups;
mod nodes;
mod static_server;
mod tags;
mod users;

use std::{net::SocketAddr, sync::Arc};

use axum::{extract::Extension, routing::get, Json, Router};
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower_http::trace::TraceLayer;

use jetty_core::{
    access_graph,
    logging::{debug, error, info, warn},
};

/// Launch the Jetty Explore web ui and accompanying server
pub async fn explore_web_ui(ag: Arc<access_graph::AccessGraph>) {
    // build our application with a route
    let app = Router::new()
        .nest("/api/", nodes::router())
        .nest("/api/user/", users::router())
        .nest("/api/group/", groups::router())
        .nest("/api/tag/", tags::router())
        .nest("/api/asset/", assets::router())
        .route("/api/last_fetch", get(last_fetch_handler))
        .fallback(get(static_server::static_handler))
        // `TraceLayer` is provided by tower-http so you have to add that as a dependency.
        // It provides good defaults but is also very customizable.
        //
        // See https://docs.rs/tower-http/0.1.1/tower_http/trace/index.html for more details.
        .layer(TraceLayer::new_for_http())
        .layer(Extension(ag));

    let app_service = app.into_make_service();

    // iterate through ports to find one that we can use
    for port in 3000..65535 {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        debug!("trying to bind on {}", addr);
        if let Ok(server) = axum::Server::try_bind(&addr) {
            info!("Serving Jetty explore on {}", addr);
            let open_url = format!("http://{}", &addr);
            // Open a web browser to the appropriate port
            match open::that(&open_url) {
                Ok(()) => debug!("Opened browser successfully."),
                Err(err) => error!(
                    "An error occurred when opening the browser to '{}': {}",
                    &open_url, err
                ),
            }
            server.serve(app_service.to_owned()).await.unwrap();
            break;
        }
    }
}

/// Return the timestamp of the last update to the Jetty graph
async fn last_fetch_handler(
    Extension(ag): Extension<Arc<access_graph::AccessGraph>>,
) -> Json<Value> {
    Json(json! { {"last_fetch_timestamp": ag.get_last_modified()} })
}