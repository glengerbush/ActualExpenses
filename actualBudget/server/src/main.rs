mod accounts;
mod api;
mod calculator;
mod categories;
mod companion;
mod concurrency;
mod error;
mod imports;
mod ledger;
mod lifecycle;
mod migrations;
mod models;
mod origin;
mod payees_tags;
mod preferences;
mod splits;
mod store;
mod themes;
use std::path::PathBuf;
#[tokio::main(worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        PathBuf::from(std::env::var("ACTUALBUDGET_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let store = store::Store::open(&directory).map_err(|error| error.message)?;
    let host = std::env::var("ACTUALBUDGET_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("ACTUALBUDGET_PORT")
        .unwrap_or_else(|_| "5006".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
    let policy = origin::OriginPolicy::from_env()?;
    let web = PathBuf::from(
        std::env::var("ACTUALBUDGET_WEB_DIR").unwrap_or_else(|_| "../web/dist".into()),
    );
    let assets = tower_http::services::ServeDir::new(&web)
        .fallback(tower_http::services::ServeFile::new(web.join("index.html")));
    let app =
        api::router(store)
            .fallback_service(assets)
            .layer(axum::middleware::from_fn_with_state(
                policy,
                origin::protect,
            ));
    println!("LISTENING {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=terminate.recv()=>{} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
