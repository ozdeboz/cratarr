// What is known about a track (the owner, 2026-10-19), for the cover's overview (lib/CoverOverview: every fact, in
// full) and the player's row beside its info button (the facts the user picks, Settings → Display → Player, Extra details, each
// in its short form: "for long content options, we'd need short format versions"). Read from the track's file (its
// details: the tags as the server names them, its tempo, genres, quality and when it came in) and what the player says
// (its plays); mood, energy and danceability from AudioMuse's analysis once the server brings them in. Only what is
// known shows.

import { trackFile, type TrackFile, type TrackInfo } from './api';

/** AudioMuse's (imported by the server: its style tags, energy and key), and the listener's plays, where the server
 *  has them (70); danceability once AudioMuse gives it. */
export type Felt = {
  style?: { name: string; score: number }[] | null;
  /** The moods that stand out (CLAP: happy, sad, relaxed, aggressive, party), strongest first. */
  mood?: { name: string; score: number }[] | null;
  energy?: number | null;
  /** AudioMuse's key ("D minor"), where the file has no key tag. */
  key?: string | null;
  danceability?: number | null;
  play_count?: number | null;
  last_played?: number | null;
};
export type Details = TrackFile & Felt;

export { FACTS, type FactId } from './facts';
import { wordShown, type FactId } from './facts';
export type Fact = { full: string; short: string; meter?: number };

/** Each track's details as last read (null: none to read); read once a track is asked about. */
const read = $state<Record<string, Details | null>>({});
const asked = new Set<string>();
export function detailsOf(source: string, id: string): Details | null {
  const key = `${source}\n${id}`;
  if (!asked.has(key) && source !== 'device') {
    asked.add(key);
    trackFile(source, id).then(
      (f) => (read[key] = f as Details),
      () => (read[key] = null),
    );
  }
  return read[key] ?? null;
}

/** The server's details with what the device read of the stream itself where they leave it out (a Bandcamp
 *  preview's: its sample rate and channels from its frames, lib/waveforms). */
export function withHeard<D extends { sample_rate_hz: number | null; channels: number | null; bitrate_kbps: number | null }>(
  d: D | null,
  h: { sample_rate_hz: number; channels: number; bitrate_kbps: number } | null,
): D | null {
  if (!d || !h) return d;
  return { ...d, sample_rate_hz: d.sample_rate_hz ?? h.sample_rate_hz, channels: d.channels ?? h.channels, bitrate_kbps: h.bitrate_kbps ?? d.bitrate_kbps };
}

const tagOf = (f: Details | null, ...names: string[]) => {
  const want = names.map((n) => n.toLowerCase());
  return f?.tags.find((t) => want.includes(t.name.toLowerCase()) && t.value.trim())?.value.trim() ?? null;
};
/** "2021-03-12": [year, "12 March 2021"]; "2021-03": [year, "March 2021"]; a year as it is. */
function released(v: string | null): [string, string] | null {
  if (!v) return null;
  const [y, m, d] = v.split(/[-/.T ]/).map(Number);
  if (!y) return [v, v];
  if (!m) return [String(y), String(y)];
  const when = new Date(y, m - 1, d || 1);
  return [String(y), when.toLocaleDateString(undefined, d ? { day: 'numeric', month: 'long', year: 'numeric' } : { month: 'long', year: 'numeric' })];
}
const monthOf = (s: number, month: 'long' | 'short' = 'long') => new Date(s * 1000).toLocaleDateString(undefined, { month, year: 'numeric' });
function ago(s: number, short = false): string {
  const days = Math.floor((Date.now() / 1000 - s) / 86400);
  if (days < 1) return 'today';
  if (days === 1) return short ? '1d ago' : 'yesterday';
  if (days < 30) return short ? `${days}d ago` : `${days} days ago`;
  return monthOf(s, short ? 'short' : 'long');
}
/** A long value cut for a chip. */
const cut = (v: string, n = 18) => (v.length > n ? `${v.slice(0, n - 1).trimEnd()}…` : v);
/** "Max Richter" → "Richter"; several (", " or " & ") → the first's. */
const surname = (v: string) => v.split(/\s*[,;&/]\s*/)[0].trim().split(/\s+/).pop() ?? v;
/** "3/12" or "3" with the total apart. */
const numberOf = (v: string | null) => (v ? Number(v.split('/')[0]) || null : null);
const totalOf = (v: string | null, total: string | null) => Number(v?.split('/')[1]) || Number(total) || null;

/** Each fact known: in full (the overview) and short (the row's chip); a meter's 0..1 too. */
export function factsOf(f: Details | null, info: TrackInfo | null): Partial<Record<FactId, Fact>> {
  const out: Partial<Record<FactId, Fact>> = {};
  if (f?.genres.length) out.genre = { full: f.genres.slice(0, 4).join(' · '), short: cut(f.genres[0]) };
  const style = f?.style?.slice(0, 3).map((m) => wordShown(m.name));
  if (style?.length) out.style = { full: style.join(' · '), short: cut(style[0]) };
  const moods = f?.mood?.slice(0, 3).map((m) => wordShown(m.name));
  if (moods?.length) out.mood = { full: moods.join(' · '), short: moods[0] };
  const r = released(tagOf(f, 'originaldate', 'releasedate', 'date'));
  if (r) out.released = { full: r[1], short: r[0] };
  const bpm = f?.bpm ?? info?.bpm ?? null;
  if (bpm) out.tempo = { full: `${Math.round(bpm)} BPM`, short: `${Math.round(bpm)} BPM` };
  const key = tagOf(f, 'initialkey', 'key', 'tkey') ?? f?.key ?? null;
  if (key) out.key = { full: key, short: cut(key, 8) };
  const plays = f?.play_count ?? info?.play_count ?? null;
  if (plays !== null) out.plays = { full: plays === 0 ? 'Not yet' : plays === 1 ? 'Once' : `${plays} times`, short: `${plays} ${plays === 1 ? 'play' : 'plays'}` };
  if (f?.last_played) out.lastPlayed = { full: ago(f.last_played), short: ago(f.last_played, true) };
  const format = f?.format?.toUpperCase() ?? info?.codec?.toUpperCase() ?? null;
  const depth = f?.bit_depth ?? info?.bit_depth ?? null;
  const rate = f?.sample_rate_hz ?? info?.sample_rate_hz ?? null;
  const kbps = f?.bitrate_kbps ?? info?.bitrate_kbps ?? null;
  const khz = rate ? +(rate / 1000).toFixed(1) : null;
  const full = [format, depth && khz ? `${depth}-bit / ${khz} kHz` : kbps ? `${kbps} kbps` : null].filter(Boolean).join(' ');
  if (full) out.quality = { full, short: [format, depth && khz ? `${depth}/${khz}` : kbps ? `${kbps}` : null].filter(Boolean).join(' ') };
  const label = tagOf(f, 'label', 'organization', 'publisher', 'tpub');
  if (label) out.label = { full: label, short: cut(label) };
  const catalogue = tagOf(f, 'catalognumber', 'catalogue', 'catalog');
  if (catalogue) out.catalogue = { full: catalogue, short: cut(catalogue, 14) };
  if (f?.added) out.added = { full: monthOf(f.added), short: `Added ${monthOf(f.added, 'short')}` };
  if (typeof f?.energy === 'number') out.energy = { full: `${Math.round(f.energy * 100)}%`, short: `Energy ${Math.round(f.energy * 100)}%`, meter: f.energy };
  if (typeof f?.danceability === 'number')
    out.danceability = { full: `${Math.round(f.danceability * 100)}%`, short: `Dance ${Math.round(f.danceability * 100)}%`, meter: f.danceability };
  const composer = tagOf(f, 'composer', 'tcom');
  if (composer) out.composer = { full: composer, short: cut(surname(composer)) };
  const trackTag = tagOf(f, 'tracknumber', 'track', 'trck');
  const n = numberOf(trackTag);
  if (n) {
    const of = totalOf(trackTag, tagOf(f, 'totaltracks', 'tracktotal'));
    const discTag = tagOf(f, 'discnumber', 'disc', 'tpos');
    const disc = numberOf(discTag);
    const discs = totalOf(discTag, tagOf(f, 'totaldiscs', 'disctotal'));
    out.track = {
      full: [`Track ${n}${of ? ` of ${of}` : ''}`, disc && (discs ?? 0) > 1 ? `Disc ${disc} of ${discs}` : null].filter(Boolean).join(' · '),
      short: of ? `${n}/${of}` : `#${n}`,
    };
  }
  if (info?.explicit) out.explicit = { full: 'Explicit', short: 'E' };
  return out;
}
