// The moment's weather with Cratarr out of reach (decision 0101, point 18): Open-Meteo, for where the device roughly is, at most
// every half hour, and only where the switch in This device is on and the internet is there. Nothing is asked while Cratarr
// answers, as it has the weather itself, and the place goes to Open-Meteo alone, never to Cratarr. No weather: time and day only,
// and the moment says so.
import { getMoment, read, type Moment } from './api';
import { inApp, onApp, toApp } from './app.svelte';
import { network } from './away.svelte';
import { askFor, coarse, momentOf, weatherOf, type Weather } from './moment';
import { prefs } from './prefs.svelte';
import { reach } from './reach.svelte';

const KEY = 'cratarr-moment-weather';
const EVERY_MS = 30 * 60_000;
/** A device that cannot say where it is within this long has no place to give. */
const PLACE_MS = 10_000;

type Asked = { at: number; weather: Weather | null };

function lastAsked(): Asked | null {
  try {
    const a = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    return a && typeof a.at === 'number' ? a : null;
  } catch {
    return null;
  }
}

function keep(a: Asked) {
  try {
    localStorage.setItem(KEY, JSON.stringify(a));
  } catch {
    /* no store: asked again at the next open */
  }
}

type Place = { lat: number; lon: number };
let told: ((p: Place | null) => void) | null = null;
if (inApp)
  onApp('location', (m) => {
    told?.(typeof m.lat === 'number' && typeof m.lon === 'number' ? { lat: m.lat, lon: m.lon } : null);
    told = null;
  });

/** Where the device roughly is; null where it cannot say or may not. The app's own: a page on a plain address gets none from
 *  its WebView. */
function roughPlace(): Promise<Place | null> {
  return new Promise((done) => {
    const wait = setTimeout(() => done(null), PLACE_MS);
    const give = (p: Place | null) => {
      clearTimeout(wait);
      done(p);
    };
    if (inApp) {
      told = give;
      toApp({ type: 'location' });
    } else if (typeof navigator !== 'undefined' && navigator.geolocation) {
      navigator.geolocation.getCurrentPosition(
        (p) => give({ lat: p.coords.latitude, lon: p.coords.longitude }),
        () => give(null),
        { enableHighAccuracy: false, maximumAge: EVERY_MS, timeout: PLACE_MS },
      );
    } else give(null);
  });
}

/** The switch turned on, with a tap to ask on: the place's permission now, not while Cratarr is away with nobody looking. */
export function askForPlace() {
  if (inApp) toApp({ type: 'location', op: 'ask' });
  else void roughPlace();
}

const internetThere = () => (typeof navigator === 'undefined' || navigator.onLine !== false) && network.validated !== false;

let asking: Promise<Weather | null> | null = null;

async function ask(now: number): Promise<Weather | null> {
  const place = await roughPlace();
  // Switched off, or Cratarr back, while the device looked: nothing leaves it.
  if (!place || !prefs.momentWeather || !reach.away) return null;
  keep({ at: now, weather: null });
  try {
    const res = await fetch(askFor(coarse(place.lat), coarse(place.lon)));
    const weather = res.ok ? weatherOf(await res.json()) : null;
    if (weather) keep({ at: now, weather });
    return weather;
  } catch {
    return null;
  }
}

/** The weather here now, where it may be asked for; the last answer for half an hour, and in that time no new ask, failed or
 *  not. */
export async function weatherHere(now = Date.now()): Promise<Weather | null> {
  if (!prefs.momentWeather || !reach.away) return null;
  const last = lastAsked();
  if (last && now - last.at < EVERY_MS) return last.weather;
  if (!internetThere()) return null;
  asking ??= ask(now).finally(() => (asking = null));
  return asking;
}

/** The moment, from wherever reads come from: from the device, the albums Cratarr last gave (`last`, else its last copy), under the time
 *  and weather here, never a name from before. No albums to go on: the read goes where any other goes. */
export const momentNow = (source: string | undefined, without: string[] | undefined, last: string[] | undefined) =>
  read(
    () => getMoment(source, without),
    async () => {
      const albums = last ?? (await getMoment(source, without).catch(() => null))?.albums;
      return albums?.length ? momentHere(albums) : null;
    },
  );

/** The moment with Cratarr out of reach: `albums` as Cratarr last gave them, its name and sky worked out here. */
export async function momentHere(albums: string[], now = new Date()): Promise<Moment> {
  const weather = await weatherHere(now.getTime());
  return { ...momentOf(weather, now.getHours() + now.getMinutes() / 60), albums, timeOnly: !weather };
}
