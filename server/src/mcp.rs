//! Cratarr as an MCP server (0.13, the owner via the ThinkPad session: "Yes, 0.13 MCP server built inside"): a thin
//! layer over the server's own reads, for an assistant such as Claude Code. Streamable HTTP at `/mcp` (the MCP Rust
//! SDK, `rmcp`), behind the same access check as the API; an API key only (no sign-in cookie, no open or loopback way
//! in), a reading key enough: every tool reads, none changes anything. Few tools, each with an action or filters, so a
//! client's list stays short (the grouped tools of Jellyfin's MCP servers).
//!
//! The tools: `search` (the library, as the page's search), `albums` (records by type, classical, set by hand, with
//! why each is what it is), `album` (one record: its facts and tracks), `classical` (composers, a composer's works, a
//! work), `health` (the library's findings: missing tracks, unreadable files…), `crates` (the account's crates, or one
//! crate's tracks).

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::App;
use crate::serve::who::{Scope, Who, acting_as, current};

/// Records a list gives at most at once.
const MOST: usize = 200;

/// `/mcp`, for keys only (`keyed`).
pub fn router(app: Arc<App>) -> Router {
    // Any Host: a key is needed (`keyed`), which a page another site opens cannot send, so the SDK's guard against DNS
    // rebinding (loopback names only) would only refuse the NAS's own names.
    let config = StreamableHttpServerConfig::default().disable_allowed_hosts();
    let service = StreamableHttpService::new(move || Ok(Cratarr::new(app.clone())), LocalSessionManager::default().into(), config);
    Router::new().nest_service("/mcp", service).layer(axum::middleware::from_fn(keyed))
}

/// Only a request with an API key (a reading or a controlling one), as the access check found it; its account rides
/// along to the tools (the request's extensions), which run where the access check's task-local does not reach.
async fn keyed(mut request: Request, next: Next) -> Response {
    match current() {
        Some(who) if who.scope != Scope::Full => {
            request.extensions_mut().insert(who);
            next.run(request).await
        }
        _ => (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": "Cratarr's MCP server takes an API key (Settings → Authentication)." })))
            .into_response(),
    }
}

#[derive(Clone)]
pub struct Cratarr {
    app: Arc<App>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// What to look for: words of a title, an artist, an album, a genre or a work.
    pub query: String,
    /// Only these kinds (artists, albums, tracks, works, composers…); all where none.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// At most this many of each kind (default 20).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AlbumsArgs {
    /// Only records of this release type: album, ep, single, incomplete, other.
    #[serde(default)]
    pub r#type: Option<String>,
    /// Only classical records (true) or only others (false).
    #[serde(default)]
    pub classical: Option<bool>,
    /// Only records with something set by hand.
    #[serde(default)]
    pub set_by_hand: Option<bool>,
    /// Only records whose album artist contains these words.
    #[serde(default)]
    pub artist: Option<String>,
    /// Skip this many (paging).
    #[serde(default)]
    pub offset: Option<usize>,
    /// At most this many (default and most 200).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AlbumArgs {
    /// The record's id, as `albums` and `search` give it.
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClassicalArgs {
    /// `composers` (every composer of the classical records), `composer` (one composer's records, by `key`), `works`
    /// (one composer's works, by `key`), or `work` (one work's recordings and movements, by `mbid`).
    pub action: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub mbid: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HealthArgs {
    /// Only findings of this kind (as the findings name it: incomplete, broken…); all where none.
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CratesArgs {
    /// One crate's tracks, by its id; the account's crates where none.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub offset: Option<usize>,
    /// At most this many tracks (default and most 200).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// A tool's answer: its JSON as text.
fn answer(v: &impl serde::Serialize) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string(v).map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn failed(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

fn asked(what: &str) -> McpError {
    McpError::invalid_params(what.to_string(), None)
}

/// The account the request's key is for (`keyed`).
fn who(ctx: &RequestContext<RoleServer>) -> Result<Who, McpError> {
    ctx.extensions
        .get::<axum::http::request::Parts>()
        .and_then(|p| p.extensions.get::<Who>())
        .cloned()
        .ok_or_else(|| McpError::invalid_request("No account: Cratarr's MCP server takes an API key.", None))
}

/// A record as a list gives it: what it is, and why.
fn row(a: &crate::source::Album) -> Value {
    let r = &a.release;
    json!({
        "id": a.id, "title": a.title, "artist": a.artist, "year": a.year, "tracks": a.tracks, "duration_secs": a.duration_secs,
        "type": r.kind, "type_from": r.type_from, "type_why": r.type_why, "secondary": r.secondary, "set_by_hand": r.set,
        "classical": r.classical, "classical_why": r.classical_why, "composers": r.composers,
    })
}

#[tool_router]
impl Cratarr {
    pub fn new(app: Arc<App>) -> Self {
        Self { app }
    }

    /// Cratarr's own records, classical said of each (`classical::tell`).
    async fn records(&self) -> Vec<crate::source::Album> {
        let own = crate::source::cratarr::KIND.id;
        let listed = self.app.collection.list(Some(own)).await;
        let mut albums: Vec<crate::source::Album> = listed.sources.into_iter().flat_map(|s| s.albums).collect();
        crate::app::classical::tell(own, &mut albums).await;
        albums
    }

    #[tool(description = "Search Cratarr's library as its page does: artists, albums, tracks, works and composers for some words.")]
    async fn search(&self, Parameters(a): Parameters<SearchArgs>, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let who = who(&ctx)?;
        let limit = a.limit.unwrap_or(20).clamp(1, 100);
        let found = acting_as(who, crate::app::search::search(&self.app, &a.query, &a.kinds, limit, 0)).await.map_err(failed)?;
        answer(&found)
    }

    #[tool(description = "List the library's records with their release type (and where it came from, and which rule decided it), \
        secondary types, what is set by hand, and whether each is classical and why. Filter by type, classical, set by hand or artist.")]
    async fn albums(&self, Parameters(a): Parameters<AlbumsArgs>) -> Result<CallToolResult, McpError> {
        let words = a.artist.as_deref().map(str::to_lowercase);
        let picked: Vec<Value> = self
            .records()
            .await
            .iter()
            .filter(|r| a.r#type.as_deref().is_none_or(|t| r.release.kind.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(t))))
            .filter(|r| a.classical.is_none_or(|c| r.release.classical.unwrap_or(false) == c))
            .filter(|r| a.set_by_hand.is_none_or(|s| r.release.set.is_empty() != s))
            .filter(|r| words.as_deref().is_none_or(|w| r.artist.as_deref().is_some_and(|x| x.to_lowercase().contains(w))))
            .map(row)
            .collect();
        let (offset, limit) = (a.offset.unwrap_or(0), a.limit.unwrap_or(MOST).clamp(1, MOST));
        let page: Vec<&Value> = picked.iter().skip(offset).take(limit).collect();
        answer(&json!({ "total": picked.len(), "offset": offset, "records": page }))
    }

    #[tool(description = "One record: its facts (type and why, secondary types, set by hand, classical and why, composers) and its tracks.")]
    async fn album(&self, Parameters(a): Parameters<AlbumArgs>) -> Result<CallToolResult, McpError> {
        let records = self.records().await;
        let record = records.iter().find(|r| r.id == a.id).ok_or_else(|| asked("No such record in the library."))?;
        let tracks = self.app.playing.album_view(crate::source::cratarr::KIND.id, &a.id).await.map_err(failed)?;
        answer(&json!({ "record": row(record), "tracks": tracks }))
    }

    #[tool(description = "Classical: `composers` (every composer of the classical records), `composer` (one composer's records, by key), \
        `works` (one composer's works, by key), or `work` (a work's recordings and movements, by its MusicBrainz id).")]
    async fn classical(&self, Parameters(a): Parameters<ClassicalArgs>) -> Result<CallToolResult, McpError> {
        let serve = crate::serve::instance().ok_or_else(|| failed("Cratarr's own library is not open."))?;
        let ix = tokio::task::spawn_blocking(move || crate::app::classical::index(&serve)).await.map_err(failed)?.map_err(failed)?;
        let key = || a.key.as_deref().ok_or_else(|| asked("Give the composer's key (from `composers`)."));
        match a.action.as_str() {
            "composers" => answer(&ix.composers),
            "composer" => {
                let c = ix.composer(key()?).ok_or_else(|| asked("No such composer."))?;
                answer(&json!({ "key": c.key, "name": c.name, "records": c.albums }))
            }
            "works" => answer(&ix.composer_works(key()?).ok_or_else(|| asked("No such composer."))?),
            "work" => {
                let mbid = a.mbid.as_deref().ok_or_else(|| asked("Give the work's MusicBrainz id."))?;
                answer(&ix.work(mbid).ok_or_else(|| asked("No such work in the library."))?)
            }
            _ => Err(asked("The action is composers, composer, works or work.")),
        }
    }

    #[tool(description = "The library's health findings, worst first: albums missing tracks (and which), unreadable or damaged files, \
        and the rest Health shows. Optionally one kind.")]
    async fn health(&self, Parameters(a): Parameters<HealthArgs>) -> Result<CallToolResult, McpError> {
        let report = self.app.health_report().await;
        let mut v = serde_json::to_value(&report).map_err(failed)?;
        if let (Some(kind), Some(findings)) = (a.kind.as_deref(), v.get_mut("findings").and_then(Value::as_array_mut)) {
            findings.retain(|f| f["kind"].as_str().is_some_and(|k| k.eq_ignore_ascii_case(kind)));
        }
        answer(&v)
    }

    #[tool(description = "The account's crates (saved and built-in), or one crate's tracks by its id.")]
    async fn crates(&self, Parameters(a): Parameters<CratesArgs>, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let who = who(&ctx)?;
        let playlists = self.app.playlists.clone();
        match a.id {
            None => answer(&acting_as(who, async move { playlists.crates(None, &[]).await }).await.map_err(failed)?),
            Some(id) => {
                let (offset, limit) = (a.offset.unwrap_or(0), a.limit.unwrap_or(MOST).clamp(1, MOST));
                let page = acting_as(who, async move {
                    let def = playlists.crate_def(&id).await?;
                    playlists.crate_page(&def.conditions, None, offset, limit, &crate::app::CrateLook::default()).await
                })
                .await
                .map_err(failed)?;
                answer(&page)
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for Cratarr {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("cratarr", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Cratarr's music library, read only. `albums` lists records with their release type and why (type_from: who said \
                 it; type_why: the rule that decided it), what is set by hand, and whether each is classical and why; `album` gives \
                 one with its tracks; `search` finds by words; `classical` reads composers and works; `health` lists problems such \
                 as missing tracks; `crates` reads the account's crates."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    /// `/mcp` behind `keyed`, answering who it was asked as.
    async fn ask(who: Option<Who>) -> (StatusCode, String) {
        let app = Router::new()
            .route("/mcp", axum::routing::post(|r: Request| async move { format!("{:?}", r.extensions().get::<Who>().map(|w| w.user.clone())) }))
            .layer(axum::middleware::from_fn(keyed));
        let req = Request::post("/mcp").body(Body::empty()).unwrap();
        let r = match who {
            Some(w) => acting_as(w, app.oneshot(req)).await.unwrap(),
            None => app.oneshot(req).await.unwrap(),
        };
        let status = r.status();
        (status, String::from_utf8(axum::body::to_bytes(r.into_body(), 4096).await.unwrap().to_vec()).unwrap())
    }

    #[test]
    fn the_tools_are_listed_with_what_they_take() {
        let tools = Cratarr::tool_router().list_all();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        names.sort_unstable();
        assert_eq!(names, ["album", "albums", "classical", "crates", "health", "search"]);
        let albums = tools.iter().find(|t| t.name == "albums").unwrap();
        let schema = serde_json::to_value(&*albums.input_schema).unwrap();
        assert!(schema["properties"]["type"].is_object() && schema["properties"]["classical"].is_object(), "{schema}");
    }

    #[tokio::test]
    async fn only_a_key_asks_and_its_account_rides_along() {
        let as_ = |scope| Some(Who { user: "me".into(), admin: true, scope, home: true });
        assert_eq!(ask(as_(Scope::Read)).await, (StatusCode::OK, "Some(\"me\")".into()));
        assert_eq!(ask(as_(Scope::Control)).await.0, StatusCode::OK);
        assert_eq!(ask(as_(Scope::Full)).await.0, StatusCode::UNAUTHORIZED, "a sign-in (the page's cookie) is no key");
        assert_eq!(ask(None).await.0, StatusCode::UNAUTHORIZED, "no one: an open or loopback way in");
    }
}
