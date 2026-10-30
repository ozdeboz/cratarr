//! Settings (decision 0011): the page's view of everything configurable, checking a draft, and saving: the servers go
//! to the library, Lidarr and the trash days to Incoming.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::incoming::{IncomingService, LidarrView, lidarr_pair};
use super::library::{Active, Library, derive_folders, epoch_now, label_of};
use super::playlists::PlaylistService;
use super::{ToggleError, VERSION};
use crate::placement;
use crate::ports::Files;
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

#[derive(Debug, Clone, Serialize)]
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
    /// Each used library's format and colour as they apply (its own, else its use's and the palette's; decision 0034).
    pub policies: BTreeMap<String, EffectivePolicy>,
    /// Why the source could not be connected or its choices not filled, if so.
    pub error: Option<String>,
}

/// A used library's settings as they apply (decision 0034).
#[derive(Debug, Clone, Serialize)]
pub struct EffectivePolicy {
    pub format: settings::Format,
    /// None for a Music library (the primary).
    pub colour: Option<String>,
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
    /// The saved naming template of Music; `None`: the one the library follows (see `NamingView`).
    pub naming: Option<String>,
    /// The libraries' colours to choose from (decision 0034), as a new one gets them.
    pub palette: &'static [&'static str],
}

/// A naming template measured against the Music library (decision 0013), for the Settings page.
#[derive(Debug, Serialize)]
pub struct NamingView {
    /// The one measured: the one asked about, else the saved one, else the one the library follows.
    pub template: String,
    pub folders: crate::naming::Share,
    pub files: crate::naming::Share,
    /// An album of the library named by it: `folder/first file`.
    pub example: Option<String>,
}

/// How long the library's names stay in memory for checking templates.
const NAMES_SECS: i64 = 600;

pub struct SettingsService {
    lib: Arc<Library>,
    files: Arc<dyn Files>,
    playlists: Arc<PlaylistService>,
    incoming: Arc<IncomingService>,
    origin: std::sync::RwLock<Origin>,
    bind: String,
    settings_path: Option<PathBuf>,
    started: i64,
    /// The Music library's albums as naming sees them, and when they were read (epoch seconds).
    names: tokio::sync::Mutex<Option<(i64, Arc<Vec<crate::naming::Album>>)>>,
    /// What each server and Lidarr said, by what was asked (their settings, the folders), for [ASKED_FOR] (decided
    /// 2026-10-23: a page that asked for the settings over and over asked Jellyfin as often).
    asked: std::sync::Mutex<HashMap<String, (std::time::Instant, Asked)>>,
}

/// How long a server's answers for the Settings page are used again.
const ASKED_FOR: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone)]
enum Asked {
    Source(SourceView),
    Lidarr(Option<LidarrView>),
}

impl SettingsService {
    pub(super) fn new(
        lib: Arc<Library>,
        files: Arc<dyn Files>,
        playlists: Arc<PlaylistService>,
        incoming: Arc<IncomingService>,
        bind: String,
        data_dir: Option<&str>,
        origin: Origin,
    ) -> Self {
        Self {
            lib,
            files,
            playlists,
            incoming,
            origin: std::sync::RwLock::new(origin),
            bind,
            settings_path: data_dir.map(|d| PathBuf::from(d).join("settings.json")),
            started: epoch_now(),
            names: tokio::sync::Mutex::new(None),
            asked: std::sync::Mutex::default(),
        }
    }

    /// Music's albums with the tags naming reads, from the catalogue, kept for `NAMES_SECS`.
    async fn music_names(&self) -> Result<Arc<Vec<crate::naming::Album>>> {
        let mut kept = self.names.lock().await;
        if let Some((at, albums)) = kept.as_ref().filter(|(at, _)| epoch_now() - at < NAMES_SECS) {
            let _ = at;
            return Ok(albums.clone());
        }
        let c = self.lib.catalogue();
        let albums = tokio::task::spawn_blocking(move || -> Result<Vec<crate::naming::Album>> {
            use crate::naming::{Album, Tags, Track};
            let mut albums: Vec<Album> = Vec::new();
            for (rel, tags) in c.folder_tags(0, &crate::naming::TAGS)? {
                let (folder, file) = rel.rsplit_once('/').unwrap_or(("", &rel));
                if albums.last().is_none_or(|a| a.folder != folder) {
                    albums.push(Album { folder: folder.to_string(), tracks: Vec::new() });
                }
                if let Some(a) = albums.last_mut() {
                    a.tracks.push(Track { file: file.to_string(), tags: Tags(tags) });
                }
            }
            albums.retain(|a| !a.folder.is_empty());
            Ok(albums)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let albums = Arc::new(albums);
        *kept = Some((epoch_now(), albums.clone()));
        Ok(albums)
    }

    /// Work out the template Music already follows, once, when none is saved: what Keep and Replace use until one is.
    /// Waits until the catalogue has read most of the library's tags.
    pub(super) async fn infer_naming(&self) {
        if self.lib.templates() != (None, None) {
            return;
        }
        let Ok(albums) = self.music_names().await else { return };
        let files: usize = albums.iter().map(|a| a.tracks.len()).sum();
        let titled = albums.iter().flat_map(|a| &a.tracks).filter(|t| t.tags.first("title").is_some()).count();
        if files == 0 || titled * 10 < files * 9 {
            return;
        }
        let a = albums.clone();
        if let Ok(found) = tokio::task::spawn_blocking(move || crate::naming::infer(&a)).await {
            tracing::info!(
                "naming: Music follows {} ({} of {} folders)",
                found.template,
                found.folders.matched,
                found.folders.total
            );
            self.lib.set_inferred_template(found.template);
        }
    }

    /// Measure a naming template against Music: `draft` as typed on the page, else the one in use.
    pub async fn naming(&self, draft: Option<&str>) -> Result<NamingView, ToggleError> {
        let draft = draft
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(|d| d.parse::<crate::naming::Template>().map_err(|e| ToggleError::BadSettings(e.to_string())))
            .transpose()?;
        let albums = self.music_names().await?;
        if draft.is_none() && self.lib.templates() == (None, None) {
            self.infer_naming().await;
        }
        let template = draft.unwrap_or_else(|| self.lib.template());
        let view = tokio::task::spawn_blocking(move || {
            let measured = crate::naming::check(&template, &albums);
            NamingView {
                template: template.to_string(),
                folders: measured.folders,
                files: measured.files,
                example: crate::naming::example(&template, &albums),
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(view)
    }

    fn kinds() -> Vec<KindView> {
        source::KINDS.iter().map(|k| KindView { id: k.id, label: k.label, fields: (k.fields)() }).collect()
    }

    /// Status, choices and a fresh look at where its libraries lie. `source` is `None` when it could not be
    /// connected.
    /// What was asked as `key` within [ASKED_FOR], else asked now and kept.
    fn asked_before(&self, key: &str) -> Option<Asked> {
        let mut asked = self.asked.lock().unwrap_or_else(|p| p.into_inner());
        asked.retain(|_, (at, _)| at.elapsed() < ASKED_FOR);
        asked.get(key).map(|(_, a)| a.clone())
    }

    fn keep_asked(&self, key: String, a: Asked) {
        self.asked.lock().unwrap_or_else(|p| p.into_inner()).insert(key, (std::time::Instant::now(), a));
    }

    /// A server as the Settings page shows it: asked of it at most every [ASKED_FOR] for the same settings and folders.
    async fn view_of(&self, cfg: &SourceConfig, source: Option<&dyn Source>, error: Option<String>) -> SourceView {
        let key = format!("source {} {:?} {:?} {}", serde_json::to_string(cfg).unwrap_or_default(), error, self.lib.place_folders(), source.is_some());
        if let Some(Asked::Source(v)) = self.asked_before(&key) {
            return v;
        }
        let v = self.ask_view_of(cfg, source, error).await;
        self.keep_asked(key, Asked::Source(v.clone()));
        v
    }

    async fn ask_view_of(&self, cfg: &SourceConfig, source: Option<&dyn Source>, error: Option<String>) -> SourceView {
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
            policies: cfg
                .libraries
                .music
                .iter()
                .chain(&cfg.libraries.incoming)
                .map(|l| (l.clone(), EffectivePolicy { format: cfg.libraries.format_of(l), colour: cfg.libraries.colour_of(l) }))
                .collect(),
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
                let files = self.files.clone();
                view.server_libraries = tokio::task::spawn_blocking(move || {
                    let mut libs = p.libraries;
                    for l in &mut libs {
                        l.read_only = !l.folders.is_empty() && !l.folders.iter().all(|f| files.can_write(std::path::Path::new(f)));
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
        let key = format!("lidarr {:?} {}", lidarr_cfg.as_ref().map(|c| (&c.url, &c.api_key)), lidarr_client.is_some());
        let lidarr = match self.asked_before(&key) {
            Some(Asked::Lidarr(v)) => v,
            _ => {
                let v = IncomingService::lidarr_view(lidarr_cfg.as_ref(), lidarr_client).await;
                self.keep_asked(key, Asked::Lidarr(v.clone()));
                v
            }
        };
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
            naming: self.lib.templates().0.map(|t| t.to_string()),
            palette: &crate::settings::PALETTE,
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
    /// `naming`: `None` keeps the saved template; a blank one goes back to the one the library follows.
    pub async fn save(
        &self,
        drafts: Vec<SourceConfig>,
        tag_rules: Option<crate::catalogue::Rules>,
        lidarr: Option<settings::LidarrConfig>,
        trash_days: Option<u32>,
        naming: Option<String>,
    ) -> Result<SettingsView, ToggleError> {
        let Some(path) = self.settings_path.clone() else { return Err(ToggleError::SettingsReadOnly) };
        let _one_at_a_time = self.lib.changing.lock().await;
        let old_lidarr = self.incoming.lidarr_state().0;
        let lidarr = match lidarr {
            Some(draft) => settings::merge_lidarr(old_lidarr.as_ref(), draft)?,
            None => old_lidarr,
        };
        let lidarr = lidarr_pair(lidarr);
        if let Some(cfg) = lidarr.0.as_ref().filter(|c| c.enabled) {
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
        let naming = match naming.as_deref().map(str::trim) {
            None => self.lib.templates().0,
            Some("") => None,
            Some(t) => Some(t.parse::<crate::naming::Template>().map_err(|e| ToggleError::BadSettings(format!("Naming: {e}")))?),
        };
        let saved: Vec<SourceConfig> = self.lib.live().iter().map(|a| a.cfg.clone()).collect();
        let merged = settings::merge(&saved, drafts)?;
        let mut next = Vec::with_capacity(merged.len());
        for cfg in merged.iter().cloned() {
            if cfg.kind == crate::source::cratarr::KIND.id {
                chosen_folders_ok(&cfg)?;
            }
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
        let to_save = settings::Saved {
            sources: merged,
            music_folder,
            tag_rules: tag_rules.clone(),
            lidarr: lidarr.0.clone(),
            trash_days,
            naming: naming.as_ref().map(|t| t.to_string()),
        };
        tokio::task::spawn_blocking(move || settings::save(&path, &to_save))
            .await
            .map_err(|e| anyhow::anyhow!("saving settings: {e}"))??;
        self.lib.set_tag_rules(tag_rules);
        self.lib.set_saved_template(naming);
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

/// Cratarr's chosen folders (decision 0044): each a plain absolute path inside its mounts (one gone since may stay, as
/// long as it lies there; its row says it is not found).
fn chosen_folders_ok(cfg: &SourceConfig) -> Result<(), ToggleError> {
    let Some(v) = cfg.values.get("folders") else { return Ok(()) };
    let list = v.as_array().ok_or_else(|| ToggleError::BadSettings("Folders: not a list of folders".into()))?;
    let roots = crate::folders::roots();
    for f in list {
        let p = f.as_str().ok_or_else(|| ToggleError::BadSettings("Folders: not a list of folders".into()))?;
        if !folder_in(p, &roots) {
            return Err(ToggleError::BadSettings(format!("Cratarr cannot see {p}.")));
        }
    }
    Ok(())
}

/// A plain absolute path at or under one of `roots`.
fn folder_in(p: &str, roots: &[String]) -> bool {
    let plain = p.starts_with('/') && !p.split('/').any(|s| s == ".." || s == ".");
    plain && roots.iter().map(|r| r.trim_end_matches('/')).any(|r| p == r || p.starts_with(&format!("{r}/")))
}

#[cfg(test)]
mod chosen_folders_tests {
    use super::folder_in;

    #[test]
    fn a_chosen_folder_lies_inside_the_mounts() {
        let roots = ["/media".to_string(), "/music/".to_string()];
        assert!(folder_in("/media/Music", &roots));
        assert!(folder_in("/media", &roots));
        assert!(folder_in("/music/A", &roots));
        assert!(!folder_in("/mediax/A", &roots), "a sibling with the same start");
        assert!(!folder_in("/media/../etc", &roots));
        assert!(!folder_in("media/Music", &roots), "relative");
        assert!(!folder_in("/data", &roots));
    }
}
