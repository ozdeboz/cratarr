//! Classical's stamp, below the app (moved from `app::classical`, which keeps its name): the served library's view hands it the
//! catalogue (`view::ServeView::classical_stamp`).

/// What the index is made from, cheaply: the crates' facts read again when it moves (their Type axis has Classical).
pub fn stamp(serve: &crate::serve::Serve) -> i64 {
    let marks = serve.userdata.marks_stamp().unwrap_or(0);
    let facts = serve.userdata.works_stamp().unwrap_or(0);
    marks.wrapping_mul(1_000_003).wrapping_add(facts)
}
