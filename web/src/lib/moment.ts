// The moment worked out on the device (decision 0101, point 18), with Cratarr out of reach: its name and its sky as Cratarr's
// `app::moment` has them, from the time of day and, where there is one, the weather. What it asks of the music stays Cratarr's
// until the crates run here. Open-Meteo says the weather by its WMO codes: read here as Home Assistant's words, which is what
// the rule goes by. Pure.

/** The weather as `app::moment` reads it; `day`: the sun is up, where that is told. */
export type Weather = { condition: string | null; cloud?: number | null; temperature?: number | null; day?: boolean | null };
export type MomentShown = { name: string; sky: string; dark: boolean };

const HOT = 28;
const WET = new Set(['snowy', 'rainy', 'snowy-rainy', 'hail', 'lightning', 'pouring', 'lightning-rainy']);

/** The weather's word in its name ("Music for a Rainy Evening"). */
function wordOf(condition: string, night: boolean): string | null {
  switch (condition) {
    case 'sunny':
      return night ? 'Clear' : 'Sunny';
    case 'clear-night':
      return 'Clear';
    case 'cloudy':
      return 'Cloudy';
    case 'fog':
      return 'Foggy';
    case 'rainy':
    case 'pouring':
      return 'Rainy';
    case 'lightning':
    case 'lightning-rainy':
    case 'hail':
      return 'Stormy';
    case 'snowy':
    case 'snowy-rainy':
      return 'Snowy';
    case 'windy':
    case 'windy-variant':
      return 'Windy';
    default:
      return null;
  }
}

function skyOf(condition: string, hot: boolean, dark: boolean): string {
  if (hot) return 'hot';
  switch (condition) {
    case 'sunny':
      return dark ? 'clear' : 'sunny';
    case 'clear-night':
      return 'clear';
    case 'partlycloudy':
      return 'fewclouds';
    case 'cloudy':
      return 'cloudy';
    case 'fog':
      return 'foggy';
    case 'rainy':
    case 'pouring':
      return 'rainy';
    case 'lightning':
    case 'lightning-rainy':
    case 'hail':
      return 'stormy';
    case 'snowy':
    case 'snowy-rainy':
      return 'snowy';
    case 'windy':
    case 'windy-variant':
      return 'windy';
    default:
      return 'plain';
  }
}

/** The moment at `hour` (0 to 24, this device's clock), with the weather where there is one. */
export function momentOf(w: Weather | null, hour: number): MomentShown {
  const condition = w?.condition ?? '';
  const part = hour >= 5 && hour < 12 ? 'Morning' : hour >= 12 && hour < 17 ? 'Afternoon' : hour >= 17 && hour < 23 ? 'Evening' : 'Night';
  const hot = !WET.has(condition) && (w?.temperature ?? -Infinity) >= HOT;
  const word = wordOf(condition, part === 'Night');
  const name = hot ? 'Music for Hot Weather' : condition === 'partlycloudy' ? 'Music for a Few Clouds' : word ? `Music for a ${word} ${part}` : `Music for the ${part}`;
  const dark = typeof w?.day === 'boolean' ? !w.day : !(hour >= 7 && hour < 19);
  return { name, sky: skyOf(condition, hot, dark), dark };
}

/** A WMO weather code as Home Assistant's word (`day`: the sun is up); null for one not known. */
export function conditionOf(code: number, day: boolean): string | null {
  if (code === 0 || code === 1) return day ? 'sunny' : 'clear-night';
  if (code === 2) return 'partlycloudy';
  if (code === 3) return 'cloudy';
  if (code === 45 || code === 48) return 'fog';
  if (code === 65 || code === 82) return 'pouring';
  if (code === 66 || code === 67) return 'snowy-rainy';
  if ((code >= 51 && code <= 57) || (code >= 61 && code <= 63) || code === 80 || code === 81) return 'rainy';
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return 'snowy';
  if (code === 95) return 'lightning-rainy';
  if (code === 96 || code === 99) return 'hail';
  return null;
}

/** Open-Meteo's `current`, as the rule reads it; null for an answer that is none. */
export function weatherOf(answer: unknown): Weather | null {
  const c = (answer as { current?: Record<string, unknown> } | null)?.current;
  if (!c || typeof c.weather_code !== 'number') return null;
  const day = c.is_day === 1;
  return {
    condition: conditionOf(c.weather_code, day),
    cloud: typeof c.cloud_cover === 'number' ? c.cloud_cover : null,
    temperature: typeof c.temperature_2m === 'number' ? c.temperature_2m : null,
    day,
  };
}

/** A place in degrees as far as it goes out: one decimal, some 10 km, the size of a weather model's square. */
export const coarse = (degrees: number) => Math.round(degrees * 10) / 10;

/** Open-Meteo's address for a coarse place. Free, with no key, and nothing of Cratarr's or the account's in it. */
export const askFor = (lat: number, lon: number) =>
  `https://api.open-meteo.com/v1/forecast?latitude=${coarse(lat)}&longitude=${coarse(lon)}&current=weather_code,cloud_cover,temperature_2m,is_day`;
