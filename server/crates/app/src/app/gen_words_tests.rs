//! GEN's tests on the whole app (SCENARIOS-GEN, GEN steps 2–5): Bandcamp's tags kept from every read of a record's own Bandcamp
//! page, never from a candidate page, never fetched for themselves (GEN.60); the services' and the typed words split by their
//! own rules on their paths (GEN.7's two paths that GEN C1 left: GEN.4, GEN.25); one link function (GEN.22).

use std::path::Path;

use super::App;
use crate::catalogue::records::Ref;

const QUAY: &str = "https://quaylights.bandcamp.com/album/quay-lights";
const CANDIDATE: &str = "https://ferryman.bandcamp.com/album/night-ferry";

/// A fake Bandcamp's album page as its schema.org description lists its keywords, read by the client's own parser and kept as if
/// just read: every read below is served it, and none asks Bandcamp.
fn page(url: &str, keywords: &[&str]) -> crate::bandcamp::Album {
    let data = r#"{"artist":"Quay","current":{"title":"Quay Lights"},"trackinfo":[{"track_num":1,"title":"One","duration":100.0}]}"#;
    let esc = data.replace('"', "&quot;");
    let ld = serde_json::json!({ "@type": "MusicAlbum", "keywords": keywords }).to_string();
    let html = format!(r#"<script type="application/ld+json">{ld}</script><script data-tralbum="{esc}"></script>"#);
    crate::bandcamp::parse_album(&html, url).unwrap()
}

/// Every row Bandcamp has in the stored words: (subject, word, kind, weight).
fn bandcamp_rows(c: &super::every_copy::Copies) -> Vec<(String, String, String, f64)> {
    let mut rows: Vec<_> = c.serve.userdata.all_words().unwrap().into_iter().filter(|r| r.1 == "bandcamp").map(|(s, _, w, k, x)| (s, w, k, x)).collect();
    rows.sort_by(|a, b| a.1.cmp(&b.1));
    rows
}

/// GEN.60 (OQ-GEN-25): each of Cratarr's own reads of a record's own Bandcamp page keeps its tags as that record's words (source
/// "bandcamp", kind tag, 1.0 each, as the page lists them); a candidate page keeps nothing for any record; nothing is fetched for
/// the tags; with Bandcamp switched off nothing new is kept.
#[tokio::test(flavor = "multi_thread")]
async fn bandcamps_tags_are_kept_from_every_read_of_the_records_own_page() {
    let state = crate::app::references::TestState::set().await;
    let c = super::every_copy::two_copies("bandcamp-tags", false);
    let quay = c.album.clone();
    assert!(crate::serve::ids::record_number(&quay).is_some(), "{quay}: a record of the catalogue");
    c.serve.catalogue().set_reference(&quay, &Ref::new("bandcamp", "album", QUAY), "hand").unwrap();
    crate::bandcamp::Bandcamp::get().keep(page(QUAY, &["electronic", "Electronic", "downtempo", " Paris "]));
    crate::bandcamp::Bandcamp::get().keep(page(CANDIDATE, &["ambient", "Oslo"]));
    let d = crate::retag::fixtures::dir("gen-words-app");
    std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
    let cfg = crate::config::Config::from_lookup(|_| None).unwrap();
    let saved = crate::settings::Saved { music_folder: Some(d.to_string_lossy().into_owned()), ..Default::default() };
    let app = App::new(cfg, saved, crate::settings::Origin::Env, Some(c.serve.clone())).await.unwrap();
    let subject = format!("album:{quay}");
    let want: Vec<(String, String, String, f64)> = ["Paris", "downtempo", "electronic"].iter().map(|w| (subject.clone(), w.to_string(), "tag".to_string(), 1.0)).collect();
    let forget = || c.serve.userdata.set_words(&subject, "bandcamp", &[], true, 1).unwrap();
    // (1) The references' build.
    crate::app::references::of(&app, &quay).await.unwrap();
    assert_eq!(bandcamp_rows(&c), want, "the references' build");
    forget();
    // (1) The list reading, through the reader the record's page and Soulseek read with.
    let read = crate::app::references::read_reference_kept(Some(c.serve.clone()));
    read(Ref::new("bandcamp", "album", QUAY)).await.unwrap().expect("its list");
    assert_eq!(bandcamp_rows(&c), want, "the list reading");
    // Read again: nothing written again (the store isn't built again for it).
    let stamp = c.serve.userdata.words_stamp().unwrap();
    read(Ref::new("bandcamp", "album", QUAY)).await.unwrap();
    assert_eq!(c.serve.userdata.words_stamp().unwrap(), stamp, "kept already: not written again");
    forget();
    // (2) The panel's and a preview's reads keep through the same keeper, by the page they read.
    let a = crate::bandcamp::Bandcamp::get().album(QUAY).await.unwrap();
    assert!(crate::app::words::keep_bandcamp(&c.serve, &a.url, &a.tags).unwrap());
    assert_eq!(bandcamp_rows(&c), want, "the panel's and a preview's read");
    forget();
    // (3) A candidate page (a request's listing, a search): read, and kept for no record.
    let a = crate::bandcamp::Bandcamp::get().album(CANDIDATE).await.unwrap();
    assert!(!crate::app::words::keep_bandcamp(&c.serve, &a.url, &a.tags).unwrap());
    assert_eq!(bandcamp_rows(&c), [], "a candidate page keeps nothing");
    // Bandcamp switched off: the reads make no request and keep nothing new.
    crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, false);
    crate::app::references::of(&app, &quay).await.unwrap();
    assert!(read(Ref::new("bandcamp", "album", QUAY)).await.is_err(), "switched off");
    assert!(!crate::app::words::keep_bandcamp(&c.serve, QUAY, &["electronic".to_string()]).unwrap());
    assert_eq!(bandcamp_rows(&c), [], "Bandcamp off: nothing new");
    // Never a page fetched for its tags: every read above was served the kept pages.
    let asked = crate::bandcamp::pages_asked_for_tests();
    assert!(!asked.iter().any(|p| p == QUAY || p == CANDIDATE), "{asked:?}");
    drop(state);
    std::fs::remove_dir_all(&d).ok();
}

/// GEN.60 (4) and its Never, on the code: the harvest asks Bandcamp nothing; the paths that read a record's own page keep its tags
/// through the one keeper; no candidate's read does. (The panel's and a preview's routes in-process wait for TS item 5's harness.)
#[test]
fn only_a_records_own_bandcamp_page_keeps_its_tags() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |file: &str| std::fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    assert!(!read("incoming/src/app/harvest.rs").to_lowercase().contains("bandcamp"), "the harvest asks Bandcamp nothing");
    for (file, keeps) in [
        ("records/src/app/references.rs", 2),
        ("api/src/api.rs", 2),
        ("base/src/app/describe.rs", 1),
        ("playing/src/app/playing.rs", 1),
        ("requests/src/app/soulseek.rs", 1),
    ] {
        let text = read(file);
        let n = text.matches("words::keep_bandcamp(").count() + text.matches("words::keep_bandcamp_read(").count() + text.matches("references::read_reference_kept(").count();
        assert!(n >= keeps, "{file}: {n} of {keeps} reads keep the record's own page's tags");
    }
    for file in ["requests/src/app/requests.rs", "records/src/app/discover.rs", "playing/src/app/radio.rs", "playing/src/app/playon.rs", "records/src/app/related.rs"] {
        assert!(!read(file).contains("keep_bandcamp"), "{file}: a candidate's or another page's read keeps nothing");
    }
}

/// GEN.22 (kind 6): one link function gives a kind's genres from its genre words and its style words, each style linked to
/// exactly one genre giving it, a blend none (GEN.64), and the record's genres and every track's come from it; the links are read
/// nowhere else.
#[test]
fn one_link_function() {
    let code = super::tag_rules_tests::sources();
    const STORE: &str = "base/src/app/words/store.rs";
    let store = &code.iter().find(|(f, _)| f == STORE).unwrap_or_else(|| panic!("{STORE}")).1;
    assert_eq!(store.matches("fn with_links(").count(), 1, "one link function");
    assert!(store.contains("fn with_links(lib: &Library, genres: KindWords, styles: &[Word]) -> KindWords"), "a kind's genres from its genre words and its style words, a blend giving none (GEN.64)");
    assert_eq!(store.matches(".genres_of(").count(), 1, "the links read in it alone");
    assert_eq!(store.matches("with_links(lib, ").count(), 2, "its two callers: the record's genres and each track's");
    for (file, text) in &code {
        if file != STORE {
            assert!(!text.contains(".genres_of(") && !text.contains("shipped().links()"), "{file} reads the links itself");
        }
    }
    // The rules crate looks a style's link up by its key, once.
    let genres = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/src/genres.rs")).unwrap();
    assert_eq!(genres.matches("pub fn genres_of(&self, style: &str) -> &[String]").count(), 1);
}
