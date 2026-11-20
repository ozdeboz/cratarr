//! The words' stamp, below the app (moved from `app::words`, which keeps its name): the served library's view hands it the
//! catalogue (`view::ServeView::words_stamp`).

/// What the merge is made from, cheaply: when the library was built, and the words and the album tags as kept.
pub fn stamp(serve: &crate::serve::Serve) -> i64 {
    let words = serve.userdata.words_stamp().unwrap_or(0);
    serve.built_at().wrapping_mul(1_000_003).wrapping_add(words)
}
