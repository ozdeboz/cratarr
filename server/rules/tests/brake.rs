//! The brake against the players' one table (shared/rules.json), as the app's and the page's tests check theirs.

use cratarr_rules::brake;

#[test]
fn the_brake_is_the_tables() {
    let r: serde_json::Value = serde_json::from_str(include_str!("../../../shared/rules.json")).unwrap();
    assert_eq!(r["decel_x"].as_f64(), Some(brake::DECEL_X));
    for (i, b) in r["brake"].as_array().unwrap().iter().enumerate() {
        let (v0, to) = (b["v0"].as_f64().unwrap(), b["to"].as_f64().unwrap());
        let length = brake::length_ms(v0, to, b["brake_ms"].as_f64().unwrap());
        assert_eq!(length, b["length_ms"].as_f64().unwrap(), "brake {i}: length");
        for p in b["at"].as_array().unwrap() {
            let t = p["t_ms"].as_f64().unwrap();
            assert!((brake::v(v0, to, length, t) - p["v"].as_f64().unwrap()).abs() < 1e-9, "brake {i} at {t}: speed");
            assert!((brake::way(v0, to, length, t) - p["way_ms"].as_f64().unwrap()).abs() < 1e-6, "brake {i} at {t}: way");
        }
    }
}
