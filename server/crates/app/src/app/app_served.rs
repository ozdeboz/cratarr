//! What the crates' facts take from the app (`serve::services::FactsFrom`): Classical, the words and Discogs' labels. The
//! server's view hands them to the catalogue (`view::ServeView`); the app sets them on the server as it is made (`App::new`).

use std::collections::HashMap;

use anyhow::Result;

use crate::serve::Serve;
use crate::serve::services::FactsFrom;

/// The app's part of the crates' facts.
pub struct AppFacts;

impl FactsFrom for AppFacts {
    fn add_classical(&self, serve: &Serve, facts: &mut [crate::crates::Facts]) -> Result<()> {
        super::classical::add_kind(serve, facts)
    }

    fn add_words(&self, serve: &Serve, album_of: &HashMap<String, String>, facts: &mut [crate::crates::Facts]) -> Result<()> {
        super::words::build(serve, album_of, facts)
    }

    fn kept_labels(&self, serve: &Serve) -> Result<HashMap<String, Vec<String>>> {
        super::discogs_facts::kept_labels(serve)
    }
}
