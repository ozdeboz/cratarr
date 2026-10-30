mod acoustid;
mod albums;
// Decision 0035's core; the pass that uses it comes next.
#[allow(dead_code)]
mod analysis;
mod bandcamp;
mod api;
mod app;
mod catalogue;
mod config;
mod ctx;
mod convert;
mod covers;
mod covers_inside;
mod crates;
mod db;
mod diagnostics;
mod export;
mod folders;
mod idempotent;
mod incoming;
mod insights;
mod journal;
mod lidarr;
mod trash;
mod userdata;
mod jellyfin;
mod m3u;
mod memwatch;
mod mounts;
mod moves;
mod naming;
mod nowplaying;
mod opus_copies;
mod opus_copy;
mod placement;
mod playlist_rewrite;
#[allow(dead_code)] // the playlist sync uses it (decision 0012)
mod playlist_merge;
mod playlist_ops;
mod ports;
mod playlists;
mod retag;
mod rewind;
mod wind;
mod search;
mod serve;
mod settings;
mod similarity;
mod slskd;
mod snapcast;
mod source;
mod stream;
mod stream_forms;
mod sync_state;
mod voice;
mod web;

use std::io::{Read, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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

/// Every route behind the sign-in requirement (decision 0043), and with each connection's address: the sign-in page's
/// throttle reads it where no proxy names the client. Each request's id around all of it, the sign-in layer's refusals
/// too (decision 0045). The whole router, as `main` serves it and the tests sweep it (`whole_router`).
fn routes(app: std::sync::Arc<app::App>, served: std::sync::Arc<serve::Serve>) -> axum::Router {
    let on = app.play_on.clone();
    let kept = Some(served.userdata.clone());
    api::router(app)
        .merge(serve::routes::router(served.clone()))
        // Listening along and the guest portal (decision 0079).
        .merge(serve::along::router(on.clone(), served.clone()))
        .merge(serve::guest::router(on, served.clone()))
        // A change sent again with its key (a page's offline changes) made once, inside the access check (whose it is);
        // the answers kept 30 days in the user data (decision 0087).
        .layer(axum::middleware::from_fn_with_state(kept, idempotent::once))
        .layer(axum::middleware::from_fn_with_state(served, serve::access::require))
        .layer(axum::middleware::from_fn(api::request_ids))
}

#[cfg(test)]
mod whole_router;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        std::process::exit(healthcheck());
    }
    if std::env::args().nth(1).as_deref() == Some("--new-api-key") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(match serve::access::mint(&args) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        });
    }
    if std::env::args().nth(1).as_deref() == Some("--analyse") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(match analysis::batch::run(&args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("{e:#}");
                2
            }
        });
    }
    // The visuals' signals of files, for measuring what they cost (decision 0046, amended): each file's length, the time
    // its one decode took, and the bytes kept, by section.
    if std::env::args().nth(1).as_deref() == Some("--signals") {
        for path in std::env::args().skip(2) {
            let started = std::time::Instant::now();
            match analysis::analyse_with_signals(std::path::Path::new(&path), &[], None) {
                Ok((a, s)) => {
                    let sections = s.read().map(|m| m.keys().map(u8::to_string).collect::<Vec<_>>().join(",")).unwrap_or_default();
                    println!("{path}: {:.0} s of sound, {:.2} s, {} bytes ({:.1} KB a minute), sections {sections}", a.duration_secs, started.elapsed().as_secs_f64(), s.0.len(), s.0.len() as f64 / 1024.0 / (a.duration_secs / 60.0).max(1e-9));
                }
                Err(e) => println!("{path}: {e:#}"),
            }
        }
        return Ok(());
    }
    // Beat this! as the NAS hears new music (decision 0049, amended), for checking it against the PC's: a JSON line a
    // file, its beats and downbeats in milliseconds and the seconds it took.
    if std::env::args().nth(1).as_deref() == Some("--beats") {
        for path in std::env::args().skip(2) {
            let started = std::time::Instant::now();
            let line = match app::beat_this::hear_timed(std::path::Path::new(&path)) {
                Ok((h, spect, model)) => {
                    let ms = |v: &[f64]| v.iter().map(|t| (t * 1000.0).round() as u32).collect::<Vec<u32>>();
                    serde_json::json!({ "file": path, "beats": ms(&h.beats), "downbeats": ms(&h.downbeats), "secs": started.elapsed().as_secs_f64(), "spectrogram_secs": spect, "model_secs": model })
                }
                Err(e) => serde_json::json!({ "file": path, "error": format!("{e:#}") }),
            };
            println!("{line}");
        }
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--rewrite-playlists") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(match playlist_rewrite::run(&args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("{e:#}");
                2
            }
        });
    }
    // What is logged (RUST_LOG, else info) governs both what is printed and the diagnostics log, and can change while
    // Cratarr runs (decision 0045); the log itself opens once the data folder is known, keeping what came before.
    let base = diagnostics::base_directives();
    let (filter, reload) = tracing_subscriber::reload::Layer::new(diagnostics::filter(&base).expect("a valid filter"));
    let (kept, pending) = diagnostics::layer();
    tracing_subscriber::registry().with(filter).with(tracing_subscriber::fmt::layer()).with(kept).init();
    diagnostics::keep_levels(base, move |f| reload.reload(f).map_err(|e| e.to_string()));

    let env = |k: &str| std::env::var(k).ok();
    let cfg = config::Config::from_lookup(env)?;
    // Until Settings' saved values are read (`app::runtime`, once the library is open): the environment's.
    serve::proxy::set(cfg.proxies.clone());
    let kept_log = diagnostics::start_global(cfg.data_dir.as_deref().map(|d| std::path::Path::new(d).join("diagnostics.sqlite")).as_deref(), pending)
        .context("cannot start the diagnostics log")?;
    diagnostics::keep_panics();
    tracing::info!(target: "server", version = env!("CARGO_PKG_VERSION"), "started");
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
    // Cratarr's own folders were found by themselves before decision 0044; they are chosen now: today's, once.
    let mut carried = false;
    for s in saved.sources.iter_mut().filter(|s| s.kind == source::cratarr::KIND.id && !s.values.contains_key("folders")) {
        let found: Vec<serde_json::Value> = source::cratarr::found_folders().into_iter().map(|p| serde_json::Value::String(p.display().to_string())).collect();
        tracing::info!("folders: the {} music folders found are Cratarr's chosen folders now (decision 0044)", found.len());
        s.values.insert("folders".into(), serde_json::Value::Array(found));
        carried = true;
    }
    if carried && matches!(origin, settings::Origin::File)
        && let Some(d) = &cfg.data_dir
    {
        settings::save(&std::path::Path::new(d).join("settings.json"), &saved)?;
    }
    if saved.lidarr.is_none() {
        saved.lidarr = settings::lidarr_from_env(&env);
    }
    if saved.sources.is_empty() {
        tracing::warn!("no music source is set up yet: add one on the Settings page (or set JELLYFIN_URL)");
    }

    // A source that is down only costs its own features; an unreadable Music folder is fatal.
    let bind = cfg.bind.clone();
    let data_dir = cfg.data_dir.clone();
    let tag_rules = saved.tag_rules.clone();
    let served = serve::Serve::open(data_dir.as_deref().map(std::path::Path::new), tag_rules, &env)
        .context("cannot open the OpenSubsonic library")?;
    serve::set_instance(served.clone());
    // The environment's server settings, with what Settings saved over them (0.9.8).
    let runtime = app::runtime::start(app::runtime::Seed::from_env(&env), Some(&served.userdata), &cfg.bind);
    let (snapcast_bind, snapcast_host) = (cfg.snapcast_bind.clone(), runtime.snapcast_host.clone());
    // Cratarr on the local network, as a Kodi streams from it: the host players reach Snapcast on, and Cratarr's port.
    let lan_base = app::runtime::lan_base(&runtime);
    let app = app::App::new(cfg, saved, origin, Some(served.clone())).await.context("cannot open the catalogue")?;
    served.set_play_on(app.play_on.clone());
    app.play_on.set_lan_base(lan_base);
    // Sync play (decision 0037): groups' sound on Snapcast's protocol; Cratarr runs on without it if the port is taken.
    if let Some(bind) = snapcast_bind {
        match tokio::net::TcpListener::bind(&bind).await {
            Ok(listener) => {
                let port = listener.local_addr().map(|a| a.port()).unwrap_or(snapcast::PORT);
                let snap = snapcast::Snapcast::new(app.play_on.clone(), snapcast::BUFFER_MS);
                snapcast::Snapcast::set_instance(snap.clone());
                let _ = app.snapcast.set(snap.clone());
                app.play_on.serve_sync_play(port, snapcast::BUFFER_MS, snapcast_host);
                tokio::spawn(snap.serve(listener));
                tracing::info!("sync play on {bind}");
            }
            Err(e) => tracing::warn!("sync play is off: cannot listen on {bind}: {e}"),
        }
    }
    let n = app.playlists.count().await.context("cannot read the playlist folder")?;
    tracing::info!("found {n} playlists");
    let status = app.status().await;
    tracing::info!("sources: {}", if status.connected { status.server_name.unwrap_or_default() } else { "none connected".into() });

    // The catalogue fills and follows itself in the background (decision 0008).
    let linker = app.clone();
    tokio::spawn(catalogue::run(app.library.catalogue(), move || linker.library.connected_sources()));

    // Decision 0035: the NAS keeps the analysis up to date (after the ThinkBook's first pass, or all of it).
    memwatch::start();
    app::analysis_pass::run(app.library.catalogue());
    // The visuals' signals for the next tracks in each queue, before they play (decision 0046, amended).
    app::analysis_pass::look_ahead(app.library.catalogue(), app.play_on.clone());

    // The crates' favourites are asked for once at start, as the media server may take a minute (decision 0024).
    let warm = app.clone();
    tokio::spawn(async move { warm.playlists.warm_favourites().await });
    app.collection.follow(&app.journal);

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

    // The OpenSubsonic API (decision 0030), kept in step with the catalogue in the background. Opened before the app
    // (above), so the built-in Cratarr source can answer while the app starts (decision 0032).
    served.attach_catalogue(app.library.catalogue());
    served.set_playlists(app.playlists.clone());
    // Who is still in the directory's group, every hour (decision 0051).
    serve::accounts::watch_directory(served.clone());
    served.set_incoming(app.incoming.clone());
    served.follow();
    // Scrobbling: the plays queued for the connected listening services, sent (decision 0068).
    app::scrobble::run(served.clone());
    // Imports of plays and favourites read the sources in use (decision 0030, amended 2026-10-15).
    let importer = app.clone();
    served.import.connect(std::sync::Arc::new(move || importer.library.connected_sources()));

    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("cannot bind {bind}"))?;
    tracing::info!("listening on {bind}");
    let routes = routes(app.clone(), served);
    // Stopped (docker stop's SIGTERM, or Ctrl-C): no new requests, the players told why their connections end, and
    // what is still open (the players' and pages' live connections never end by themselves) cut after a few seconds.
    let (stopping, mut stopped) = tokio::sync::watch::channel(false);
    let server = axum::serve(listener, routes.into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(async move {
        stop_signal().await;
        tracing::info!(target: "server", "stopping");
        app.play_on.stopping();
        let _ = stopping.send(true);
    });
    tokio::select! {
        r = server => r?,
        () = async {
            let _ = stopped.wait_for(|s| *s).await;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        } => {}
    }
    kept_log.flush();
    // Gone now, not when the runtime's blocking work (a catalogue pass, an import) has finished: that held a restart
    // asked from Settings for a minute and a half (0.9.6). What is written is written; the databases commit as they go.
    std::process::exit(0)
}

/// A restart asked from Settings (0.9.6): Cratarr stops as `docker stop` stops it, and Docker's restart policy brings it
/// back.
pub(crate) static RESTART: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// SIGTERM (what docker stop sends), Ctrl-C, or a restart asked from Settings.
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = term.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                    () = RESTART.notified() => {}
                }
                return;
            }
            Err(e) => tracing::warn!("cannot listen for SIGTERM: {e}"),
        }
    }
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = RESTART.notified() => {}
    }
}
