mod albums;
mod api;
mod app;
mod config;
mod covers;
mod jellyfin;
mod m3u;
mod nowplaying;
mod placement;
mod playlists;
mod settings;
mod source;
mod web;

use std::io::{Read, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

/// `cratarr-server --healthcheck`: for the container's health check (the image has no curl). Exit code 0 when
/// the local server answers its health endpoint with 200.
fn healthcheck() -> i32 {
    let port = std::env::var("CRATARR_BIND")
        .ok()
        .and_then(|b| b.rsplit(':').next().map(str::to_string))
        .unwrap_or_else(|| "8080".into());
    let Ok(mut s) = std::net::TcpStream::connect(format!("127.0.0.1:{port}")) else { return 1 };
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    if s.write_all(b"GET /api/health HTTP/1.0\r\nHost: localhost\r\n\r\n").is_err() {
        return 1;
    }
    let mut reply = String::new();
    s.read_to_string(&mut reply).ok();
    i32::from(!reply.starts_with("HTTP/1.1 200") && !reply.starts_with("HTTP/1.0 200"))
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        std::process::exit(healthcheck());
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let env = |k: &str| std::env::var(k).ok();
    let cfg = config::Config::from_lookup(env)?;
    for retired in config::RETIRED {
        if env(retired).is_some() {
            tracing::warn!("{retired} is no longer used: Cratarr finds the library paths itself (decision 0005)");
        }
    }

    // Sources saved from the page, or on first run one made from the JELLYFIN_* variables; see decision 0005.
    let saved = match &cfg.data_dir {
        Some(d) => settings::load(&std::path::Path::new(d).join("settings.json"), &env)?,
        None => None,
    };
    let (sources, origin) = match saved {
        Some(s) => (s, settings::Origin::File),
        None => (settings::from_env(&env), settings::Origin::Env),
    };
    if sources.is_empty() {
        tracing::warn!("no music source is set up yet: add one on the Settings page (or set JELLYFIN_URL)");
    }

    // A source that is down only costs its own features; an unreadable playlist folder is fatal.
    let bind = cfg.bind.clone();
    let app = app::App::new(cfg, sources, origin).await;
    let n = app.playlist_count().await.context("cannot read the playlist folder")?;
    tracing::info!("found {n} playlists");
    let status = app.status().await;
    tracing::info!("sources: {}", if status.connected { status.server_name.unwrap_or_default() } else { "none connected".into() });

    // Files come and go in the library without the playlists changing, so re-check dead entries now and then; and
    // look again for sources whose libraries could not be found yet (server down at start).
    let checker = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        tick.tick().await;
        loop {
            tick.tick().await;
            checker.refresh_dead().await;
            checker.retry_placement().await;
        }
    });

    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("cannot bind {bind}"))?;
    tracing::info!("listening on {bind}");
    axum::serve(listener, api::router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
