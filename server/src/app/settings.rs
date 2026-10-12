//! Settings (decision 0011): the page's view of everything configurable, checking a draft, and saving: the servers go
//! to the library, Lidarr and the trash days to Incoming.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::incoming::{IncomingService, LidarrView, lidarr_pair};
use super::library::{Active, Library, derive_folders, epoch_now, label_of};
use super::playlists::PlaylistService;
use super::{ToggleError, VERSION};
use crate::placement;
use crate::settings::{self, Origin, SourceConfig};
use crate::source::{self, Choice, Field, Source, Status};

/// Set by the container, so shown but not editable.
#[derive(Debug, Serialize)]
pub struct FixedSettings {
    pub bind: String,
}

#[derive(Debug, Serialize)]
pub struct KindView {
    pub id: &'static str,
    pub label: &'static str,
    pub fields: Vec<Field>,
}

#[derive(Debug, Serialize)]
pub struct SourceView {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub enabled: bool,
    /// Secrets left out; see `secrets_set`.
    pub values: source::Values,
    pub secrets_set: Vec<String>,
    pub status: Status,
    pub choices: BTreeMap<String, Vec<Choice>>,
    /// What its libraries are used for, as saved.
    pub libraries: settings::Roles,
    /// Its music libraries and where Cratarr sees their files (decision 0009).
    pub server_libraries: Vec<placement::PlacedLibrary>,
    /// Why the source could not be connected or its choices not filled, if so.
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub sources: Vec<SourceView>,
    pub kinds: Vec<KindView>,
    /// Whether the sources come from the settings file or (first run) the environment.
    pub origin: Origin,
    pub fixed: FixedSettings,
    /// `None` when CRATARR_DATA_DIR is unset: nothing can be saved.
    pub settings_file: Option<String>,
    pub playlists: usize,
    pub version: &'static str,
    /// Epoch seconds.
    pub started: i64,
    /// Where the Music libraries' files are, as Cratarr sees them: the playlists live there.
    pub music_folder: Option<String>,
    /// `None` when no Lidarr is set.
    pub lidarr: Option<LidarrView>,
    pub tag_rules: crate::catalogue::Rules,
    /// How long albums deleted from Incoming stay in the trash.
    pub trash_days: u32,
    pub catalogue: Option<crate::catalogue::Status>,
}

pub struct SettingsService {
    lib: Arc<Library>,
    playlists: Arc<PlaylistService>,
    incoming: Arc<IncomingService>,
    origin: std::sync::RwLock<Origin>,
    bind: String,
    settings_path: Option<PathBuf>,
    started: i64,
}

impl SettingsService {
    pub(super) fn new(
        lib: Arc<Library>,
        playlists: Arc<PlaylistService>,
        incoming: Arc<IncomingService>,
        bind: String,
        data_dir: Option<&str>,
        origin: Origin,
    ) -> Self {
        Self {
            lib,
            playlists,
            incoming,
            origin: std::sync::RwLock::new(origin),
            bind,
            settings_path: data_dir.map(|d| PathBuf::from(d).join("settings.json")),
            started: epoch_now(),
        }
    }

    fn kinds() -> Vec<KindView> {
        source::KINDS.iter().map(|k| KindView { id: k.id, label: k.label, fields: (k.fields)() }).collect()
    }

    /// Status, choices and a fresh look at where its libraries lie. `source` is `None` when it could not be
    /// connected.
    async fn view_of(&self, cfg: &SourceConfig, source: Option<&dyn Source>, error: Option<String>) -> SourceView {
        let (values, secrets_set) = settings::redact(cfg);
        let mut view = SourceView {
            id: cfg.id.clone(),
            kind: cfg.kind.clone(),
            label: label_of(cfg),
            enabled: cfg.enabled,
            values,
            secrets_set,
            status: Status::default(),
            choices: BTreeMap::new(),
            libraries: cfg.libraries.clone(),
            server_libraries: Vec::new(),
            error,
        };
        let Some(s) = source else { return view };
        let places = self.lib.place_folders();
        let (status, choices, placed) = tokio::join!(s.status(), s.options(), placement::place(s, &places));
        view.status = status.unwrap_or_default();
        match choices {
            Ok(c) => view.choices = c,
            Err(e) => {
                view.error.get_or_insert(format!("{e:#}"));
            }
        }
        match placed {
            Ok(p) => {
                view.server_libraries = tokio::task::spawn_blocking(move || {
                    let mut libs = p.libraries;
                    for l in &mut libs {
                        l.read_only = !l.folders.is_empty() && !l.folders.iter().all(|f| crate::mounts::writable(std::path::Path::new(f)));
                    }
                    libs
                })
                .await
                .unwrap_or_default();
            }
            Err(e) => {
                view.error.get_or_insert(format!("cannot list the libraries: {e:#}"));
            }
        }
        view
    }

    pub async fn view(&self) -> Result<SettingsView> {
        let mut sources = Vec::new();
        for a in self.lib.live().iter() {
            // A disabled source is shown with its choices too, so it can be checked before switching it on.
            let probe;
            let s: Option<&dyn Source> = match (&a.source, a.cfg.enabled) {
                (Some(s), _) => Some(&**s),
                (None, false) => {
                    probe = source::kind(&a.cfg.kind).and_then(|k| (k.connect)(&a.cfg.values, true).ok());
                    probe.as_deref()
                }
                (None, true) => None,
            };
            sources.push(self.view_of(&a.cfg, s, a.error.clone()).await);
        }
        let origin = *self.origin.read().unwrap_or_else(|p| p.into_inner());
        // Read before the struct below: a lock guard must not live across its awaits.
        let tag_rules = self.lib.tag_rules();
        let music_folder = self.lib.music();
        let (lidarr_cfg, lidarr_client) = self.incoming.lidarr_state();
        let lidarr = IncomingService::lidarr_view(lidarr_cfg.as_ref(), lidarr_client).await;
        Ok(SettingsView {
            sources,
            kinds: Self::kinds(),
            origin,
            fixed: FixedSettings { bind: self.bind.clone() },
            settings_file: self.settings_path.as_ref().map(|p| p.display().to_string()),
            playlists: self.playlists.count().await?,
            version: VERSION,
            started: self.started,
            music_folder,
            lidarr,
            tag_rules,
            trash_days: self.incoming.trash_days(),
            catalogue: {
                let c = self.lib.catalogue();
                tokio::task::spawn_blocking(move || c.status()).await.ok().and_then(Result::ok)
            },
        })
    }

    /// One source as the page is filling it in: connect with what is there (a blank secret means the saved one,
    /// under the same rules as saving) and show its status, choices and libraries. Nothing is saved.
    pub async fn preview(&self, draft: SourceConfig) -> Result<SourceView, ToggleError> {
        let saved: Vec<SourceConfig> = self.lib.live().iter().map(|a| a.cfg.clone()).collect();
        let cfg = settings::merge(&saved, vec![draft])?.remove(0);
        let kind = source::kind(&cfg.kind).ok_or_else(|| ToggleError::BadSettings("unknown kind of source".into()))?;
        let s = (kind.connect)(&cfg.values, true).map_err(|e| ToggleError::BadSettings(format!("{e:#}")))?;
        Ok(self.view_of(&cfg, Some(&*s), None).await)
    }

    /// Check a Lidarr as the page fills it in (a blank key means the saved one, under the rules of saving). Nothing
    /// is saved.
    pub async fn preview_lidarr(&self, draft: settings::LidarrConfig) -> Result<Option<LidarrView>, ToggleError> {
        let saved = self.incoming.lidarr_state().0;
        let cfg = settings::merge_lidarr(saved.as_ref(), draft)?;
        let client = cfg.as_ref().and_then(|c| crate::lidarr::Lidarr::new(&c.url, &c.api_key).ok());
        Ok(IncomingService::lidarr_view(cfg.as_ref(), client).await)
    }

    /// Check every enabled source against its server, then save the list and switch over. Nothing is saved if a
    /// check fails. The Music libraries' files hold the playlists, so Cratarr must see them, all in one folder.
    /// `tag_rules`: `None` keeps what is saved.
    /// `lidarr`: `None` keeps the saved one; a blank address removes it.
    pub async fn save(
        &self,
        drafts: Vec<SourceConfig>,
        tag_rules: Option<crate::catalogue::Rules>,
        lidarr: Option<settings::LidarrConfig>,
        trash_days: Option<u32>,
    ) -> Result<SettingsView, ToggleError> {
        let Some(path) = self.settings_path.clone() else { return Err(ToggleError::SettingsReadOnly) };
        let _one_at_a_time = self.lib.changing.lock().await;
        let old_lidarr = self.incoming.lidarr_state().0;
        let lidarr = match lidarr {
            Some(draft) => settings::merge_lidarr(old_lidarr.as_ref(), draft)?,
            None => old_lidarr,
        };
        let lidarr = lidarr_pair(lidarr);
        if let Some(cfg) = &lidarr.0 {
            let client = lidarr.1.as_ref().ok_or_else(|| ToggleError::BadSettings("Lidarr: the API key is not usable".into()))?;
            client.version().await.map_err(|e| ToggleError::BadSettings(format!("Lidarr at {}: {e:#}", cfg.url)))?;
        }
        let old_rules = self.lib.tag_rules();
        let places = self.lib.place_folders();
        let tag_rules = tag_rules.unwrap_or(old_rules);
        let trash_days = trash_days.unwrap_or_else(|| self.incoming.trash_days());
        if !(1..=3650).contains(&trash_days) {
            return Err(ToggleError::BadSettings("keep deleted albums for 1 to 3650 days".into()));
        }
        let saved: Vec<SourceConfig> = self.lib.live().iter().map(|a| a.cfg.clone()).collect();
        let merged = settings::merge(&saved, drafts)?;
        let mut next = Vec::with_capacity(merged.len());
        for cfg in merged.iter().cloned() {
            let a = Active::start(cfg, &places).await;
            if a.cfg.enabled {
                let bad = |why: String| ToggleError::BadSettings(format!("{}: {why}", a.cfg.id));
                let s = a.source.as_ref().ok_or_else(|| bad(a.error.clone().unwrap_or_else(|| "cannot connect".into())))?;
                let st = s.status().await.map_err(|e| bad(format!("{e:#}")))?;
                if !st.connected {
                    return Err(bad(st.error.unwrap_or_else(|| "not connected".into())));
                }
                for id in &a.cfg.libraries.music {
                    match a.placement.libraries.iter().find(|l| &l.id == id) {
                        None => return Err(bad("a Music library is no longer on the server".into())),
                        Some(l) if l.folders.is_empty() => {
                            return Err(ToggleError::BadSettings(format!("Cratarr can't see the files of {}", l.name)));
                        }
                        Some(_) => {}
                    }
                }
            }
            next.push(Arc::new(a));
        }
        let mut music_folders: Vec<String> = next
            .iter()
            .filter(|a| a.source.is_some())
            .flat_map(|a| {
                a.cfg.libraries.music.iter().filter_map(|id| a.placement.libraries.iter().find(|l| &l.id == id)?.folders.first().cloned())
            })
            .collect();
        music_folders.sort();
        music_folders.dedup();
        if music_folders.len() > 1 {
            return Err(ToggleError::BadSettings(format!(
                "the Music libraries are in different folders ({}); playlists live in one",
                music_folders.join(", ")
            )));
        }
        let music_folder = derive_folders(&next, self.lib.music().as_deref()).music;
        let to_save = settings::Saved { sources: merged, music_folder, tag_rules: tag_rules.clone(), lidarr: lidarr.0.clone(), trash_days };
        tokio::task::spawn_blocking(move || settings::save(&path, &to_save))
            .await
            .map_err(|e| anyhow::anyhow!("saving settings: {e}"))??;
        self.lib.set_tag_rules(tag_rules);
        self.incoming.apply_settings(lidarr, trash_days);
        self.lib.switch_to(next).await?;
        // Servers may have changed: link their tracks again soon.
        self.lib.catalogue().poke();
        *self.origin.write().unwrap_or_else(|p| p.into_inner()) = Origin::File;
        self.incoming.forget_lists().await;
        tracing::info!("settings saved and applied");
        drop(_one_at_a_time);
        Ok(self.view().await?)
    }
}
