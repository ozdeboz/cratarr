//! What Discogs says of the library's albums and artists, and of Discover's release groups (0.9.8, the owner: Missing
//! genres lacked Discogs' genres and styles; labels and catalogue numbers, credits, artist profiles; Discover's genres),
//! kept a while (`UserData::discogs_facts`). An album is asked when its fixes or its page are read, and in the background
//! for the albums missing a genre or a year; a release group in a Discover look. An album's genres and styles go into
//! the library's words as Discogs' too (as the harvest's), so every place the words reach has them.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use super::discogs::{AlbumFacts, ArtistFacts, MbLabel};
use crate::catalogue::records::Ref;
use crate::userdata::words::RawWord;

/// Asked again after this long; where Discogs had nothing, sooner.
const AGAIN: i64 = 30 * 86_400;
const NONE_AGAIN: i64 = 7 * 86_400;
/// Asked again after this long where MusicBrainz's link could not be had (F47): what Discogs' search found stands in till then.
const LINK_AGAIN: i64 = 15 * 60;

fn due(kept: Option<&(i64, Option<String>)>, now: i64) -> bool {
    kept.is_none_or(|(at, facts)| now - at >= if facts.is_some() { AGAIN } else { NONE_AGAIN })
}

/// The time facts read at `now` are kept as read: now; where MusicBrainz's link could not be had (F47), as long before as makes them
/// due again after `LINK_AGAIN`.
fn kept_at(now: i64, found: bool, link_unread: bool) -> i64 {
    if link_unread { now - if found { AGAIN } else { NONE_AGAIN } + LINK_AGAIN } else { now }
}

fn parse<T: serde::de::DeserializeOwned>(kept: Option<(i64, Option<String>)>) -> Option<T> {
    kept.and_then(|(_, f)| f).and_then(|f| serde_json::from_str(&f).ok())
}

/// An album's subject, as its words are kept.
fn album_subject(album: &str) -> String {
    format!("album:{album}")
}

/// An album's facts as kept, not asked for. Blocking.
pub fn album_kept(served: &crate::serve::Serve, album: &str) -> Result<Option<AlbumFacts>> {
    Ok(parse(served.userdata.discogs_facts(&album_subject(album))?))
}

/// Every album's labels as kept from Discogs, by its id (0.11, quick crates: the crates' Label where an
/// album's files name none): what the rounds kept so far, never asked for here. Blocking.
pub fn kept_labels(served: &crate::serve::Serve) -> Result<HashMap<String, Vec<String>>> {
    let mut out = HashMap::new();
    for (subject, json) in served.userdata.discogs_facts_of("album:")? {
        let (Some(album), Ok(kept)) = (subject.strip_prefix("album:"), serde_json::from_str::<Labelled>(&json)) else { continue };
        let names = label_names(&kept.labels);
        if !names.is_empty() {
            out.insert(album.to_string(), names);
        }
    }
    Ok(out)
}

/// An album's facts as read for its labels alone (the crates' Label, F9's `label_named`).
#[derive(serde::Deserialize)]
struct Labelled {
    #[serde(default)]
    labels: Vec<super::discogs::Label>,
}

/// Where an album's labels at MusicBrainz are kept (F9): beside its Discogs facts, as the files' release names them.
const MUSICBRAINZ_LABELS: &str = "musicbrainz-labels:";

fn musicbrainz_subject(album: &str) -> String {
    format!("{MUSICBRAINZ_LABELS}{album}")
}

/// An album's labels as its files' MusicBrainz release names them (F9), as kept beside its facts; none where none were read.
/// Blocking.
pub fn musicbrainz_labels(served: &crate::serve::Serve, album: &str) -> Result<Vec<MbLabel>> {
    Ok(parse(served.userdata.discogs_facts(&musicbrainz_subject(album))?).unwrap_or_default())
}

/// `GET /api/albums/{source}/{album}/discogs`'s answer (F9): Discogs' facts as given (`album`), their `labels` as About shows them
/// (`labels::about`: Discogs' with their ids, then MusicBrainz's of the files' release, kept beside them, of a name Discogs names
/// none of); `{}` where neither says anything. Blocking.
pub fn answer(served: &crate::serve::Serve, album: &str, facts: Option<&AlbumFacts>) -> Result<serde_json::Value> {
    answer_with(served.catalogue(), facts, &musicbrainz_labels(served, album)?)
}

/// `answer`, MusicBrainz's labels given. Blocking.
fn answer_with(cat: &crate::catalogue::Catalogue, facts: Option<&AlbumFacts>, musicbrainz: &[MbLabel]) -> Result<serde_json::Value> {
    let labels = super::labels::about(cat, facts.map_or(&[][..], |f| f.labels.as_slice()), musicbrainz)?;
    let mut answer = match facts {
        Some(f) => serde_json::to_value(f)?,
        None if labels.is_empty() => return Ok(serde_json::json!({})),
        None => serde_json::json!({}),
    };
    if let Some(o) = answer.as_object_mut() {
        o.insert("labels".into(), serde_json::to_value(labels)?);
    }
    Ok(answer)
}

/// The name a label's reference was kept with (F9: who may make a label by it, `labels::open_label`): an album's label at Discogs by
/// its id (its facts), or at MusicBrainz by its MBID (its release's, kept beside them); none where nothing kept names it. Blocking.
pub fn label_named(served: &crate::serve::Serve, r: &Ref) -> Result<Option<(String, bool)>> {
    if r.kind != "label" {
        return Ok(None);
    }
    match r.source.as_str() {
        "discogs" => {
            let Ok(id) = r.value.parse::<u64>() else { return Ok(None) };
            for (_, json) in served.userdata.discogs_facts_of("album:")? {
                if let Ok(kept) = serde_json::from_str::<Labelled>(&json)
                    && let Some(l) = kept.labels.into_iter().find(|l| l.id == Some(id))
                {
                    return Ok(Some((l.name, l.namesake)));
                }
            }
        }
        "musicbrainz" => {
            for (_, json) in served.userdata.discogs_facts_of(MUSICBRAINZ_LABELS)? {
                if let Ok(kept) = serde_json::from_str::<Vec<MbLabel>>(&json)
                    && let Some(l) = kept.into_iter().find(|l| l.mbid == r.value)
                {
                    return Ok(Some((l.name, false)));
                }
            }
        }
        _ => {}
    }
    Ok(None)
}

/// Labels' names as the crates offer them: each once, Discogs' "Not On Label" (self-released, no label) none.
fn label_names(labels: &[super::discogs::Label]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in labels {
        let name = l.name.trim();
        if name.is_empty() || name.to_lowercase().starts_with("not on label") || out.iter().any(|x| x.eq_ignore_ascii_case(name)) {
            continue;
        }
        out.push(name.to_string());
    }
    out
}

/// The runner's kind of a record's Discogs read (CRED.11).
pub const READ_KIND: &str = "record:discogs";

/// A record's Discogs read, begun as a job (SCENARIOS-CRED CRED.11; the owner's rules, 2026-11-21 04:16 and 04:43): by any account's
/// open of the record, once while its stored facts are fresh (their time, the one due marker: T-CRED-28), only while Discogs is
/// switched on, with the server's key; never on a GET, and the open never waits on it. None begun where its facts are fresh,
/// Discogs is off (nothing queued for later), or its read runs already (one read per record at a time). Whether one was begun.
pub async fn begin_read(served: Arc<crate::serve::Serve>, record: i64) -> bool {
    if !super::switches::is_on(super::switches::DISCOGS) {
        return false;
    }
    let album = crate::serve::ids::record(record);
    let (ud, subject) = (served.userdata.clone(), album_subject(&album));
    let Ok(Ok(kept)) = crate::run::spawn_blocking(move || ud.discogs_facts(&subject)).await else { return false };
    if !due(kept.as_ref(), super::moment::now_secs()) {
        return false;
    }
    let Ok(handle) = crate::runner::runner().begin_keyed(READ_KIND, &album, None) else { return false };
    crate::runner::jobs::run(Arc::new(handle), move |_| read_record(served, record)).is_ok()
}

/// A record's Discogs facts read (`begin_read`'s work; CRED.6, CRED.11, T-CRED-13): its Discogs master and release by MusicBrainz's
/// link (the files' release's links as the works pass keeps them, else its release group's asked of MusicBrainz now), else a
/// Discogs reference it holds that counts (a link, a hand's pick, a match today's pairing confirms, or the one a record not had was
/// opened by); never by Discogs' search for its names. Its facts kept with their time (the due marker), its genres and styles as
/// Discogs' words, its master and release as its references by that link (`how` naming it, never "search"), its main release's
/// list as Discogs', and its credits by that release. Nothing to read it by: "asked, none" with its time.
pub async fn read_record(served: Arc<crate::serve::Serve>, record: i64) -> Result<()> {
    read_record_as(served, record, None, false).await
}

/// `read_record`, as the one scheduled job reads: `names`, the record's artist and title, a Discogs search by them where it has no
/// link nor reference that counts (never a name two artists here share, X8; what it finds a guess until today's pairing confirms it
/// whole: T-CRED-13, the owner's 02:32 rule); `typing`, its release type kept from the release's formats where MusicBrainz typed it
/// not (`types_pass`'s, folded).
pub async fn read_record_as(served: Arc<crate::serve::Serve>, record: i64, names: Option<(String, String)>, typing: bool) -> Result<()> {
    let album = crate::serve::ids::record(record);
    let subject = album_subject(&album);
    let (s, a) = (served.clone(), album.clone());
    let (kept, release, refs, linked) = crate::run::spawn_blocking(move || -> Result<_> {
        let kept = s.userdata.discogs_facts(&album_subject(&a))?;
        let release = s.library.album_names(&a)?.and_then(|(release, _, _)| release).filter(|r| super::discogs::is_mbid(r));
        let refs = s.catalogue().references_of(&a)?;
        let linked: Vec<String> = match &release {
            Some(r) => s.userdata.mb_release(r)?.and_then(|j| serde_json::from_str::<super::works::Stored>(&j).ok()).map(|st| st.discogs).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok((kept, release, refs, linked))
    })
    .await??;
    let now = super::moment::now_secs();
    if !due(kept.as_ref(), now) {
        return Ok(());
    }
    let id_of = |prefix: &str| linked.iter().find_map(|l| l.strip_prefix(prefix)).and_then(|x| x.parse::<u64>().ok());
    let (mut master, mut release_id, mut how) = (id_of("master:"), id_of("release:"), "link");
    let mut musicbrainz = None;
    // MusicBrainz's link, asked now where the works pass hasn't kept it: the release group's (F9's one question, its labels with it).
    if master.is_none()
        && release_id.is_none()
        && let Some(r) = &release
        && let Ok((group, labels)) = super::discogs::release_group_and_labels(&super::discover::http()?, r).await
    {
        musicbrainz = Some(labels);
        if let Some(g) = group {
            master = super::discogs::master_of(Some(&g), "", "").await?;
        }
    }
    // Else a Discogs reference the record holds that counts (T-CRED-13): never a name search's guess.
    if master.is_none() && release_id.is_none() {
        for (r, row) in refs.iter().filter(|(r, _)| r.source == "discogs") {
            if !(matches!(row.how.as_str(), "link" | "hand" | "files") || row.confirmed) {
                continue;
            }
            match r.kind.as_str() {
                "master" => master = master.or(r.value.parse().ok()),
                "release" => release_id = release_id.or(r.value.parse().ok()),
                _ => continue,
            }
            how = if row.how == "hand" { "hand" } else { "link" };
        }
    }
    // The scheduled job's last way: Discogs' search for its names, never a name two artists here share (X8), a guess until paired.
    let mut searched = false;
    if master.is_none()
        && release_id.is_none()
        && let Some((artist, title)) = &names
    {
        let (cat, key) = (served.catalogue().clone(), crate::serve::ids::artist_key(artist));
        let shared = crate::run::spawn_blocking(move || -> Result<bool> { Ok(cat.artists_of_key(&key)? > 1) }).await??;
        if !shared {
            master = super::discogs::masters_by_names(artist, title).await?.into_iter().next();
            searched = master.is_some();
            how = "search";
        }
    }
    let found = if master.is_some() || release_id.is_some() { super::discogs::album_facts_by(master, release_id).await? } else { None };
    let (mut facts, credits) = match found {
        Some((f, c)) => (Some(f), c),
        None => (None, Vec::new()),
    };
    // Today's pairing: a name-found master whose main release's list pairs whole with a copy's files is a reference (HS01); one that
    // doesn't stays a guess, its list kept with its facts.
    if searched && let Some(f) = facts.as_mut() {
        let (cat, list) = (served.catalogue().clone(), f.tracks.clone());
        let confirmed = crate::run::spawn_blocking(move || -> Result<bool> {
            Ok(cat.copy_files(record)?.values().any(|files| crate::catalogue::lists::confirms(&list, files)))
        })
        .await??;
        f.guess = !confirmed;
        if f.guess {
            f.list = Some(f.tracks.clone());
        }
    }
    keep_read(&served, &album, &subject, facts, credits, musicbrainz, how, now, typing).await
}

/// What a record's Discogs read found, kept (`read_record`): its facts and time, Discogs' words, MusicBrainz's labels where asked,
/// its references by `how` with Discogs' list, and its credits by the release (FA15's "asked, none" where it found none).
#[allow(clippy::too_many_arguments)]
async fn keep_read(
    served: &Arc<crate::serve::Serve>,
    album: &str,
    subject: &str,
    facts: Option<AlbumFacts>,
    credits: Vec<crate::catalogue::credits::CreditRow>,
    musicbrainz: Option<Vec<MbLabel>>,
    how: &'static str,
    now: i64,
    typing: bool,
) -> Result<()> {
    // A guess's facts count nowhere (T-CRED-13): no words, no reference, no list, its credits kept unshown.
    let guess = facts.as_ref().is_some_and(|f| f.guess);
    let words: Vec<RawWord> = facts
        .iter()
        .filter(|f| !f.guess)
        .flat_map(|f| {
            let genres = f.genres.iter().map(|g| RawWord { word: g.clone(), kind: "genre".into(), weight: 1.0 });
            genres.chain(f.styles.iter().map(|s| RawWord { word: s.clone(), kind: "style".into(), weight: 1.0 }))
        })
        .collect();
    let (s, a, subject) = (served.clone(), album.to_string(), subject.to_string());
    let json = facts.as_ref().map(serde_json::to_string).transpose()?;
    let labels = musicbrainz.as_ref().map(serde_json::to_string).transpose()?;
    crate::run::spawn_blocking(move || -> Result<()> {
        if let Some(labels) = &labels {
            s.userdata.set_discogs_facts(&musicbrainz_subject(&a), Some(labels), now)?;
        }
        s.userdata.set_discogs_facts(&subject, json.as_deref(), now)?;
        s.userdata.set_words(&subject, "discogs", &words, true, now)?;
        if let Some(f) = &facts {
            if !guess {
                super::references::keep_discogs_as(&s, &a, f, how)?;
                // `genres_pass`'s and `types_pass`'s, folded: the library's extra genres, and its Discogs type where MusicBrainz gave none.
                s.userdata.set_discogs_genres(&[(a.clone(), f.genres.iter().chain(&f.styles).cloned().collect())], now)?;
                if typing {
                    let (primary, secondary) = crate::serve::library::release_words(f.formats.iter().map(String::as_str));
                    s.userdata.set_release_types(&[(format!("dc:{a}"), primary, secondary)], now)?;
                }
            }
            if let (Some(record), Some(release)) = (crate::serve::ids::record_number(&a), f.release.or(f.master)) {
                s.catalogue().keep_credits(record, "discogs", &format!("release:{release}"), Some(&credits), guess, now)?;
            }
        }
        Ok(())
    })
    .await?
}

/// The one scheduled Discogs job's kind on the runner (CRED.11 (g), T-CRED-28).
pub const PASS_KIND: &str = "discogs:records";
/// Records read in one of its passes, at Discogs' pace (its 60 a minute, two asks a record).
const PASS_BATCH: usize = 100;

/// The one scheduled Discogs job (SCENARIOS-CRED CRED.11 (g), T-CRED-28; a change DC3c makes): `fill`'s round, `genres_pass`,
/// `types_pass` and harvest's own Discogs loop folded into one, on `discogs::start`'s runner, paused with background reading (O5),
/// only while Discogs is on. Its list: the records lacking a genre, a year or a type first, Incoming's arrivals included (the intake
/// plan's GET no longer reads), then every other album (harvest's), each asked once while its facts are fresh by the one due marker
/// it shares with the record's open (its facts' time): never once by each of the rounds it folds. Each read as the open's is, by
/// MusicBrainz's link or a reference that counts; where it has neither, by Discogs' search for its names (never by an artist name
/// two artists here share, X8), and what that finds is a guess until today's pairing confirms it whole (the owner's 02:32 rule,
/// HS01): kept with its facts, its credits unshown, its genres, styles and types counted nowhere, and no reference kept. How many
/// it read.
pub async fn scheduled_pass(served: Arc<crate::serve::Serve>) -> Result<usize> {
    if !super::switches::is_on(super::switches::DISCOGS) {
        return Ok(0);
    }
    let (s, now) = (served.clone(), super::moment::now_secs());
    let (due, typed): (Vec<(String, String, String)>, std::collections::HashSet<String>) = crate::run::spawn_blocking(move || -> Result<_> {
        let lacking: std::collections::HashSet<String> = s.library.lacking()?.into_iter().filter(|l| l.genre || l.year).map(|l| l.id).collect();
        let untyped: std::collections::HashSet<String> = s.library.albums_to_type()?.into_iter().map(|a| a.0).collect();
        let mut out: Vec<(bool, String, String, String)> = Vec::new();
        for (album, _, artist, title) in s.library.album_subjects()? {
            if due(s.userdata.discogs_facts(&album_subject(&album))?.as_ref(), now) {
                out.push((!(lacking.contains(&album) || untyped.contains(&album)), album, artist, title));
            }
        }
        out.sort_by_key(|o| o.0);
        Ok((out.into_iter().take(PASS_BATCH).map(|(_, a, ar, t)| (a, ar, t)).collect(), untyped))
    })
    .await??;
    if due.is_empty() {
        return Ok(0);
    }
    let Ok(job) = crate::runner::runner().begin_keyed(PASS_KIND, PASS_KIND, None) else { return Ok(0) };
    let (total, mut n) = (due.len() as u64, 0usize);
    for (i, (album, artist, title)) in due.into_iter().enumerate() {
        if super::switches::background_paused() || !super::switches::is_on(super::switches::DISCOGS) {
            break;
        }
        job.step("Reading Discogs", i as u64, Some(total));
        super::discogs::on_turn(&job, &super::discogs::TURN);
        let Some(record) = crate::serve::ids::record_number(&album) else { continue };
        match read_record_as(served.clone(), record, Some((artist, title)), typed.contains(&album)).await {
            Ok(()) => n += 1,
            Err(e) => tracing::info!("discogs: {album}: {e:#}"),
        }
    }
    job.finish(Ok::<_, anyhow::Error>(n));
    Ok(n)
}

/// An artist's facts, by their MusicBrainz id where known, else their name.
pub async fn artist(served: &Arc<crate::serve::Serve>, name: &str, mbid: Option<&str>) -> Result<Option<ArtistFacts>> {
    let subject = format!("artist:{}", super::words::key(name));
    let (ud, key) = (served.userdata.clone(), subject.clone());
    let kept = crate::run::spawn_blocking(move || ud.discogs_facts(&key)).await??;
    let now = super::moment::now_secs();
    if !due(kept.as_ref(), now) {
        return Ok(parse(kept));
    }
    let (facts, link_unread) = super::discogs::artist_facts(mbid.filter(|m| super::discogs::is_mbid(m)), name).await?;
    let (ud, json) = (served.userdata.clone(), facts.as_ref().map(serde_json::to_string).transpose()?);
    let at = kept_at(now, facts.is_some(), link_unread);
    crate::run::spawn_blocking(move || ud.set_discogs_facts(&subject, json.as_deref(), at)).await??;
    Ok(facts)
}

/// A release group's genres and styles (Discover).
pub async fn release_group(served: &Arc<crate::serve::Serve>, group: &str) -> Result<Option<AlbumFacts>> {
    let subject = format!("rg:{group}");
    let (ud, key) = (served.userdata.clone(), subject.clone());
    let kept = crate::run::spawn_blocking(move || ud.discogs_facts(&key)).await??;
    let now = super::moment::now_secs();
    if !due(kept.as_ref(), now) {
        return Ok(parse(kept));
    }
    let facts = super::discogs::release_group_facts(group).await?;
    let (ud, json) = (served.userdata.clone(), facts.as_ref().map(serde_json::to_string).transpose()?);
    crate::run::spawn_blocking(move || ud.set_discogs_facts(&subject, json.as_deref(), now)).await??;
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::discogs::Label;

    /// F47: an artist's facts read while MusicBrainz's link could not be had are asked again after a quarter hour, found or not; read
    /// with it, after a month (none found: a week).
    #[test]
    fn facts_read_without_musicbrainzs_link_are_asked_again_soon() {
        let now = 1_800_000_000;
        let kept = |found: bool, unread: bool| (kept_at(now, found, unread), found.then(|| "{}".to_string()));
        for found in [true, false] {
            assert!(!due(Some(&kept(found, true)), now + LINK_AGAIN - 1), "kept a while");
            assert!(due(Some(&kept(found, true)), now + LINK_AGAIN), "asked again after a quarter hour");
            assert!(!due(Some(&kept(found, false)), now + 86_400), "with the link: kept");
        }
        assert!(due(Some(&kept(true, false)), now + AGAIN) && due(Some(&kept(false, false)), now + NONE_AGAIN));
    }

    #[test]
    fn discogs_labels_are_named_once_and_not_on_label_is_no_label() {
        let l = |name: &str| Label { name: name.into(), catno: None, id: None, namesake: false };
        let names = label_names(&[l("Warp Records"), l("warp records"), l("Not On Label (Air Self-released)"), l(" "), l("Bleep")]);
        assert_eq!(names, ["Warp Records", "Bleep"]);
    }

    /// F9.1 (b), (e): Discogs' answer names an album's labels as About shows them: Discogs' each with its catalogue number, by the
    /// label its id is held by (`id`), else that id to open one by (`ref`); "Not On Label" as text; then MusicBrainz's of the files'
    /// release (by their MBIDs) of a name Discogs names none of. The rest of Discogs' facts as before; MusicBrainz's alone where
    /// Discogs has none; `{}` where neither names any. Nothing made by answering.
    #[test]
    fn the_discogs_answer_names_each_label_as_about_shows_it() {
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let held = cat.open_label(&Ref::new("discogs", "label", "23528"), "Low Tide Records", true).unwrap();
        let (anthem, nonesuch) = ("00000000-0000-4000-8000-000000000901", "00000000-0000-4000-8000-000000000902");
        let label = |name: &str, catno: Option<&str>, id: u64| Label { name: name.into(), catno: catno.map(str::to_string), id: Some(id), namesake: false };
        let facts = AlbumFacts {
            master: Some(3001),
            year: Some(2021),
            labels: vec![label("Low Tide Records", Some("LTR-012"), 23528), label("International Anthem", Some("IARC0041"), 4401), label("Not On Label (Kite Theory Self-released)", None, 1818)],
            ..AlbumFacts::default()
        };
        let mb = |name: &str, mbid: &str, catno: Option<&str>| MbLabel { name: name.into(), mbid: mbid.into(), catno: catno.map(str::to_string) };
        let musicbrainz = [mb("International Anthem", anthem, Some("IARC0041")), mb("Nonesuch", nonesuch, Some("075597"))];
        let v = answer_with(&cat, Some(&facts), &musicbrainz).unwrap();
        assert_eq!(
            v["labels"],
            serde_json::json!([
                { "name": "Low Tide Records", "catno": "LTR-012", "id": crate::serve::ids::label(held) },
                { "name": "International Anthem", "catno": "IARC0041", "ref": { "source": "discogs", "id": "4401" } },
                { "name": "Not On Label (Kite Theory Self-released)" },
                { "name": "Nonesuch", "catno": "075597", "ref": { "source": "musicbrainz", "id": nonesuch } },
            ])
        );
        assert_eq!((v["master"].as_u64(), v["year"].as_u64()), (Some(3001), Some(2021)), "the rest as before");
        let alone = serde_json::json!({ "labels": [{ "name": "Nonesuch", "catno": "075597", "ref": { "source": "musicbrainz", "id": nonesuch } }] });
        assert_eq!(answer_with(&cat, None, &musicbrainz[1..]).unwrap(), alone, "MusicBrainz's alone");
        assert_eq!(answer_with(&cat, None, &[]).unwrap(), serde_json::json!({}));
        let made: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM labels", [], |r| r.get(0)).unwrap();
        assert_eq!(made, 1, "nothing made by answering");
    }

    /// F9: a reference an album's labels were kept with, at Discogs (its facts) or at MusicBrainz (its release's labels, kept beside
    /// them), names the label one may be made by, by that name; another names none. The crates read Discogs' labels alone, as before.
    #[test]
    fn a_label_is_named_by_what_its_album_kept() {
        let (nonesuch, other) = ("00000000-0000-4000-8000-000000000902", "00000000-0000-4000-8000-000000000903");
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        let facts = r#"{"labels":[{"name":"Low Tide Records","catno":"LTR-012","id":23528},{"name":"Kite Records"}]}"#;
        served.userdata.set_discogs_facts("album:al-1", Some(facts), 1).unwrap();
        let kept = serde_json::to_string(&[MbLabel { name: "Nonesuch".into(), mbid: nonesuch.into(), catno: None }]).unwrap();
        served.userdata.set_discogs_facts(&musicbrainz_subject("al-1"), Some(&kept), 1).unwrap();
        let named = |r: Ref| label_named(&served, &r).unwrap();
        assert_eq!(named(Ref::new("discogs", "label", "23528")), Some(("Low Tide Records".to_string(), false)));
        assert_eq!(named(Ref::new("musicbrainz", "label", nonesuch)), Some(("Nonesuch".to_string(), false)));
        assert_eq!(named(Ref::new("discogs", "label", "4401")), None);
        assert_eq!(named(Ref::new("musicbrainz", "label", other)), None);
        assert_eq!(named(Ref::new("discogs", "master", "23528")), None, "a label's reference alone");
        assert_eq!(musicbrainz_labels(&served, "al-1").unwrap().len(), 1);
        assert_eq!(kept_labels(&served).unwrap().get("al-1"), Some(&vec!["Low Tide Records".to_string(), "Kite Records".to_string()]), "the crates as before");
        assert_eq!(kept_labels(&served).unwrap().len(), 1, "MusicBrainz's kept labels are no album's facts");
    }
}
