//! AudioMuse's analysis, taken in (decision 0054; the owner, 2026-10-19: "Yes, import them"): energy, key, its style
//! tags, CLAP's moods and danceability, and its "sounds like" vectors. AudioMuse runs on the owner's PC, not here (the
//! owner: it "does not move to the NAS"). A scheduled task there writes what AudioMuse's database holds as JSON lines,
//! `<data>/audiomuse/audiomuse.jsonl`, copied over SSH every half hour when it changed. Here the file is read when it
//! changed, and the whole set replaces the last: the tracks AudioMuse matched to Cratarr's own (`tr-…`) are kept, the
//! others counted.

use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::catalogue::{AudioMuseRow, Catalogue};

/// The file the PC's task writes, in Cratarr's data folder.
pub const FILE: &str = "audiomuse/audiomuse.jsonl";
/// How many style tags a track shows (and a crate's Style tests).
pub const STYLES: usize = 5;
/// The catalogue's note of the file last taken in (its size and time).
const STAMP: &str = "audiomuse_file";

/// One track, as the PC's task writes it from AudioMuse's `score` and `embedding` tables.
#[derive(Deserialize)]
struct Line {
    /// Cratarr's own track id, as AudioMuse matched it (`track_server_map.provider_track_id`).
    track: String,
    #[serde(default)]
    energy: Option<f64>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    scale: Option<String>,
    /// `mood_vector`: AudioMuse's style tags with their scores, `electronic:0.608,chillout:0.561,…`.
    #[serde(default)]
    style: Option<String>,
    /// `other_features`: CLAP's `danceable:0.71,happy:0.40,…`; all 0 until CLAP has heard the track.
    #[serde(default)]
    moods: Option<String>,
    /// The similarity vector, little-endian 32-bit floats, in base64.
    #[serde(default)]
    embedding: Option<String>,
}

/// What a file brought in.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub kept: u64,
    /// A track Cratarr does not have (gone since AudioMuse heard it).
    pub unknown: u64,
    /// Not a line of this kind.
    pub bad: u64,
}

/// Takes the file in when it changed since the last time (its size and time); `None` when it had not, or is not
/// there. Blocking.
pub fn import_if_changed(cat: &Catalogue, path: &Path) -> Result<Option<Counts>> {
    let Ok(meta) = std::fs::metadata(path) else { return Ok(None) };
    let at = meta.modified()?.duration_since(UNIX_EPOCH)?.as_secs();
    let stamp = format!("{}:{at}", meta.len());
    if cat.meta(STAMP).as_deref() == Some(stamp.as_str()) {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let (rows, counts) = parse(cat, &text);
    cat.replace_audiomuse(&rows)?;
    cat.set_meta(STAMP, &stamp);
    Ok(Some(counts))
}

fn parse(cat: &Catalogue, text: &str) -> (Vec<AudioMuseRow>, Counts) {
    use base64::Engine;
    let mut counts = Counts::default();
    let mut rows = Vec::new();
    for raw in text.lines() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            counts.bad += 1;
            continue;
        };
        let Some(track) = cat.track_of_source(crate::source::cratarr::KIND.id, &line.track) else {
            counts.unknown += 1;
            continue;
        };
        let key = match (line.key.filter(|k| !k.is_empty()), line.scale.filter(|s| !s.is_empty())) {
            (Some(k), Some(s)) => Some(format!("{k} {s}")),
            (k, _) => k,
        };
        let embedding = line.embedding.and_then(|e| base64::engine::general_purpose::STANDARD.decode(e).ok()).filter(|e| e.len() % 4 == 0 && !e.is_empty());
        rows.push(AudioMuseRow { track, energy: line.energy, key, style: line.style, moods: line.moods, embedding });
        counts.kept += 1;
    }
    (rows, counts)
}

/// Names with scores as AudioMuse writes them (`name:0.608,name:0.561`), the highest first.
pub fn weighted(text: Option<&str>) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = text
        .unwrap_or_default()
        .split(',')
        .filter_map(|p| {
            let (name, score) = p.trim().rsplit_once(':')?;
            // Lowercase, as most of its tags are: four come capitalised from its model's tag list ("Hip-Hop",
            // "House", "Mellow", "Progressive rock").
            Some((name.trim().to_lowercase(), score.trim().parse::<f64>().ok()?))
        })
        .filter(|(n, s)| !n.is_empty() && s.is_finite())
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

/// CLAP's score of each mood (`similarity::MOODS`' order); none until CLAP heard the track (all 0).
pub fn mood_scores(moods: Option<&str>) -> Option<[f32; 5]> {
    let scores = weighted(moods);
    scores.iter().any(|(_, x)| *x > 0.0).then(|| crate::similarity::MOODS.map(|m| scores.iter().find(|(n, _)| n == m).map_or(0.0, |(_, x)| *x as f32)))
}

/// Where each of CLAP's moods stands out (the owner, 2026-10-23): the library's top quarter of that mood's scores,
/// over the tracks CLAP heard. CLAP scores every mood between about 0.5 and 0.65, so a fixed bar (AudioMuse's 0.3)
/// gave every heard track all five. A mood no track was heard on: never.
pub fn stand_out(heard: impl Iterator<Item = [f32; 5]>) -> [f64; 5] {
    let mut each: [Vec<f64>; 5] = Default::default();
    for scores in heard {
        for (v, x) in each.iter_mut().zip(scores) {
            v.push(x as f64);
        }
    }
    each.map(|mut v| {
        v.sort_by(f64::total_cmp);
        if v.is_empty() { f64::INFINITY } else { crate::similarity::quantile(&v, 0.75) }
    })
}

/// The moods a track has: those it scores in the library's top quarter of (`stand_out`).
pub fn moods_of(scores: Option<[f32; 5]>, stand_out: &[f64; 5]) -> Vec<(&'static str, f64)> {
    let Some(scores) = scores else { return Vec::new() };
    crate::similarity::MOODS.into_iter().zip(scores).zip(stand_out).filter(|((_, x), t)| *x > 0.0 && *x as f64 >= **t).map(|((m, x), _)| (m, (x as f64 * 100.0).round() / 100.0)).collect()
}

/// CLAP's danceability: none while every score is 0 (CLAP has not heard the track yet).
pub fn danceability(row: &AudioMuseRow) -> Option<f64> {
    let all = weighted(row.moods.as_deref());
    if all.iter().all(|(_, s)| *s <= 0.0) {
        return None;
    }
    all.iter().find(|(n, _)| n == "danceable").map(|(_, s)| *s)
}

/// A track's AudioMuse facts, for its details: `energy` 0..1, `key` ("D minor"), `style` (its top tags), `mood`
/// (those that stand out against the library: `stand_out`), `danceability` 0..1; each null where AudioMuse has none.
pub fn details(row: Option<&AudioMuseRow>, stand_out: &[f64; 5]) -> Value {
    let Some(row) = row else {
        return json!({ "energy": null, "key": null, "style": null, "mood": null, "danceability": null });
    };
    let named = |v: Vec<(String, f64)>| -> Value {
        if v.is_empty() {
            return Value::Null;
        }
        Value::Array(v.into_iter().map(|(name, score)| json!({ "name": name, "score": score })).collect())
    };
    let mood = moods_of(mood_scores(row.moods.as_deref()), stand_out).into_iter().map(|(m, x)| (m.to_string(), x)).collect();
    json!({
        "energy": row.energy,
        "key": row.key,
        "style": named(weighted(row.style.as_deref()).into_iter().take(STYLES).collect()),
        "mood": named(mood),
        "danceability": danceability(row),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mood_stands_out_in_the_librarys_top_quarter() {
        let heard = ["happy:0.50,sad:0.60", "happy:0.55,sad:0.61", "happy:0.60,sad:0.62", "happy:0.65,sad:0.63", "happy:0.00,sad:0.00"];
        let t = stand_out(heard.iter().filter_map(|s| mood_scores(Some(s))));
        assert!((t[0] - 0.6125).abs() < 1e-6, "happy: the top quarter of 0.50..0.65, the unheard one left out: {}", t[0]);
        assert!((t[1] - 0.6225).abs() < 1e-6, "sad: {}", t[1]);
        assert!((t[2] - 0.0).abs() < 1e-9, "relaxed: every heard track scored it 0: {}", t[2]);
    }

    #[test]
    fn scores_read_as_written_and_moods_wait_for_clap() {
        assert_eq!(weighted(Some("electronic:0.608, chillout:0.7,bad,x:nan")), vec![("chillout".to_string(), 0.7), ("electronic".to_string(), 0.608)]);
        let row = |moods: &str| AudioMuseRow { track: 1, energy: Some(0.7), key: Some("D minor".into()), style: None, moods: Some(moods.into()), embedding: None };
        assert_eq!(danceability(&row("danceable:0.00,happy:0.00,sad:0.00")), None, "CLAP not run yet");
        assert_eq!(mood_scores(Some("danceable:0.00,happy:0.00")), None);
        assert_eq!(danceability(&row("danceable:0.71,happy:0.40,sad:0.10,relaxed:0.31")), Some(0.71));
        // Happy stands out above 0.35, sad above 0.5: this track is happy, not sad.
        let bars = [0.35, 0.5, 0.5, 0.5, 0.5];
        let d = details(Some(&row("danceable:0.71,happy:0.40,sad:0.45")), &bars);
        assert_eq!(d["key"], "D minor");
        assert_eq!(d["style"], Value::Null);
        assert_eq!(d["mood"].as_array().map(|m| m.iter().map(|x| x["name"].clone()).collect::<Vec<_>>()), Some(vec![json!("happy")]));
        assert_eq!(details(None, &bars)["energy"], Value::Null);
    }

    #[test]
    fn a_file_is_taken_in_whole_when_it_changed_and_only_then() {
        use base64::Engine;
        let d = std::env::temp_dir().join(format!("cratarr-audiomuse-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(d.join("A")).unwrap();
        std::fs::write(d.join("A/01 x.flac"), b"some sound").unwrap();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let track = cat.track_of(0, "A/01 x.flac").unwrap();
        let id = crate::serve::ids::song(track);
        let vector = base64::engine::general_purpose::STANDARD.encode([1.0f32, 0.5].iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>());
        let file = d.join("audiomuse.jsonl");
        let line = format!(r#"{{"track":"{id}","energy":0.7,"key":"D","scale":"minor","style":"electronic:0.6","moods":"danceable:0.00","embedding":"{vector}"}}"#);
        std::fs::write(&file, format!("{line}\n{{\"track\":\"tr-999999\"}}\nnot json\n")).unwrap();
        let counts = import_if_changed(&cat, &file).unwrap().unwrap();
        assert_eq!(counts, Counts { kept: 1, unknown: 1, bad: 1 });
        let row = cat.audiomuse_of(track).unwrap();
        assert_eq!((row.energy, row.key.as_deref(), row.embedding.as_ref().map(Vec::len)), (Some(0.7), Some("D minor"), Some(8)));
        assert!(import_if_changed(&cat, &file).unwrap().is_none(), "not changed: not read again");
        // A new file: the set replaces the last (the track gone from it is gone here too).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&file, "\n").unwrap();
        assert_eq!(import_if_changed(&cat, &file).unwrap().unwrap(), Counts::default());
        assert!(cat.audiomuse_of(track).is_none());
        std::fs::remove_dir_all(&d).ok();
    }
}
