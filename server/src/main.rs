use cratarr_api::{analysis, api, app, catalogue, config, diagnostics, fixtures, idempotent, journal, mcp, memwatch, run, runner, serve, settings, snapcast, source};
// What the route sweep's tests (whole_router.rs) name besides.
#[cfg(test)]
use cratarr_api::{bandcamp, playlists, search};

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
    api::router(app.clone())
        .merge(serve::routes::router(served.clone()))
        // Cratarr as an MCP server (0.13): for API keys only, read only.
        .merge(mcp::router(app))
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
// What Home Assistant reads of Cratarr, pinned (HA-CONTRACT-0.18).
#[cfg(test)]
mod home_assistant;
// The one command path through the router (SCENARIOS-GF, GF's core).
#[cfg(test)]
mod one_path;

#[cfg(test)]
mod lookups_dc2;


/// What the arguments ask (W4): with none, the server; a known flag, its own work; anything else, nothing at all. So a flag
/// mistyped, or one this version lacks (`--version` before 0.17), never starts a second server on a live data folder.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    /// No argument: the server, on the data folder.
    Server,
    /// `--version` alone: the version printed, nothing opened.
    Version,
    Healthcheck,
    NewApiKey,
    MakeFixtures,
    MakeLayout,
    MakeGateCases,
    Analyse,
    Signals,
    Beats,
    /// Anything else, as given.
    Unknown(String),
}

/// The known flags, as the answer to an unknown argument lists them.
const KNOWN: &str = "--version | --healthcheck | --new-api-key <name> --out <file> | --make-fixtures <folder> | --make-layout <folder> | --make-gate-cases <folder> | --analyse <music folder> --out <file> [--threads N] | --signals <file>... | --beats <file>... | none: the server";

/// The arguments after the program's name, read once (W4).
fn mode(args: &[String]) -> Mode {
    let Some(first) = args.first() else { return Mode::Server };
    match first.as_str() {
        "--version" if args.len() == 1 => Mode::Version,
        "--healthcheck" => Mode::Healthcheck,
        "--new-api-key" => Mode::NewApiKey,
        "--make-fixtures" => Mode::MakeFixtures,
        "--make-layout" => Mode::MakeLayout,
        "--make-gate-cases" => Mode::MakeGateCases,
        "--analyse" => Mode::Analyse,
        "--signals" => Mode::Signals,
        "--beats" => Mode::Beats,
        _ => Mode::Unknown(args.join(" ")),
    }
}

/// The fixture library (decision 0093): `--make-fixtures <folder>`, made anew there; `--make-layout <folder>`, every kind of
/// library. The end-to-end suite is a program of its own: this is its way to them.
fn make_and_leave(flag: &str, make: fn(&std::path::Path) -> Result<Vec<std::path::PathBuf>>, dir: Option<&String>) -> ! {
    let Some(dir) = dir else {
        eprintln!("{flag} <folder>");
        std::process::exit(2);
    };
    std::process::exit(match make(std::path::Path::new(dir)) {
        Ok(albums) => {
            println!("{} albums in {dir}", albums.len());
            0
        }
        Err(e) => {
            eprintln!("{e:#}");
            1
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match mode(&args) {
        Mode::Server => {}
        // Before anything is opened or logged: nothing of the data folder is read, so it is safe in a live container.
        Mode::Version => {
            println!("cratarr-server {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        Mode::Unknown(given) => {
            eprintln!("cratarr-server: unknown argument: {given}\nknown: {KNOWN}");
            std::process::exit(2);
        }
        Mode::Healthcheck => std::process::exit(healthcheck()),
        Mode::NewApiKey => std::process::exit(match serve::access::mint(&args[1..]) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        }),
        Mode::MakeFixtures => make_and_leave("--make-fixtures", fixtures::make, args.get(1)),
        Mode::MakeLayout => make_and_leave("--make-layout", fixtures::make_layout, args.get(1)),
        Mode::MakeGateCases => make_and_leave("--make-gate-cases", fixtures::gate_cases, args.get(1)),
        Mode::Analyse => std::process::exit(match analysis::batch::run(&args[1..]) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("{e:#}");
                2
            }
        }),
        // The visuals' signals of files, for measuring what they cost (decision 0046, amended): each file's length, the time
        // its one decode took, and the bytes kept, by section.
        Mode::Signals => {
            for path in &args[1..] {
                let started = std::time::Instant::now();
                match analysis::analyse_with_signals(std::path::Path::new(path), &[], None) {
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
        Mode::Beats => {
            for path in &args[1..] {
                let started = std::time::Instant::now();
                let line = match app::beat_this::hear_timed(std::path::Path::new(path)) {
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

    // A file from before 0.16 holds its libraries' roles: read as 0.16's list (decision 0101), and written so once the
    // migration report keeps what they were.
    let libraries_report = match &cfg.data_dir {
        Some(d) => settings::libraries_migration(&std::path::Path::new(d).join("settings.json"))?,
        None => None,
    };
    // A file from before 0.18: each library gets a copy of the install's tag rules (OQ-GEN-22, GEN.62), and is written so once
    // the migration report keeps it.
    let tag_rules_report = match &cfg.data_dir {
        Some(d) => settings::tag_rules_migration(&std::path::Path::new(d).join("settings.json"))?,
        None => None,
    };
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
    if carried && libraries_report.is_none() && tag_rules_report.is_none() && matches!(origin, settings::Origin::File)
        && let Some(d) = &cfg.data_dir
    {
        settings::save(&std::path::Path::new(d).join("settings.json"), &saved)?;
    }
    let as_read = (libraries_report.is_some() || tag_rules_report.is_some()).then(|| saved.clone());
    saved.lidarr = settings::lidarr_at_start(&saved, &env);
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
    // RN's start, once the first start has moved what it moves (0.15's stores and journal, inside `App::new`) and before any pass asks a
    // source (each waits minutes first). The app's journal is the server's (RN's R1: the saved queues and the runner's jobs in it).
    journal::set_instance(app.journal.clone());
    // 0.18's step, RN's parts (DI06): 0.17's bulk queue and its kept holds onto the journal's rows, before the turns take the holds up.
    // An install upgrading from 0.17 is one whose settings file was from before 0.18 (its tag rules copied to each library now).
    let upgrading = tag_rules_report.is_some();
    app::step_018::run(&app.journal, data_dir.as_deref().map(std::path::Path::new), &served.userdata, app::moment::now_secs(), upgrading).context("cannot finish 0.18's step")?;
    // Every outside service's hold kept across a restart taken up before anything is asked (OQ-T3), Bandcamp's day after its bot
    // check among them (Y8); their holds kept from now on, in the journal's file (RN's R4).
    app::turn::keep_all(&app.journal, &[&app::discogs::TURN, &app::lastfm::FM]);
    // The runner's jobs (RN): those a stop cut off read back and said so, before any job begins; the finished pruned each hour.
    runner::runner().attach(app.journal.clone()).context("cannot read the runner's jobs")?;
    runner::runner().prune_hourly();
    // Before the first build and the first request: which copy plays turns on them.
    served.library.set_kinds(app.library.folder_kinds());
    served.set_rules(app.library.folder_rules());
    // The libraries' part of the migration report (decision 0101): what the roles held, and what the servers, answering now,
    // do not list. The tag rules' part (OQ-GEN-22). Then the file as 0.18's.
    if let (Some(file), Some(d)) = (as_read, data_dir.as_deref()) {
        if let Some(mut report) = libraries_report {
            let unlisted = app.library.unlisted_libraries();
            if let Some(u) = report.get_mut("unmatched").and_then(serde_json::Value::as_array_mut) {
                u.push(serde_json::json!({ "what": "libraries no server lists", "count": unlisted.len(), "sample": unlisted.iter().take(10).collect::<Vec<_>>() }));
            }
            served.userdata.set_migration_part("libraries", &report).context("cannot keep the migration report")?;
            tracing::info!("settings: the libraries' roles are 0.16's list now (decision 0101)");
        }
        // Kept once: a file an older Cratarr wrote again is read the same way, but the step's report is the upgrade's.
        if let Some(report) = tag_rules_report
            && !served.userdata.migration_report().context("cannot read the migration report")?.contains_key("tag_rules")
        {
            served.userdata.set_migration_part("tag_rules", &report).context("cannot keep the migration report")?;
            tracing::info!("settings: each library has tag rules of its own now, a copy of the install's (OQ-GEN-22)");
        }
        settings::save(&std::path::Path::new(d).join("settings.json"), &file)?;
    }
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
                crate::run::spawn(snap.serve(listener));
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
    crate::run::spawn(catalogue::run(app.library.catalogue(), move || linker.library.connected_sources()));

    // Decision 0035: the NAS keeps the analysis up to date (after a PC's first pass, or all of it).
    memwatch::start();
    app::analysis_pass::run(app.library.catalogue());
    // The visuals' signals for the next tracks in each queue, before they play (decision 0046, amended).
    app::analysis_pass::look_ahead(app.library.catalogue(), app.play_on.clone(), app.served.clone());

    // The crates' favourites are asked for once at start, as the media server may take a minute (decision 0024).
    let warm = app.clone();
    crate::run::spawn(async move { warm.crates.warm_favourites().await });
    app.collection.follow(&app.journal);
    // A run of file changes a stop cut off: read now, before anything can write what it was to write.
    crate::run::spawn(app::file_runs::settle_cut_off(app.journal.clone()));

    // One background pass keeps Cratarr in step with the sources and the library (see `App::sync`); the first runs
    // at once, so the incoming lists are ready soon after start.
    let syncer = app.clone();
    crate::run::spawn(async move {
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
    // 0.18's step, its part `facts` (VR42), once the catalogue is attached (what it and the fold read through `served`): 0.17's lists keyed by reference, its counts and years carried, once.
    let facts = app::references::step_018_facts(&served).context("cannot finish 0.18's step (facts)")?;
    tracing::info!(target: "migration", "0.18's facts: {}", serde_json::to_string(&facts.counts).unwrap_or_default());
    // RV12 (d): the records' due read, a schedule on the runner (one record a tick, never in bulk).
    if let Err(e) = app::references::start_due_reads() {
        tracing::warn!(target: "library", "the records' due read: {e}");
    }
    // Its part `wm` (WM.22), before the fold (T-WM-5): a record named by a site's stamp named again by the tag rule, 0.17's
    // expectations seeded as placements, a record made by a stamp merged into the one its placement names; the raw tags kept.
    // Its seeds: Soulseek's items 0.17 left Placed, each with its placing folder (T-WM-9), read from `soulseek.json`.
    let seeds = served.data_dir.as_deref().map(|d| app::soulseek::placed_seeds(d, served.catalogue())).unwrap_or_default();
    if let Some((report, merges)) = served.catalogue().step_018_wm(&seeds, &|| Ok(())).context("cannot finish 0.18's step (wm)")? {
        tracing::info!(target: "migration", "0.18's wm: {}", serde_json::to_string(&report.counts).unwrap_or_default());
        for (from, into) in merges {
            tracing::info!(target: "migration", "0.18's wm: al-{from} into al-{into}");
        }
    }
    // Its part `fold` (FA22): F40's pairs, one tracklist one record, joined once; each merge said for the HA session.
    if let Some(folded) = served.catalogue().fold_018(&served.library.kinds(), &|| Ok(())).context("cannot finish 0.18's step (fold)")? {
        for (from, into) in folded {
            tracing::info!(target: "migration", "0.18's fold: al-{from} into al-{into}");
        }
    }
    served.set_playlists(app.crates.clone());
    // Who is still in the directory's group, every hour (decision 0051).
    serve::accounts::watch_directory(served.clone());
    served.set_incoming(app.incoming.clone());
    served.follow();
    // Scrobbling: the plays queued for the connected listening services, sent (decision 0068).
    app::scrobble::run(served.clone());
    // Soulseek in Cratarr (0.15): Lidarr's Wanted searched, records fetched on a tick, once a library takes them.
    app::soulseek::start(&app);
    // The single fetches a stop cut off followed again by their batches (RN's DQ3, DQ5); the bulk queue's worker with them.
    crate::run::spawn(async { app::track_fetch::resume().await });
    app::track_intake::resume(&app);
    // Records not had that nothing keeps and nobody saw for six months, forgotten once a day (RT01), with their kept lists and
    // the artists left with nothing (RT01b): after Soulseek's list is read, whose items keep theirs (read sooner, an empty list
    // would be begun and written over the kept one), and the catalogue attached.
    app::retention::start(served.clone());
    app::requests::start(&app);
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

/// A restart asked from Settings: in `api`, which asks it (SPLIT-PLAN P9).
use api::RESTART;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// P4.3 (W4): each known flag is its mode, no argument the server, anything else unknown, as given; the image starts the
    /// binary with no argument and checks its health with `--healthcheck`.
    #[test]
    fn the_known_flags_are_the_known_flags() {
        let of = |args: &[&str]| mode(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
        assert_eq!(of(&[]), Mode::Server);
        assert_eq!(of(&["--version"]), Mode::Version);
        assert_eq!(of(&["--healthcheck"]), Mode::Healthcheck);
        assert_eq!(of(&["--new-api-key", "phone", "--out", "/tmp/key"]), Mode::NewApiKey);
        assert_eq!(of(&["--make-fixtures", "/tmp/fixtures"]), Mode::MakeFixtures);
        assert_eq!(of(&["--make-fixtures"]), Mode::MakeFixtures, "its own usage line, as before");
        assert_eq!(of(&["--make-layout", "/tmp/layout"]), Mode::MakeLayout);
        assert_eq!(of(&["--analyse", "/music", "--out", "/tmp/analysis"]), Mode::Analyse);
        assert_eq!(of(&["--signals", "a.flac", "b.flac"]), Mode::Signals);
        assert_eq!(of(&["--beats", "a.flac"]), Mode::Beats);
        for unknown in [&["--versoin"][..], &["-v"], &["version"], &["--version", "extra"], &["--version extra"], &["--help"], &[""], &["/data"]] {
            assert_eq!(of(unknown), Mode::Unknown(unknown.join(" ")), "{unknown:?}");
        }
        for flag in ["--version", "--healthcheck", "--new-api-key", "--make-fixtures", "--make-layout", "--analyse", "--signals", "--beats"] {
            assert!(KNOWN.contains(flag), "{flag} among the known: {KNOWN}");
        }
        let image = include_str!("../../Dockerfile");
        assert!(image.contains(r#"ENTRYPOINT ["/cratarr-server"]"#), "the image starts the server with no argument");
        assert!(image.contains(r#"CMD ["/cratarr-server", "--healthcheck"]"#), "and checks it with --healthcheck");
    }

    /// `--make-gate-cases <folder>` is a known flag (W4), its own work, never a server (`fixtures::gate`).
    #[test]
    fn the_gate_cases_flag_is_known() {
        let of = |args: &[&str]| mode(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
        assert_eq!(of(&["--make-gate-cases", "/tmp/cases"]), Mode::MakeGateCases);
        assert_eq!(of(&["--make-gate-cases"]), Mode::MakeGateCases, "its own usage line");
        assert!(KNOWN.contains("--make-gate-cases <folder>"), "{}", KNOWN);
    }
}
