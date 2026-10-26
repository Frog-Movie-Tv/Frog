use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use frog::{config::Config, db, jobs, routes, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // .env is a convenience for running outside Docker; real environment
    // variables always win.
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "frog=info,tower_http=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().compact())
        .init();

    let config = Config::from_env()?;
    if !config.has_tmdb_key() {
        tracing::warn!(
            "TMDB_API_KEY is empty — search and metadata will fail until you set it in .env"
        );
    }

    let pool = db::connect(&config.database_url).await?;
    tracing::info!(database = %config.database_url, "database ready");

    let bind = config.bind;
    let state = AppState::new(pool, config);

    routes::auth::bootstrap_from_env(&state).await?;
    let worker = jobs::spawn(state.clone());

    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("Frog is listening on http://{bind}");

    // With connect info, so sign-in throttling can tell callers apart when
    // there is no proxy in front setting X-Forwarded-For.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    worker.abort();
    tracing::info!("bye");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("can install the Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("can install the SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
