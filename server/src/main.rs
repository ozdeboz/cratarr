mod albums;
mod api;
mod app;
mod catalogue;
mod config;
mod covers;
mod incoming;
mod insights;
mod journal;
mod lidarr;
mod trash;
mod jellyfin;
mod m3u;
mod mounts;
mod nowplaying;
mod placement;
#[allow(dead_code)] // the playlist sync uses it (decision 0012)
mod playlist_merge;
mod ports;
mod playlists;
mod settings;
mod source;
mod sync_state;
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
        // The tag reader warns about every MP3 whose length it has to estimate; that is normal and would flood the log.
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
                .add_directive("lofty=error".parse().expect("a valid directive")),
        )
        .init();

    let env = |k: &str| std::env::var(k).ok();
    let cfg = config::Config::from_lookup(env)?;
    for retired in config::RETIRED {
        if env(retired).is_some() {
            tracing::warn!("{retired} is no longer used and can be removed (decision 0005)");
        }
    }

    // Sources saved from the page, or on first run one made from the JELLYFIN_* variables; see decision 0005.
    let saved = match &cfg.data_dir {
        Some(d) => settings::load(&std::path::Path::new(d).join("settings.json"), &env)?,
        None => None,
    };
    let (saved, origin) = match saved {
        Some(s) => (s, settings::Origin::File),
        None => (settings::Saved { sources: settings::from_env(&env), ..Default::default() }, settings::Origin::Env),
    };
    // Lidarr from LIDARR_URL / LIDARR_API_KEY until one is saved in Settings.
    let mut saved = saved;
    if saved.lidarr.is_none() {
        saved.lidarr = settings::lidarr_from_env(&env);
    }
    // `cratarr-server --move-playlists`: the one-off move of the playlist files into Music/.playlists, out of
    // Jellyfin's reach (decision 0012). Safe to run again; prints what it did. Run it while the server is stopped,
    // or restart the server afterwards so it reads the files from there.
    if std::env::args().nth(1).as_deref() == Some("--move-playlists") {
        let music = saved.music_folder.clone().or(cfg.playlist_dir.clone()).context("no Music folder is known yet")?;
        let moved = playlists::move_into_hidden_folder(std::path::Path::new(&music))?;
        println!("moved: {}", moved.moved.len());
        println!("already there: {}", moved.already.len());
        for c in &moved.conflicts {
            println!("conflict, both kept: {c}");
        }
        std::process::exit(i32::from(!moved.conflicts.is_empty()));
    }
    if saved.sources.is_empty() {
        tracing::warn!("no music source is set up yet: add one on the Settings page (or set JELLYFIN_URL)");
    }

    // A source that is down only costs its own features; an unreadable Music folder is fatal.
    let bind = cfg.bind.clone();
    let app = app::App::new(cfg, saved, origin).await.context("cannot open the catalogue")?;
    let n = app.playlists.count().await.context("cannot read the playlist folder")?;
    tracing::info!("found {n} playlists");
    let status = app.status().await;
    tracing::info!("sources: {}", if status.connected { status.server_name.unwrap_or_default() } else { "none connected".into() });

    // The catalogue fills and follows itself in the background (decision 0008).
    let linker = app.clone();
    tokio::spawn(catalogue::run(app.library.catalogue(), move || linker.library.connected_sources()));

    // One background pass keeps Cratarr in step with the sources and the library (see `App::sync`); the first runs
    // at once, so the incoming lists are ready soon after start.
    let syncer = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(app::SYNC_SECS));
        let mut pass: u64 = 0;
        loop {
            tick.tick().await;
            syncer.sync(pass).await;
            pass += 1;
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
