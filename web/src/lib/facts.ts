// The facts known about a track, by name (lib/trackFacts works them out): apart, with nothing imported, so the
// preferences (which keep the player row's choice) can name them without importing what reads them.

/** Every fact there is, in the overview's order (the owner's: genres, mood, release date, tempo and key, plays,
 *  quality, label, when added, energy and danceability, composer; then the rest). AudioMuse's "mood" list is style
 *  words (chillout, ambient, jazz…), so it shows as Style (the owner); its CLAP model gives the moods (happy, sad,
 *  relaxed, aggressive, party) and danceability. `meter`: shown as a bar in the overview. */
export const FACTS = [
  { id: 'genre', label: 'Genre' },
  { id: 'style', label: 'Style' },
  { id: 'mood', label: 'Mood' },
  { id: 'released', label: 'Release date' },
  { id: 'tempo', label: 'Tempo' },
  { id: 'key', label: 'Key' },
  { id: 'plays', label: 'Plays' },
  { id: 'lastPlayed', label: 'Last played' },
  { id: 'quality', label: 'Quality' },
  { id: 'label', label: 'Label' },
  { id: 'catalogue', label: 'Catalogue number' },
  { id: 'added', label: 'Added' },
  { id: 'energy', label: 'Energy', meter: true },
  { id: 'danceability', label: 'Danceability', meter: true },
  { id: 'composer', label: 'Composer' },
  { id: 'track', label: 'Track' },
  { id: 'explicit', label: 'Explicit' },
] as const;
export type FactId = (typeof FACTS)[number]['id'];

/** AudioMuse's style and mood words as shown (the owner, 2026-10-23): capitalised as the genres are. They are kept as
 *  AudioMuse names them, lowercase, which the filters and crates match. */
const WORDS: Record<string, string> = { rnb: 'RnB' };
export const wordShown = (w: string) => WORDS[w] ?? (w ? w[0].toUpperCase() + w.slice(1) : w);
/** The values axes whose values are AudioMuse's words. */
export const WORD_AXES: readonly string[] = ['style', 'mood'];
