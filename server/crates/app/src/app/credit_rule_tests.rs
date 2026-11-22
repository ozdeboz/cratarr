//! DC3a 3a-1: the record's credit by its deciding copy's library rules (SCENARIOS-DC3 F54.6, as the coordinator corrected it for
//! GEN.63 on 2026-11-22: artist separators are "/" and ";" alone, so the two libraries differ by ";").

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::catalogue::{Catalogue, Encoding, FolderRules, Rules};
use crate::serve::Serve;
use crate::serve::copies::Kind;

/// A FLAC in `dir`, `name`, with these tags.
fn tagged(dir: &Path, name: &str, tags: &[(lofty::tag::ItemKey, &str)]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = crate::retag::fixtures::flac(dir, name);
    let set = |t: &mut lofty::tag::Tag| {
        for (key, value) in tags {
            t.insert_text(key.clone(), value.to_string());
        }
    };
    crate::retag::rewrite(&path, set, |_| Ok(())).unwrap();
    path
}

/// One copy of "Kruder; Dorfmeister"'s "The K&D Sessions" in `dir`: two tracks, its MusicBrainz release `release`.
fn copy(dir: &Path, release: &str) -> Vec<PathBuf> {
    use lofty::tag::ItemKey;
    (1..=2)
        .map(|t| {
            let n = t.to_string();
            let title = format!("Session {t}");
            tagged(
                dir,
                &format!("{t:02} Session"),
                &[
                    (ItemKey::AlbumTitle, "The K&D Sessions"),
                    (ItemKey::AlbumArtist, "Kruder; Dorfmeister"),
                    (ItemKey::TrackArtist, "Kruder; Dorfmeister"),
                    (ItemKey::TrackTitle, title.as_str()),
                    (ItemKey::TrackNumber, n.as_str()),
                    (ItemKey::MusicBrainzReleaseId, release),
                ],
            )
        })
        .collect()
}

/// The names of a record's artists, in order.
fn credited(cat: &Catalogue, record: i64) -> Vec<String> {
    cat.record_artists(record).unwrap().into_iter().map(|a| cat.artist_named(a).unwrap().map(|n| n.name).unwrap_or_default()).collect()
}

/// F54.6, as corrected: a record with copies in MUSIC (ranked first, its artist separators ["/"]) and MORE (["/", ";"]), ALBUMARTIST
/// "Kruder; Dorfmeister": one artist while MUSIC's copy decides, two once MUSIC's copy is gone and MORE's decides; MUSIC's rules
/// changed to ["/", ";"], a record only MUSIC holds is credited again at the next build, MORE's untouched. F54's rule reads the
/// library's rules and adds no separator of its own. Never two libraries' rules mixed in one record's credit; never "&" a
/// separator. Red first: the record's artists are its first-ranked copy's album artists as its own library splits them, but the
/// pass credits them as the build's one set of rules reads them (not each copy's library).
#[test]
fn a_credit_splits_by_its_first_copys_library_rules() {
    let root = crate::retag::fixtures::dir("f54-6-library-rules");
    let (music, more) = (root.join("music"), root.join("more"));
    const REL: &str = "c0000000-0000-4000-8000-000000005460";
    let in_music = copy(&music.join("K&D"), REL);
    let in_more = copy(&more.join("K&D"), REL);
    let solo = tagged(
        &music.join("Solo"),
        "01 Solo",
        &[
            (lofty::tag::ItemKey::AlbumTitle, "Solo"),
            (lofty::tag::ItemKey::AlbumArtist, "Kruder; Dorfmeister"),
            (lofty::tag::ItemKey::TrackArtist, "Kruder; Dorfmeister"),
            (lofty::tag::ItemKey::TrackTitle, "Solo"),
            (lofty::tag::ItemKey::TrackNumber, "1"),
        ],
    );
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();
    let slash = Encoding { artist_separators: s(&["/"]), ..Encoding::default() };
    let both = Encoding { artist_separators: s(&["/", ";"]), ..Encoding::default() };
    let cat = Arc::new(Catalogue::open(None, vec![music.clone(), more.clone()]).unwrap());
    cat.walk_all();
    let mut all: Vec<PathBuf> = in_music.iter().chain(&in_more).cloned().collect();
    all.push(solo.clone());
    cat.reread(&all).unwrap();
    let rules = FolderRules::new(Rules::default(), BTreeMap::from([(0, slash.clone()), (1, both.clone())]));
    cat.set_folder_rules(rules.clone());
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let serve = Serve::open(Some(&data), Rules::default(), &|_| None).unwrap();
    serve.library.set_kinds(vec![(0, Kind::Main), (1, Kind::Main)]);
    serve.attach_catalogue(cat.clone());
    serve.set_rules(rules);
    cat.identify().unwrap();
    serve.refresh().unwrap();
    let (sessions, solo_album) = (serve.library.album_in(0, "K&D").unwrap().unwrap(), serve.library.album_in(0, "Solo").unwrap().unwrap());
    let (record, solo_record) = (crate::serve::ids::record_number(&sessions).unwrap(), crate::serve::ids::record_number(&solo_album).unwrap());
    assert_eq!(serve.library.album_in(1, "K&D").unwrap(), Some(sessions.clone()), "one record, two copies");
    assert_eq!(credited(&cat, record), ["Kruder; Dorfmeister"], "MUSIC's copy decides: one artist by MUSIC's rules");
    // MUSIC's copy goes: MORE's decides, by MORE's rules.
    for f in &in_music {
        std::fs::remove_file(f).unwrap();
    }
    cat.walk_all();
    cat.identify().unwrap();
    serve.refresh().unwrap();
    assert_eq!(credited(&cat, record), ["Kruder", "Dorfmeister"], "MORE's copy decides: two by MORE's rules");
    // MUSIC's rules changed: its own records are credited again at the next build; MORE's is as it was.
    let rules = FolderRules::new(Rules::default(), BTreeMap::from([(0, both.clone()), (1, both)]));
    cat.set_folder_rules(rules.clone());
    serve.set_rules(rules);
    cat.identify().unwrap();
    serve.refresh().unwrap();
    assert_eq!(credited(&cat, solo_record), ["Kruder", "Dorfmeister"], "MUSIC's own record, by its new rules");
    assert_eq!(credited(&cat, record), ["Kruder", "Dorfmeister"]);
    std::fs::remove_dir_all(&root).ok();
}
