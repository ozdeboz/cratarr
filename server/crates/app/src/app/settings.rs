//! Settings (decision 0011): the page's view of everything configurable, checking a draft, and saving: the servers go
//! to the library, Lidarr and the trash days to Incoming.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::incoming::{IncomingService, LidarrView, lidarr_pair};
use super::library::{Active, Library, derive_folders, epoch_now, label_of};
use super::playlists::PlaylistService;
use super::{ToggleError, VERSION};
use crate::placement;
use crate::ports::Files;
use crate::settings::{self, ChangeError, FETCHERS, LibraryKind, NAME_MOST, OWN_MUSIC, OWN_MUSIC_OFF, Origin, SourceConfig, valid_colour};
use crate::source::{self, Choice, Field, FieldKind, Source, Status, Values};

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
    /// Its libraries in use, in order, as saved (0101 point 1).
    pub libraries: settings::Libraries,
    /// Its music libraries and where Cratarr sees their files (decision 0009).
    pub server_libraries: Vec<placement::PlacedLibrary>,
    /// Each used library's quality limit and colour as they apply (its own, else its use's and the palette's;
    /// decision 0034).
    pub policies: BTreeMap<String, EffectivePolicy>,
    /// Why the source could not be connected or its choices not filled, if so.
    pub error: Option<String>,
}

/// A used library's settings as they apply (decision 0034).
#[derive(Debug, Clone, Serialize)]
pub struct EffectivePolicy {
    pub quality: settings::Quality,
    /// Its dot (every library has one, 0101 point 1).
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
        let (values, secrets_set) = redact(cfg);
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
                .0
                .iter()
                .map(|l| (l.id.clone(), EffectivePolicy { quality: cfg.libraries.quality_of(&l.id), colour: cfg.libraries.colour_of(&l.id) }))
                .collect(),
            error,
        };
        let Some(s) = source else { return view };
        let places = self.lib.place_folders();
        let (status, choices, placed) = tokio::join!(s.status(), s.options(), placement::place(s, &places, &cfg.libraries));
        view.status = status.unwrap_or_default();
        match choices {
            Ok(c) => view.choices = c,
            Err(e) => {
                view.error.get_or_insert(format!("{e:#}"));
            }
        }
        match placed {
            Ok(p) => {
                let (files, own) = (self.files.clone(), cfg.own_music());
                view.server_libraries = tokio::task::spawn_blocking(move || {
                    let mut libs = p.libraries;
                    // Own music is no setting of a library: its folder's (0101 point 9).
                    libs.retain(|l| own.is_none() || std::path::Path::new(&l.id).parent() != own.as_deref());
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
        let cfg = merge(&saved, vec![draft])?.remove(0);
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

    /// The tag rules alone saved (the Catalogue sheet's genre aliases, 0.15): every source as it is.
    pub async fn save_tag_rules(&self, rules: crate::catalogue::Rules) -> Result<SettingsView, ToggleError> {
        let sources: Vec<SourceConfig> = self.lib.live().iter().map(|a| a.cfg.clone()).collect();
        self.save(sources, Some(rules), None, None, None).await
    }

    /// The tag rules as kept.
    pub fn tag_rules(&self) -> crate::catalogue::Rules {
        self.lib.tag_rules()
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
        let merged = merge(&saved, drafts)?;
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
                // Its main libraries checked when they or the source changed, not on every save: a main library's folder
                // out of reach for a while (unmounted) must not stop the trash days or the naming from being saved
                // (2026-11-02).
                let as_saved = saved.iter().any(|s| s.id == a.cfg.id && s.kind == a.cfg.kind && s.libraries == a.cfg.libraries && s.values == a.cfg.values);
                for id in a.main().iter().filter(|_| !as_saved) {
                    match a.placement.libraries.iter().find(|l| &l.id == id) {
                        None => return Err(bad("a main library is no longer on the server".into())),
                        Some(l) if l.folders.is_empty() => {
                            return Err(ToggleError::BadSettings(format!("Cratarr can't see the files of {}", l.name)));
                        }
                        Some(_) => {}
                    }
                }
            }
            next.push(Arc::new(a));
        }
        // Several main libraries in folders of their own are fine (0101 point 1: equal; playlists live in the database).
        let music_folder = derive_folders(&next, self.lib.music().as_deref()).music;
        let to_save = settings::Saved {
            sources: merged,
            music_folder,
            tag_rules: tag_rules.clone(),
            lidarr: lidarr.0.clone(),
            lidarr_removed: settings::lidarr_removed(lidarr.0.as_ref(), &|k: &str| std::env::var(k).ok()),
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

/// Sources' settings checked against their plugins (SPLIT-PLAN P2: here, above the stores, where the plugins are).
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Check a list from the page, keep only fields the plugin declares (with the right shape), and fill blank secrets
/// from the saved source with the same id, unless a field the secret is bound to has changed.
pub fn merge(saved: &[SourceConfig], draft: Vec<SourceConfig>) -> Result<Vec<SourceConfig>, ChangeError> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(draft.len());
    for mut d in draft {
        if !valid_id(&d.id) {
            return Err(ChangeError::Invalid(format!("{:?} is not a usable source id (a–z, 0–9 and -, at most 32)", d.id)));
        }
        if !seen.insert(d.id.clone()) {
            return Err(ChangeError::Invalid(format!("two sources are called {:?}", d.id)));
        }
        let Some(kind) = source::kind(&d.kind) else {
            return Err(ChangeError::Invalid(format!("{}: unknown kind of source {:?}", d.id, d.kind)));
        };
        let fields = (kind.fields)();
        // Cratarr's own folders (decision 0044) are its values though no field of its form shows them (they are chosen in
        // the folder browser): kept, or every save dropped them and the source then had no library (2026-11-02, found
        // on the development instance: "a Music library is no longer on the server" for any change).
        let own_folders = d.kind == crate::source::CRATARR;
        d.values.retain(|k, v| {
            (own_folders && k == "folders" && v.is_array())
                || (own_folders && k == OWN_MUSIC && v.is_string())
                || (own_folders && k == OWN_MUSIC_OFF && v.is_array())
                || fields.iter().any(|f| {
                    f.key == k
                        && match f.kind {
                            FieldKind::MultiChoice => v.is_array() || v.is_string(),
                            _ => v.is_string(),
                        }
                })
        });
        // The own-music folder holds a subfolder per account: never inside a library's folder, nor around one (two libraries
        // over the same files).
        if let Some(own) = d.own_music() {
            if !own.is_absolute() {
                return Err(ChangeError::Invalid(format!("{}: the own-music folder must be a whole path", d.id)));
            }
            if source::list(&d.values, "folders").iter().any(|c| own.starts_with(c) || Path::new(c).starts_with(&own)) {
                return Err(ChangeError::Invalid(format!("{}: the own-music folder can't lie in a library's folder or hold one", d.id)));
            }
        }
        let prev = saved.iter().find(|s| s.id == d.id && s.kind == d.kind);
        // The libraries in use, in order (0101 point 1): each once, one kind each; own music is no setting (its folder's,
        // 0101 point 9).
        d.libraries.0.retain(|l| !l.id.trim().is_empty());
        let mut listed = std::collections::HashSet::new();
        for l in &d.libraries.0 {
            if !listed.insert(l.id.as_str()) {
                return Err(ChangeError::Invalid(format!("{}: {} is listed twice: a library has one kind", d.id, l.id)));
            }
            if l.kind == LibraryKind::Own {
                return Err(ChangeError::Invalid(format!("{}: {} is own music, which comes from the own-music folder, not the list", d.id, l.id)));
            }
        }
        let mains: Vec<String> = d.libraries.of(LibraryKind::Main).map(str::to_string).collect();
        let mut fetched_by: Vec<String> = Vec::new();
        for l in d.libraries.0.iter_mut() {
            let lib = l.id.clone();
            if let Some(c) = &l.colour {
                if !valid_colour(c) {
                    return Err(ChangeError::Invalid(format!("{}: {lib}'s colour {c:?} is not #rrggbb", d.id)));
                }
                l.colour = Some(c.to_ascii_lowercase());
            }
            // A name kept trimmed; blank is none (the folder's name again).
            l.name = l.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
            if let Some(n) = &l.name
                && (n.chars().count() > NAME_MOST || n.chars().any(char::is_control))
            {
                return Err(ChangeError::Invalid(format!("{}: {lib}'s name is longer than {NAME_MOST} characters or holds control characters", d.id)));
            }
            // Its fetcher (decision 0099's amendment: Lidarr and Soulseek siblings): one of them, an Incoming library's, one
            // library each.
            l.fetcher = l.fetcher.as_deref().map(str::trim).filter(|f| !f.is_empty()).map(str::to_string);
            if let Some(f) = l.fetcher.as_deref() {
                if !FETCHERS.contains(&f) {
                    return Err(ChangeError::Invalid(format!("{}: {lib}'s fetcher {f:?} is not one of {}", d.id, FETCHERS.join(", "))));
                }
                if l.kind != LibraryKind::Incoming {
                    return Err(ChangeError::Invalid(format!("{}: {lib} is not an Incoming library: only an Incoming library is fetched into", d.id)));
                }
                if fetched_by.iter().any(|x| x == f) {
                    return Err(ChangeError::Invalid(format!("{}: {f} fetches into one Incoming library only", d.id)));
                }
                fetched_by.push(f.to_string());
            }
            // Where its records are kept by default (0101 point 2): one of the server's main libraries, an Incoming
            // library's only.
            l.keep_to = l.keep_to.as_deref().map(str::trim).filter(|k| !k.is_empty()).map(str::to_string);
            if let Some(k) = l.keep_to.as_deref() {
                if l.kind != LibraryKind::Incoming {
                    return Err(ChangeError::Invalid(format!("{}: {lib} is not an Incoming library: only an Incoming library's records are kept", d.id)));
                }
                if !mains.iter().any(|m| m == k) {
                    return Err(ChangeError::Invalid(format!("{}: {lib} keeps into {k}, which is not one of its main libraries", d.id)));
                }
            }
        }
        for secret in fields.iter().filter(|f| f.kind == FieldKind::Secret) {
            if source::text(&d.values, secret.key).is_some() {
                continue;
            }
            d.values.remove(secret.key);
            let Some(prev) = prev else { continue };
            let Some(old) = source::text(&prev.values, secret.key) else { continue };
            if let Some(changed) = secret.bound_to.iter().find(|b| source::text(&prev.values, b) != source::text(&d.values, b)) {
                let field = fields.iter().find(|f| f.key == *changed).map_or(*changed, |f| f.label);
                return Err(ChangeError::SecretNeeded {
                    name: d.id.clone(),
                    field: field.to_lowercase(),
                    secret: secret.label.to_lowercase(),
                });
            }
            d.values.insert(secret.key.into(), old.into());
        }
        out.push(d);
    }
    Ok(out)
}

/// The values the page may see: secrets left out, plus which secrets are set.
pub fn redact(cfg: &SourceConfig) -> (Values, Vec<String>) {
    let secrets: Vec<&str> = source::kind(&cfg.kind)
        .map(|k| (k.fields)().into_iter().filter(|f| f.kind == FieldKind::Secret).map(|f| f.key).collect())
        .unwrap_or_default();
    let mut values = cfg.values.clone();
    let mut set = Vec::new();
    for s in secrets {
        if source::text(&values, s).is_some() {
            set.push(s.to_string());
        }
        values.remove(s);
    }
    (values, set)
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

#[cfg(test)]
mod merge_tests {
    //! `merge` and `redact`'s tests, moved up with them from `settings` (SPLIT-PLAN P2).
    use super::*;
    use crate::settings::{Libraries, LibrarySetting};

    fn list(entries: &[(&str, LibraryKind)]) -> Libraries {
        Libraries(entries.iter().map(|(id, k)| LibrarySetting::new(*id, *k)).collect())
    }

    #[test]
    fn cratarrs_own_folders_are_kept_through_a_save() {
        let own = |folders: serde_json::Value| SourceConfig {
            id: "cratarr".into(),
            kind: crate::source::CRATARR.into(),
            enabled: true,
            libraries: list(&[("/music", LibraryKind::Main), ("/incoming", LibraryKind::Incoming)]),
            values: [("folders".to_string(), folders)].into_iter().collect(),
        };
        let out = merge(&[], vec![own(serde_json::json!(["/music", "/incoming"]))]).unwrap();
        assert_eq!(out[0].values.get("folders"), Some(&serde_json::json!(["/music", "/incoming"])));
        // Only as a list, and only Cratarr's own: another kind's undeclared values still go.
        let out = merge(&[], vec![own(serde_json::json!("/music"))]).unwrap();
        assert!(out[0].values.get("folders").is_none());
    }

    #[test]
    fn the_own_music_folder_is_kept_and_checked() {
        let own = |folder: &str| SourceConfig {
            id: "cratarr".into(),
            kind: crate::source::CRATARR.into(),
            enabled: true,
            libraries: Libraries::default(),
            values: [("folders".to_string(), serde_json::json!(["/music", "/media/x"])), (OWN_MUSIC.to_string(), serde_json::json!(folder))].into_iter().collect(),
        };
        let out = merge(&[], vec![own("/own")]).unwrap();
        assert_eq!(out[0].own_music(), Some(std::path::PathBuf::from("/own")));
        assert_eq!(merge(&[], vec![own(" ")]).unwrap()[0].own_music(), None, "blank: none");
        assert!(merge(&[], vec![own("own")]).is_err(), "a whole path");
        assert!(merge(&[], vec![own("/music/own")]).is_err(), "not inside a library");
        assert!(merge(&[], vec![own("/media")]).is_err(), "not around one");
        let mut other = jf("jf", "http://a", Some("k"));
        other.values.insert(OWN_MUSIC.into(), "/own".into());
        assert_eq!(merge(&[], vec![other]).unwrap()[0].own_music(), None, "Cratarr's own source's only");
        // 0.17: switched off for some accounts, kept as saved.
        let mut off = own("/own");
        off.values.insert(OWN_MUSIC_OFF.into(), serde_json::json!(["bo"]));
        let kept = merge(&[], vec![off]).unwrap();
        assert!(kept[0].own_music_on("ann"));
        assert!(!kept[0].own_music_on("bo"));
    }

    #[test]
    fn library_settings_are_checked() {
        let with = |entries: Vec<LibrarySetting>| {
            let mut s = jf("jf", "http://a", Some("k"));
            s.libraries = Libraries(entries);
            merge(&[], vec![s])
        };
        let main = |id: &str| LibrarySetting::new(id, LibraryKind::Main);
        let incoming = |id: &str| LibrarySetting::new(id, LibraryKind::Incoming);
        // A colour as #rrggbb, kept in lower case.
        assert!(with(vec![LibrarySetting { colour: Some("red".into()), ..main("m") }]).is_err());
        let out = with(vec![LibrarySetting { colour: Some("#AABBCC".into()), ..main("m") }]).unwrap();
        assert_eq!(out[0].libraries.0[0].colour.as_deref(), Some("#aabbcc"));
        // A name of its own, kept trimmed; blank is none again.
        let out = with(vec![LibrarySetting { name: Some("  Vinyl rips ".into()), ..incoming("i") }, LibrarySetting { name: Some("   ".into()), ..incoming("j") }]).unwrap();
        assert_eq!(out[0].libraries.name_of("i").as_deref(), Some("Vinyl rips"));
        assert_eq!(out[0].libraries.name_of("j"), None);
        assert!(with(vec![LibrarySetting { name: Some("x".repeat(81)), ..main("m") }]).is_err());
        // A fetcher: only one of the siblings, only an Incoming library's, one library each.
        let out = with(vec![main("m"), LibrarySetting { fetcher: Some("soulseek".into()), ..incoming("i") }, incoming("k")]).unwrap();
        assert_eq!(out[0].libraries.fetched_by("soulseek"), Some("i"));
        assert!(with(vec![LibrarySetting { fetcher: Some("soulseek".into()), ..incoming("i") }, LibrarySetting { fetcher: Some("soulseek".into()), ..incoming("k") }]).is_err());
        assert!(with(vec![LibrarySetting { fetcher: Some("usenet".into()), ..incoming("i") }]).is_err());
        assert!(with(vec![LibrarySetting { fetcher: Some("lidarr".into()), ..main("m") }]).is_err());
        // Keep's default: one of the server's main libraries, an Incoming library's only.
        let out = with(vec![main("m"), main("n"), LibrarySetting { keep_to: Some("n".into()), ..incoming("i") }]).unwrap();
        assert_eq!(out[0].libraries.get("i").and_then(|l| l.keep_to.as_deref()), Some("n"));
        assert!(with(vec![main("m"), LibrarySetting { keep_to: Some("elsewhere".into()), ..incoming("i") }]).is_err());
        assert!(with(vec![main("m"), LibrarySetting { keep_to: Some("m".into()), ..main("n") }]).is_err());
    }

    fn jf(id: &str, url: &str, key: Option<&str>) -> SourceConfig {
        let mut values = Values::new();
        values.insert("url".into(), url.into());
        values.insert("player".into(), "Finamp".into());
        if let Some(k) = key {
            values.insert("api_key".into(), k.into());
        }
        SourceConfig { id: id.into(), kind: "jellyfin".into(), enabled: true, libraries: Libraries::default(), values }
    }

    #[test]
    fn a_blank_secret_keeps_the_saved_one_while_the_address_stays() {
        let saved = vec![jf("jellyfin", "http://jf:8096", Some("secret"))];
        let out = merge(&saved, vec![jf("jellyfin", "http://jf:8096", None)]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), Some("secret"));
        let out = merge(&saved, vec![jf("jellyfin", "http://jf:8096", Some("  "))]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), Some("secret"));
    }

    #[test]
    fn a_new_address_needs_the_secret_again() {
        let saved = vec![jf("jellyfin", "http://jf:8096", Some("secret"))];
        let e = merge(&saved, vec![jf("jellyfin", "http://evil:8096", None)]).unwrap_err();
        assert!(matches!(e, ChangeError::SecretNeeded { .. }), "{e}");
        assert!(e.to_string().contains("address"), "{e}");
        let out = merge(&saved, vec![jf("jellyfin", "http://new:8096", Some("k2"))]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), Some("k2"));
    }

    #[test]
    fn a_secret_never_moves_to_another_source() {
        let saved = vec![jf("jellyfin", "http://jf:8096", Some("secret"))];
        let out = merge(&saved, vec![jf("other", "http://jf:8096", None)]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), None);
    }

    #[test]
    fn ids_and_kinds_are_checked_and_unknown_fields_dropped() {
        assert!(merge(&[], vec![jf("Bad Id", "http://x", Some("k"))]).is_err());
        assert!(merge(&[], vec![jf("a", "http://x", Some("k")), jf("a", "http://y", Some("k"))]).is_err());
        let mut odd = jf("a", "http://x", Some("k"));
        odd.kind = "winamp".into();
        assert!(merge(&[], vec![odd]).is_err());
        let mut extra = jf("a", "http://x", Some("k"));
        extra.values.insert("smuggled".into(), "x".into());
        extra.values.insert("player".into(), serde_json::json!(["not", "a", "string"]));
        let out = merge(&[], vec![extra]).unwrap();
        assert!(!out[0].values.contains_key("smuggled"));
        assert!(!out[0].values.contains_key("player"), "wrong shape is dropped");
    }

    #[test]
    fn a_library_is_listed_once_with_one_kind() {
        let mut s = jf("a", "http://x", Some("k"));
        s.libraries = list(&[("l1", LibraryKind::Main), (" ", LibraryKind::Main), ("l2", LibraryKind::Incoming)]);
        let out = merge(&[], vec![s.clone()]).unwrap();
        assert_eq!(out[0].libraries, list(&[("l1", LibraryKind::Main), ("l2", LibraryKind::Incoming)]), "a blank one goes");
        s.libraries.0.push(LibrarySetting::new("l1", LibraryKind::Incoming));
        assert!(merge(&[], vec![s.clone()]).is_err(), "one library, one kind");
        s.libraries = list(&[("/own/ann", LibraryKind::Own)]);
        assert!(merge(&[], vec![s]).is_err(), "own music comes from its folder");
    }

    #[test]
    fn the_page_never_sees_secrets() {
        let (values, set) = redact(&jf("jellyfin", "http://jf:8096", Some("secret")));
        assert!(!values.contains_key("api_key"));
        assert_eq!(set, vec!["api_key"]);
        let (_, set) = redact(&jf("jellyfin", "http://jf:8096", None));
        assert!(set.is_empty());
    }
}
