//! A crate's records kept apart (`spread`, 0.12.2) against the worked cases the page checks its own copy against
//! (tests/vectors/spread.json; 0.13, one spread).

#[test]
fn the_spread_is_the_cases() {
    let cases: serde_json::Value = serde_json::from_str(include_str!("vectors/spread.json")).unwrap();
    for c in cases.as_array().unwrap() {
        let records: Vec<&str> = c["records"].as_array().unwrap().iter().map(|r| r.as_str().unwrap()).collect();
        let want: Vec<usize> = c["order"].as_array().unwrap().iter().map(|i| i.as_u64().unwrap() as usize).collect();
        let gap = c["gap"].as_u64().unwrap() as usize;
        assert_eq!(cratarr_rules::spread(&records, gap), want, "{}", c["about"]);
    }
}
