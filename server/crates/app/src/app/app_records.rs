//! The app as records' functions read it (`records_host::RecordsHost`).

use std::sync::Arc;

use super::App;
use super::records_host::RecordsHost;

impl RecordsHost for App {
    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn served(&self) -> &Option<Arc<crate::serve::Serve>> {
        &self.served
    }

    fn collection(&self) -> &Arc<super::CollectionService> {
        &self.collection
    }

    fn artists(&self) -> &Option<Arc<super::artists::ArtistInfo>> {
        &self.artists
    }

    fn lidarr_config(&self) -> Option<crate::settings::LidarrConfig> {
        self.incoming.lidarr_state().0
    }

    fn lidarr(&self) -> Option<crate::lidarr::Lidarr> {
        self.incoming.lidarr_state().1
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    /// LOOKUPS Found 10 (RI.10, LK.A1): a link pasted on a record (`PUT /api/albums/{s}/{a}/references`) begins the record's read
    /// as its open does, a Cratarr job of `record:read` keyed by the record, and answers at once from what is kept, with the read's
    /// task; the job keeps the list it reads. Red first: `choose` began no read and answered no task (the read waited for the next
    /// open).
    #[tokio::test]
    async fn a_link_pasted_begins_the_records_read() {
        // The process's one runner keys a record's read by its id, and another test's catalogue numbers its records from 1 too
        // (GEN's `bandcamp_words_are_kept_by_the_records_job_only`): one at a time, or one follows the other's read and fails.
        let _state = crate::app::references::TestState::set().await;
        let serve =crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        let cat = serve.catalogue().clone();
        let record = cat.record_for_tests("Answered", 0);
        let album = crate::serve::ids::record(record);
        let d = std::env::temp_dir().join(format!("cratarr-choose-read-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let cfg = crate::config::Config::from_lookup(|_| None).unwrap();
        let saved = crate::settings::Saved { music_folder: Some(d.to_string_lossy().into_owned()), ..Default::default() };
        let app = crate::app::App::new(cfg, saved, crate::settings::Origin::Env, Some(serve.clone())).await.unwrap();
        let (gate, asked) = (Arc::new(tokio::sync::Semaphore::new(0)), Arc::new(AtomicUsize::new(0)));
        let read = {
            let (gate, asked) = (gate.clone(), asked.clone());
            move |_: crate::catalogue::records::Ref| {
                let (gate, asked) = (gate.clone(), asked.clone());
                async move {
                    let _open = gate.acquire().await.unwrap();
                    asked.fetch_add(1, SeqCst);
                    let list = vec![crate::catalogue::identity::RefTrack { disc: 1, position: 1, title: "Lamplight".into(), length_ms: Some(200_000), recording: None }];
                    Ok::<_, anyhow::Error>(Some((list, None, crate::app::references::ReadMore::default())))
                }
            }
        };
        let link = crate::app::references::Choose { link: Some("https://musicbrainz.org/release/00000000-0000-4000-8000-000000000377".into()), ..Default::default() };
        let answer = crate::app::references::choose_reading(&app, &album, link, read).await.unwrap();
        assert_eq!(answer.task, Some(serde_json::json!({ "kind": "record:read", "key": album })), "answered at once, with the read's task");
        assert_eq!(asked.load(SeqCst), 0, "nothing read before the answer");
        assert!(matches!(crate::runner::runner().begin_keyed("record:read", &album, None), Err(Ok(_))), "the record's read runs as a job");
        gate.add_permits(8);
        for _ in 0..500 {
            if !cat.lists_kept(record).unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(cat.lists_kept(record).unwrap().iter().any(|l| l.reference.source == "musicbrainz"), "the job read it and kept its list");
        std::fs::remove_dir_all(&d).ok();
    }
}
