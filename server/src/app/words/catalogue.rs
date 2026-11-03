//! The Catalogue sheet's server side (decision 0097's controls, 0.15): for each kind of word, whether Cratarr's are
//! written to the files, which sources say it (each on or off: a source off takes its words out of the kind
//! everywhere, kept to come back), and the kind's alias list (one spelling per word: real synonyms, the folding of
//! case, spaces and hyphens being automatic).
//!
//! - **Write**: genre, type and year by default (the decision); style, mood and tags leave the files' own. Type and year
//!   are one switch, 0.13's "Write types and years into files" (`file_facts`): one run writes both.
//! - **Sources** by kind as the store has them (`store`): mood is AudioMuse's (and the hand's); the hand is always on.
//! - **Aliases**: genre's are the tag rules' (file fixes use them too); the others' are kept here.
//!
//! Kept as server choices (`words:write:<kind>`, `words:off:<kind>`, `words:aliases:<kind>`); the store is built again
//! when they change (`userdata::words_stamp` counts them).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::store::{Kind, Source};

/// Every kind the sheet lists, in its order.
pub const ALL: [&str; 6] = ["type", "year", "genre", "style", "mood", "tag"];

/// The sources that can say a kind, strongest first.
fn sources_of(kind: Kind) -> &'static [Source] {
    use Source::*;
    match kind {
        Kind::Genre | Kind::Tag => &[Hand, Files, Household, Musicbrainz, Discogs, Lastfm],
        Kind::Style => &[Hand, Files, Audiomuse, Household, Musicbrainz, Discogs, Lastfm],
        Kind::Mood => &[Hand, Audiomuse],
    }
}

/// A source's name as the sheet shows it.
fn named(s: Source) -> &'static str {
    match s {
        Source::Hand => "By hand",
        Source::Files => "The files",
        Source::Audiomuse => "AudioMuse",
        Source::Household => "Household tags",
        Source::Musicbrainz => "MusicBrainz",
        Source::Discogs => "Discogs",
        Source::Lastfm => "Last.fm",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceRow {
    pub id: &'static str,
    pub name: &'static str,
    pub on: bool,
}

/// One word and the spellings that are it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliasRow {
    pub word: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindRow {
    pub kind: &'static str,
    pub write: bool,
    pub sources: Vec<SourceRow>,
    pub aliases: Vec<AliasRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Catalogue {
    pub kinds: Vec<KindRow>,
}

fn write_choice(kind: &str) -> String {
    format!("words:write:{kind}")
}

fn off_choice(kind: &str) -> String {
    format!("words:off:{kind}")
}

fn aliases_choice(kind: &str) -> String {
    format!("words:aliases:{kind}")
}

/// The sheet's name for a kind, as `ALL` spells it.
fn kind_name(kind: &str) -> Result<&'static str> {
    match ALL.iter().find(|k| **k == kind) {
        Some(k) => Ok(k),
        None => bail!("No such kind of word: {kind}"),
    }
}

/// Whether a kind is written to the files: type and year as 0.13's switch; the others as chosen, genre by default.
pub fn writes(serve: &crate::serve::Serve, kind: &str) -> bool {
    match kind {
        "type" | "year" => crate::app::file_facts::is_on(serve),
        k => match serve.userdata.import_choice(&write_choice(k)).ok().flatten() {
            Some(c) => c == "1",
            None => k == "genre",
        },
    }
}

/// The sources switched off, by kind.
pub fn off(serve: &crate::serve::Serve) -> HashMap<Kind, HashSet<Source>> {
    super::store::KINDS
        .into_iter()
        .filter_map(|k| {
            let chosen = serve.userdata.import_choice(&off_choice(k.name())).ok().flatten()?;
            let set: HashSet<Source> = chosen.split(',').filter_map(|s| Source::of(s.trim())).filter(|s| *s != Source::Hand).collect();
            (!set.is_empty()).then_some((k, set))
        })
        .collect()
}

/// Each kind's aliases: the word, and the spellings that are it (genre's the tag rules').
pub fn aliases(serve: &crate::serve::Serve) -> HashMap<Kind, Vec<(String, Vec<String>)>> {
    let mut out = HashMap::from([(Kind::Genre, serve.rules().genre_aliases.iter().map(|a| (a.to.clone(), a.from.clone())).collect::<Vec<_>>())]);
    for k in [Kind::Style, Kind::Mood, Kind::Tag] {
        out.insert(k, kept_aliases(serve, k.name()).into_iter().map(|a| (a.word, a.aliases)).collect());
    }
    out
}

fn kept_aliases(serve: &crate::serve::Serve, kind: &str) -> Vec<AliasRow> {
    serve.userdata.import_choice(&aliases_choice(kind)).ok().flatten().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}

fn alias_rows(serve: &crate::serve::Serve, kind: Kind) -> Vec<AliasRow> {
    match kind {
        Kind::Genre => serve.rules().genre_aliases.iter().map(|a| AliasRow { word: a.to.clone(), aliases: a.from.clone() }).collect(),
        k => kept_aliases(serve, k.name()),
    }
}

/// One kind's row.
pub fn row(serve: &crate::serve::Serve, kind: &str) -> Result<KindRow> {
    let name = kind_name(kind)?;
    let Some(k) = Kind::of(name) else {
        return Ok(KindRow { kind: name, write: writes(serve, name), sources: Vec::new(), aliases: Vec::new() });
    };
    let off = off(serve);
    let sources = sources_of(k).iter().map(|s| SourceRow { id: s.name(), name: named(*s), on: !off.get(&k).is_some_and(|o| o.contains(s)) }).collect();
    Ok(KindRow { kind: name, write: writes(serve, name), sources, aliases: alias_rows(serve, k) })
}

/// The whole sheet.
pub fn view(serve: &crate::serve::Serve) -> Result<Catalogue> {
    Ok(Catalogue { kinds: ALL.iter().map(|k| row(serve, k)).collect::<Result<_>>()? })
}

/// A kind's writing and its sources set (`sources`: by id, on or off; the hand cannot be switched off). Type and year
/// are one switch: setting either sets both.
pub fn set_kind(serve: &crate::serve::Serve, kind: &str, write: Option<bool>, sources: Option<&HashMap<String, bool>>) -> Result<KindRow> {
    let name = kind_name(kind)?;
    if let Some(w) = write {
        match name {
            "type" | "year" => crate::app::file_facts::set_on(serve, w)?,
            k => serve.userdata.set_import_choice(&write_choice(k), if w { "1" } else { "0" })?,
        }
    }
    if let Some(sources) = sources {
        let Some(k) = Kind::of(name) else { bail!("{name} has no sources to switch") };
        let mut off: HashSet<Source> = off(serve).remove(&k).unwrap_or_default();
        for (id, on) in sources {
            let Some(s) = Source::of(id).filter(|s| sources_of(k).contains(s)) else { bail!("{id} does not say a {name}") };
            if s == Source::Hand {
                if !on {
                    bail!("What is set by hand is always kept");
                }
                continue;
            }
            if *on {
                off.remove(&s);
            } else {
                off.insert(s);
            }
        }
        let mut list: Vec<&str> = off.iter().map(|s| s.name()).collect();
        list.sort_unstable();
        serve.userdata.set_import_choice(&off_choice(name), &list.join(","))?;
    }
    row(serve, name)
}

/// A kind's alias list with one word's spellings set (none: the word's entry goes); a spelling of another word's taken
/// from it (one word per spelling). Each spelling once, trimmed; never the word itself.
fn with_alias(mut list: Vec<AliasRow>, word: &str, aliases: &[String]) -> Result<Vec<AliasRow>> {
    let word = word.trim();
    if word.is_empty() {
        bail!("A word is needed");
    }
    let fold = super::key;
    let mut seen = HashSet::from([fold(word)]);
    let spellings: Vec<String> = aliases.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty() && seen.insert(fold(a))).collect();
    // Its spellings, and the word itself: none of them another word's any more.
    let taken: HashSet<String> = spellings.iter().map(|a| fold(a)).chain(std::iter::once(fold(word))).collect();
    for r in &mut list {
        r.aliases.retain(|a| !taken.contains(&fold(a)));
    }
    list.retain(|r| fold(&r.word) != fold(word) && !r.aliases.is_empty());
    if !spellings.is_empty() {
        list.push(AliasRow { word: word.to_string(), aliases: spellings });
    }
    list.sort_by_key(|r| fold(&r.word));
    Ok(list)
}

/// A kind's alias list kept (genre's into the tag rules).
async fn keep_aliases(app: &Arc<crate::app::App>, serve: &crate::serve::Serve, kind: Kind, list: Vec<AliasRow>) -> Result<Vec<AliasRow>> {
    match kind {
        Kind::Genre => {
            let mut rules = app.settings.tag_rules();
            rules.genre_aliases = list.iter().map(|a| crate::catalogue::normalise::Alias { from: a.aliases.clone(), to: a.word.clone() }).collect();
            app.settings.save_tag_rules(rules).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        k => serve.userdata.set_import_choice(&aliases_choice(k.name()), &serde_json::to_string(&list)?)?,
    }
    Ok(list)
}

fn word_kind(kind: &str) -> Result<Kind> {
    match Kind::of(kind) {
        Some(k) => Ok(k),
        None => bail!("{kind} has no aliases (genre, style, mood and tag have)"),
    }
}

/// A word's spellings set in its kind's alias list (none: its entry goes).
pub async fn set_alias(app: &Arc<crate::app::App>, serve: &crate::serve::Serve, kind: &str, word: &str, aliases: &[String]) -> Result<Vec<AliasRow>> {
    let k = word_kind(kind)?;
    let list = with_alias(alias_rows(serve, k), word, aliases)?;
    keep_aliases(app, serve, k, list).await
}

/// A word merged into another (a word's menu: Merge into…, and Rename everywhere into a new spelling): `from`, and the
/// spellings that were it, become spellings of `into`.
pub async fn merge(app: &Arc<crate::app::App>, serve: &crate::serve::Serve, kind: &str, from: &str, into: &str) -> Result<Vec<AliasRow>> {
    let k = word_kind(kind)?;
    let fold = super::key;
    if fold(from).is_empty() || fold(into).is_empty() {
        bail!("Both words are needed");
    }
    let list = alias_rows(serve, k);
    let mut spellings: Vec<String> = list.iter().find(|r| fold(&r.word) == fold(into)).map(|r| r.aliases.clone()).unwrap_or_default();
    spellings.extend(list.iter().find(|r| fold(&r.word) == fold(from)).map(|r| r.aliases.clone()).unwrap_or_default());
    if fold(from) != fold(into) {
        spellings.push(from.trim().to_string());
    }
    let list = with_alias(list, into, &spellings)?;
    keep_aliases(app, serve, k, list).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(word: &str, aliases: &[&str]) -> AliasRow {
        AliasRow { word: word.into(), aliases: aliases.iter().map(|a| a.to_string()).collect() }
    }

    #[test]
    fn a_spelling_is_one_words_and_an_empty_list_takes_the_entry_away() {
        let list = vec![row("Hip Hop", &["rap", "hiphop"])];
        // "rap" given to another word: taken from Hip Hop; the word itself and repeats are not its own aliases.
        let out = with_alias(list, "Rap", &["rap".into(), "RAP".into(), " gangsta ".into(), "Rap".into()]).unwrap();
        assert_eq!(out, vec![row("Hip Hop", &["hiphop"]), row("Rap", &["gangsta"])]);
        let out = with_alias(out, "rap", &[]).unwrap();
        assert_eq!(out, vec![row("Hip Hop", &["hiphop"])], "none: its entry goes");
        assert!(with_alias(out, "  ", &["x".into()]).is_err());
    }

    #[test]
    fn the_sheet_lists_every_kind_and_mood_says_only_audiomuse_and_the_hand() {
        assert_eq!(ALL, ["type", "year", "genre", "style", "mood", "tag"]);
        assert_eq!(sources_of(Kind::Mood), &[Source::Hand, Source::Audiomuse]);
        assert!(sources_of(Kind::Genre).contains(&Source::Files) && !sources_of(Kind::Genre).contains(&Source::Audiomuse));
        assert!(kind_name("colour").is_err());
    }
}
