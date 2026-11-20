//! Tests moved up from requests' files (the app split's phase 2): each runs the bulk queue (`app::track_intake`, incoming) too, so
//! they go up to incoming. By origin, their bodies as they were but for paths.

/// From `soulseek`'s tests (TN.5): its searcher and fetcher and the bulk queue's worker, while slskd is away.
#[cfg(test)]
mod soulseek {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use anyhow::Result;

    use crate::app::soulseek::tests::{answers_of, fetch_from, gate_through, landed, next_fetch, on_disk, own_done, own_list, ready, ticked, tide_list, tn_item};
    use crate::app::soulseek::{Item, ROUND_AT_LEAST, State, files_under, next_search, search_in};
    use crate::app::track_intake::tests as bulk;
    use crate::app::track_intake_base::testing::{Clock, FakeAcoustid, FakeSlskd, T0};
    use crate::app::turn::{Failure, Turn};
    use crate::catalogue::records::Ref;
    use crate::slskd::Slskd;

    /// A list read standing in for MusicBrainz's (`read_reference`): "Tide Tables"' list.
    fn listed_by_musicbrainz(_: Ref) -> std::future::Ready<Result<Option<crate::app::references::Read>>> {
        std::future::ready(Ok(Some((tide_list(), None, Default::default()))))
    }

    /// TN.5's world at t0, slskd at `url`: away until t0+120 (503 to all it is asked before then: slskd has no pause of its own in 0.17);
    /// ss-1 waiting with its list read (a miss before), due; ss-2 waiting with no list yet, its MusicBrainz release to read, due; ss-7 at
    /// its check with its files in slskd's folder, due, AcoustID to ask; the bulk queue's ti-1 queued.
    struct Tn5 {
        d: PathBuf,
        cat: Arc<crate::catalogue::Catalogue>,
        neap: i64,
        fake: Arc<FakeSlskd>,
        slskd: Slskd,
        heard: Arc<FakeAcoustid>,
        acoustid: Arc<Turn>,
        intake: bulk::Through,
    }

    async fn tn5(name: &str, url: &str) -> Tn5 {
        let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
        let neap = cat.record_for_tests("Neap Season", 0);
        assert!(cat.set_reference(&crate::serve::ids::record(neap), &Ref::new("musicbrainz", "release", "r-neap"), "link").unwrap());
        let items = vec![
            Item { misses: 1, ..tn_item("ss-1", 9101, "Tide Tables", State::Waiting) },
            Item { tracks: Vec::new(), ..tn_item("ss-2", neap, "Neap Season", State::Waiting) },
            Item { percent: 100.0, ..ticked(tn_item("ss-7", 9107, "Harbour Lights", State::Checking), &["peer-a"], Some("b7")) },
        ];
        let d = own_list(name, items);
        let tone = bulk::tone(&d);
        landed(&d.join("slskd"), "b7", "Harbour Lights", &tone);
        let fake = Arc::new(FakeSlskd {
            away_until: T0 + 120,
            found: HashMap::from([("Glass Harbour Tide Tables", answers_of("peer-a", "Tide Tables")), ("Glass Harbour Neap Season", answers_of("peer-b", "Neap Season"))]),
            folder: d.join("slskd"),
            file: tone,
            ..Default::default()
        });
        let slskd = bulk::slskd_at(url, &fake);
        let given = slskd.search("Glass Harbour").await.unwrap_err();
        assert!(matches!(crate::app::turn::class(&given), Failure::Away(_)), "the given: slskd away, its 503: {given:#}");
        let heard = Arc::new(FakeAcoustid::default());
        let intake = bulk::Through::new(&d, slskd.clone(), &heard);
        bulk::start_with(&d, vec![bulk::waiting("ti-1", &bulk::tide_tables()[0])]);
        Tn5 { d, cat, neap, fake, slskd, heard, acoustid: Arc::new(Turn::new(crate::app::turn::ACOUSTID)), intake }
    }

    /// The searcher, the fetcher and the bulk worker taking their work at `now` as their loops do (`next_search`, `next_fetch`, the bulk
    /// queue's next step), each until it has none: the items taken, in turn. Lists read through `read`.
    async fn rounds_at<R, F>(w: &Tn5, read: &R, now: i64) -> Vec<String>
    where
        R: Fn(Ref) -> F,
        F: std::future::Future<Output = Result<Option<crate::app::references::Read>>>,
    {
        let mut took = Vec::new();
        for _ in 0..5 {
            let Some(id) = next_search() else { break };
            search_in(&id, &w.cat, read, ready(&w.slskd)).await;
            took.push(id);
        }
        for _ in 0..5 {
            let Some(id) = next_fetch() else { break };
            fetch_from(&id, &w.slskd, &w.d.join("slskd"), gate_through(w.acoustid.clone(), w.heard.clone())).await;
            took.push(id);
        }
        for _ in 0..5 {
            let Some(id) = bulk::worked(&w.intake, now).await else { break };
            took.push(id);
        }
        took
    }

    /// TN.5: while slskd is away (until t0+120) the other sources' work goes on at once: MusicBrainz is asked for ss-2's release and its
    /// list kept (the item's and the catalogue's), AcoustID is asked for ss-7's three files, and ss-7 is placed. Never a loop asleep
    /// while that work waits: all of it done long before slskd is back, by tokio's clock too, slskd asked once for each and no more.
    #[tokio::test(start_paused = true)]
    async fn another_source_goes_on_while_one_is_paused() {
        let _one = crate::app::switches::TEST_STATE.lock().await;
        let (_pinned, _clock) = Clock::at_t0();
        let w = tn5("goes-on", "http://192.168.1.20:5052").await;
        let start = tokio::time::Instant::now();
        let asked = Mutex::new(Vec::new());
        let musicbrainz = |r: Ref| {
            asked.lock().unwrap().push((r.source.clone(), r.value.clone(), tokio::time::Instant::now() - start));
            listed_by_musicbrainz(r)
        };
        assert_eq!(rounds_at(&w, &musicbrainz, T0).await, ["ss-1", "ss-2", "ss-7", "ti-1"]);

        // MusicBrainz asked for ss-2's release at once, its list kept.
        let mb = asked.lock().unwrap().clone();
        assert_eq!(mb.iter().map(|(s, v, _)| (s.as_str(), v.as_str())).collect::<Vec<_>>(), [("musicbrainz", "r-neap")]);
        assert!(mb[0].2 < Duration::from_secs(1), "asked at once: {:?}", mb[0].2);
        let held = on_disk(&w.d);
        assert_eq!(held["ss-2"].tracks, tide_list(), "the item's list");
        let kept_lists = w.cat.reference_tracks(w.neap).unwrap();
        assert!(kept_lists.iter().any(|(s, l)| s == "musicbrainz" && l.len() == 3), "the catalogue's: {kept_lists:?}");
        // AcoustID asked for ss-7's three files, and ss-7 placed in its library.
        let acoustid = w.heard.asked.lock().unwrap().clone();
        assert_eq!(acoustid.len(), 3, "{acoustid:?}");
        assert!(acoustid.iter().all(|(at, when)| *at == T0 && *when - start < Duration::from_secs(5)), "{acoustid:?}");
        let ss7 = &held["ss-7"];
        assert_eq!((ss7.state, ss7.batch.as_deref(), ss7.failed.len()), (State::Placed, None, 0));
        let placed = files_under(Path::new(ss7.placed.as_deref().expect("placed")));
        assert_eq!(placed.iter().filter(|p| p.extension().is_some_and(|e| e == "flac")).count(), 3, "{placed:?}");
        // No loop asleep: the rounds done long before slskd is back; ss-1 and ss-2 held a round as away holds a search, slskd asked once
        // for each (the given, ss-1's and ss-2's searches, ss-7's transfers at its check, ti-1's search), each answered 503.
        assert!(tokio::time::Instant::now() - start < Duration::from_secs(10), "{:?}", tokio::time::Instant::now() - start);
        assert_eq!((held["ss-1"].due, held["ss-2"].due), (T0 + ROUND_AT_LEAST, T0 + ROUND_AT_LEAST));
        assert_eq!(w.fake.asked.lock().unwrap().len(), 5, "slskd asked once for each, away");
        assert!(w.fake.violations.lock().unwrap().is_empty(), "{:?}", w.fake.violations.lock().unwrap());
        assert!(w.heard.violations.lock().unwrap().is_empty(), "{:?}", w.heard.violations.lock().unwrap());
        bulk::done(&w.d);
        own_done(&w.d);
    }
}
