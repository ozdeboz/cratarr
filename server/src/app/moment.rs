//! Home's moment (the owner, 2026-10-23): what to play now, from the weather Home Assistant tells and the time of day,
//! named "Music for a Rainy Evening". Plain rules, not learned ("a lot more rudimentary"): the energy follows the time
//! of day (the sun's height, else the clock), calm at night and lively by day; the mood follows the weather, happy in
//! the sun, sad and relaxed in the rain; rain and great heat ("a lot more mellowness") calm it down. Home shows a few albums near it and a radio inside it
//! (`like:now`, `crates::Value::Moment`).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::crates::Target;

/// The weather as Home Assistant tells it (the cratarr-ha integration, on each change).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Weather {
    /// Its weather entity's state: "sunny", "clear-night", "partlycloudy", "cloudy", "fog", "rainy", "pouring",
    /// "lightning", "lightning-rainy", "snowy", "snowy-rainy", "hail", "windy", "windy-variant", "exceptional".
    #[serde(default)]
    pub condition: Option<String>,
    /// Cloud cover (%).
    #[serde(default)]
    pub cloud: Option<f64>,
    /// °C.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// The sun's height above the horizon (degrees).
    #[serde(default)]
    pub sun: Option<f64>,
    /// Where it is: seconds east of UTC (for the time of day's name).
    #[serde(default)]
    pub tz: Option<i64>,
    /// When it was told (epoch s).
    #[serde(default)]
    pub at: i64,
}

/// The weather last told, and the last clock offset a page gave (what `like:now` names its time of day by when Home
/// Assistant gave none).
static TOLD: Mutex<Option<Weather>> = Mutex::new(None);
static PAGE_TZ: Mutex<Option<i64>> = Mutex::new(None);

/// How long what Home Assistant told holds when it tells nothing new (it tells each change, and every half hour).
const STALE: i64 = 3 * 3600;

pub fn tell(mut w: Weather, now: i64) {
    w.at = now;
    *TOLD.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
}

/// The weather last told, however long ago (Connections shows when Home Assistant last told it).
pub fn last_told() -> Option<Weather> {
    TOLD.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The weather, while it holds.
pub fn told(now: i64) -> Option<Weather> {
    TOLD.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|w| now - w.at < STALE)
}

/// A page's clock offset (seconds east of UTC), kept for `like:now`.
pub fn page_tz(tz: i64) {
    *PAGE_TZ.lock().unwrap_or_else(|e| e.into_inner()) = Some(tz.clamp(-14 * 3600, 14 * 3600));
}

/// Where it is: Home Assistant's offset, else a page's, else UTC.
pub fn tz(w: Option<&Weather>) -> i64 {
    w.and_then(|w| w.tz).or_else(|| *PAGE_TZ.lock().unwrap_or_else(|e| e.into_inner())).unwrap_or(0)
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The moment now, with the weather while it holds.
pub fn now() -> Moment {
    let at = now_secs();
    let w = told(at);
    moment(w.as_ref(), at, tz(w.as_ref()))
}

/// A moment: its name ("Music for a Rainy Evening") and what it asks of the music.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Moment {
    pub name: String,
    pub target: Target,
    /// The sky Home paints behind it: "sunny", "clear", "fewclouds", "cloudy", "foggy", "rainy", "stormy", "snowy",
    /// "windy", "hot", or "plain" without the weather; `dark` after sunset (by the clock without the sun's height).
    pub sky: &'static str,
    pub dark: bool,
}

/// How calm the night is and how lively midday (on the library's energy scale).
const NIGHT: f64 = 0.15;
const MIDDAY: f64 = 0.85;
/// The sun's height where the day's energy starts rising, and where it is full (a winter noon here, ~15°, is not).
const SUN_LOW: f64 = -12.0;
const SUN_HIGH: f64 = 30.0;
/// From this heat on (°C), each degree calms it this much, at most `HEAT_MOST`; and makes it this much more relaxed.
const HEAT_FROM: f64 = 26.0;
const HEAT_CALM: f64 = 0.04;
const HEAT_MOST: f64 = 0.35;
const HEAT_RELAXED: f64 = 0.08;
/// From this heat on (°C) the moment is named for it.
const HOT: f64 = 28.0;
/// What rain or snow takes off the energy.
const WET_CALM: f64 = 0.1;

fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How gloomy a condition is (0 bright to 1 dark and wet), and whether it is wet; none for one not known.
fn gloom_of(condition: &str) -> Option<(f64, bool)> {
    Some(match condition {
        "sunny" | "clear-night" => (0.0, false),
        "partlycloudy" => (0.35, false),
        "windy" | "windy-variant" => (0.4, false),
        "cloudy" => (0.6, false),
        "fog" => (0.75, false),
        "snowy" => (0.75, true),
        "rainy" | "snowy-rainy" => (0.85, true),
        "hail" | "lightning" => (0.9, true),
        "pouring" | "lightning-rainy" => (1.0, true),
        _ => return None,
    })
}

/// The weather's word in its name ("Music for a Rainy Evening"); great heat and a few clouds are named on their own.
fn word_of(condition: &str, night: bool) -> Option<&'static str> {
    Some(match condition {
        "sunny" if night => "Clear",
        "sunny" => "Sunny",
        "clear-night" => "Clear",
        "cloudy" => "Cloudy",
        "fog" => "Foggy",
        "rainy" | "pouring" => "Rainy",
        "lightning" | "lightning-rainy" | "hail" => "Stormy",
        "snowy" | "snowy-rainy" => "Snowy",
        "windy" | "windy-variant" => "Windy",
        _ => return None,
    })
}

/// The moment at `now` (epoch s), `tz` seconds east of UTC, with the weather when Home Assistant told it.
pub fn moment(w: Option<&Weather>, now: i64, tz: i64) -> Moment {
    let hour = (now + tz).rem_euclid(86_400) as f64 / 3600.0;
    // The day's energy: the sun's height when told, else the clock (calmest at 2, liveliest at 14).
    let day = match w.and_then(|w| w.sun) {
        Some(sun) => smoothstep((sun - SUN_LOW) / (SUN_HIGH - SUN_LOW)),
        None => (1.0 + (std::f64::consts::TAU * (hour - 14.0) / 24.0).cos()) / 2.0,
    };
    let mut energy = NIGHT + (MIDDAY - NIGHT) * day;
    let condition = w.and_then(|w| w.condition.as_deref()).unwrap_or("");
    let heat = w.and_then(|w| w.temperature).map_or(0.0, |t| (t - HEAT_FROM).max(0.0));
    energy -= (heat * HEAT_CALM).min(HEAT_MOST);
    // The mood: the condition's gloom, a dry one's by its cloud cover when told.
    let mood = gloom_of(condition).map(|(gloom, wet)| {
        let gloom = match w.and_then(|w| w.cloud) {
            Some(cloud) if !wet && condition != "fog" => 0.7 * (cloud / 100.0).clamp(0.0, 1.0),
            _ => gloom,
        };
        if wet {
            energy -= WET_CALM;
        }
        let dark = 2.0 * gloom - 1.0;
        [0.8 * -dark, 0.7 * dark, (0.6 * dark.max(0.0) + heat * HEAT_RELAXED).min(1.0)]
    });
    // Its name (the owner, 2026-10-23): "Music for a Sunny Morning", "Music for Hot Weather", "Music for a Few Clouds";
    // without the weather, "Music for the Evening".
    let part = match hour {
        h if (5.0..12.0).contains(&h) => "Morning",
        h if (12.0..17.0).contains(&h) => "Afternoon",
        h if (17.0..23.0).contains(&h) => "Evening",
        _ => "Night",
    };
    let wet = gloom_of(condition).is_some_and(|g| g.1);
    let hot = !wet && w.and_then(|w| w.temperature).is_some_and(|t| t >= HOT);
    let name = match word_of(condition, part == "Night") {
        _ if hot => "Music for Hot Weather".to_string(),
        _ if condition == "partlycloudy" => "Music for a Few Clouds".to_string(),
        Some(word) => format!("Music for a {word} {part}"),
        None => format!("Music for the {part}"),
    };
    let dark = w.and_then(|w| w.sun).map_or(!(7.0..19.0).contains(&hour), |sun| sun < 0.0);
    let sky = match condition {
        _ if hot => "hot",
        "sunny" if dark => "clear",
        "sunny" => "sunny",
        "clear-night" => "clear",
        "partlycloudy" => "fewclouds",
        "cloudy" => "cloudy",
        "fog" => "foggy",
        "rainy" | "pouring" => "rainy",
        "lightning" | "lightning-rainy" | "hail" => "stormy",
        "snowy" | "snowy-rainy" => "snowy",
        "windy" | "windy-variant" => "windy",
        _ => "plain",
    };
    Moment { name, target: Target { energy: energy.clamp(0.0, 1.0), mood }, sky, dark }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-23 at `h` o'clock UTC.
    fn at(h: i64) -> i64 {
        1_792_713_600 + h * 3600
    }

    fn weather(condition: &str, cloud: Option<f64>, temperature: f64, sun: f64) -> Weather {
        Weather { condition: Some(condition.into()), cloud, temperature: Some(temperature), sun: Some(sun), tz: Some(7200), at: 0 }
    }

    #[test]
    fn energy_follows_the_sun_and_the_clock_without_it() {
        let night = moment(Some(&weather("clear-night", Some(0.0), 10.0, -40.0)), at(22), 7200);
        let noon = moment(Some(&weather("sunny", Some(0.0), 20.0, 55.0)), at(11), 7200);
        assert!(night.target.energy < 0.2 && noon.target.energy > 0.8, "{night:?} {noon:?}");
        let (two, fourteen) = (moment(None, at(0), 7200), moment(None, at(12), 7200));
        assert!(two.target.energy < 0.2 && fourteen.target.energy > 0.8);
        assert_eq!(two.target.mood, None, "no weather, no mood");
        assert_eq!((two.name.as_str(), fourteen.name.as_str()), ("Music for the Night", "Music for the Afternoon"));
    }

    #[test]
    fn the_sun_makes_it_happy_the_rain_sad_and_relaxed() {
        let sunny = moment(Some(&weather("sunny", Some(5.0), 18.0, 30.0)), at(13), 7200);
        let rainy = moment(Some(&weather("pouring", Some(100.0), 11.0, 30.0)), at(13), 7200);
        let [h, s, r] = sunny.target.mood.unwrap();
        assert!(h > 0.6 && s < -0.5 && r.abs() < 1e-9, "{sunny:?}");
        let [h, s, r] = rainy.target.mood.unwrap();
        assert!(h < -0.6 && s > 0.5 && r > 0.5, "{rainy:?}");
        assert!(rainy.target.energy < sunny.target.energy, "rain calms it");
        assert_eq!((sunny.name.as_str(), rainy.name.as_str()), ("Music for a Sunny Afternoon", "Music for a Rainy Afternoon"));
        assert_eq!((sunny.sky, sunny.dark, rainy.sky), ("sunny", false, "rainy"));
    }

    #[test]
    fn great_heat_calms_it_and_relaxes_it() {
        let warm = moment(Some(&weather("sunny", Some(0.0), 24.0, 50.0)), at(13), 7200);
        let hot = moment(Some(&weather("sunny", Some(0.0), 35.0, 50.0)), at(13), 7200);
        assert!(hot.target.energy < warm.target.energy - 0.3, "{warm:?} {hot:?}");
        assert!(hot.target.mood.unwrap()[2] > 0.6);
        assert_eq!(hot.name, "Music for Hot Weather");
        let night = moment(Some(&weather("clear-night", Some(0.0), 12.0, -30.0)), at(22), 7200);
        assert_eq!((night.name.as_str(), night.sky, night.dark), ("Music for a Clear Night", "clear", true));
        assert_eq!(moment(Some(&weather("partlycloudy", Some(40.0), 17.0, 20.0)), at(9), 7200).name, "Music for a Few Clouds");
    }

    #[test]
    fn what_home_assistant_told_holds_a_while() {
        tell(weather("rainy", None, 11.0, -27.0), 1000);
        assert!(told(1000 + STALE - 1).is_some());
        assert!(told(1000 + STALE).is_none());
    }
}
