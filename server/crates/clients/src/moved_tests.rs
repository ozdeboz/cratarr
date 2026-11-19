//! Tests moved up into this crate from the crates below, by their module (SPLIT-PLAN 3.2): each needs this crate.

/// From `crate::app::reach`'s tests.
#[cfg(test)]
mod reach {
    use crate::app::reach::*;
    use crate::app::reach::testing::*;

    /// O1: a service an administrator set up (slskd here) never follows a redirect: its key stays on the host given.
    #[tokio::test]
    async fn an_admin_service_never_follows_a_redirect() {
        use axum::{response::Redirect, routing::get};
        let done = serde_json::json!({ "transfers": [{ "id": "t1", "filename": "x", "state": "Completed, Succeeded", "percentComplete": 100.0 }] });
        let app = axum::Router::new()
            .route("/api/v0/transfers/downloads/batches/{id}", get(|| async { Redirect::temporary("/elsewhere") }))
            .route("/elsewhere", get(move || std::future::ready(axum::Json(done.clone()))));
        let at = fake(app).await;
        let slskd = crate::slskd::Slskd::new(&format!("http://127.0.0.1:{at}"), "key").unwrap();
        assert!(slskd.transfers("b1").await.is_err(), "the redirect not followed, its answer not read");
        let r = lan_client().build().unwrap().get(format!("http://127.0.0.1:{at}/api/v0/transfers/downloads/batches/b1")).send().await.unwrap();
        assert!(r.status().is_redirection());
    }
}
