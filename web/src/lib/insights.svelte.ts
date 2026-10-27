// The insights data and what is derived from it (decision 0023): fetched once, read by the Insights page (statistics,
// the graph) and by the playlist view's Related section (a playlist's genres, focus and closest playlists).
import { ApiError, getInsights, type Insights, type Proximity } from './api';
import { hiddenLibraries } from './listFilter.svelte';
import { prefs } from './prefs.svelte';
import { focusRanks, key, rankers, weigh } from './proximity';

class InsightsStore {
  data = $state<Insights | null>(null);
  error = $state<string | null>(null);
  /** Deleted in this visit (and not undone): left out until the data is fetched again. */
  removed = $state<string[]>([]);

  /** The proximity data without deleted playlists, links renumbered. */
  shown = $derived.by((): Proximity | null => {
    if (!this.data) return null;
    const p = this.data.proximity;
    const index = new Map<number, number>();
    p.playlists.forEach((x, i) => !this.removed.includes(x.name) && index.set(i, index.size));
    const playlists = p.playlists.filter((_, i) => index.has(i));
    const links = p.links.filter((l) => index.has(l.a) && index.has(l.b)).map((l) => ({ ...l, a: index.get(l.a)!, b: index.get(l.b)! }));
    return { playlists, links };
  });
  rank = $derived(this.shown ? rankers(this.shown) : null);
  focus = $derived(this.shown ? focusRanks(this.shown) : []);
  /** How close each two playlists are under the signals turned on (as the graph scores them). */
  weights = $derived(this.shown && this.rank ? weigh(this.shown, this.rank, prefs.signals) : new Map<string, number>());

  #asked: string | null = null;
  /** Fetches the data once for the libraries switched on (again after a failure, and when a switch changes: Collection
   *  counts those only); what was shown stays until the new data comes. */
  load() {
    const source = prefs.source || undefined;
    const without = hiddenLibraries();
    const period = prefs.insightsPeriod;
    const asked = JSON.stringify([source, without, period]);
    if (this.#asked === asked) return;
    this.#asked = asked;
    getInsights(source, without, period).then(
      (d) => {
        if (this.#asked !== asked) return;
        this.data = d;
        this.error = null;
      },
      (e) => {
        if (this.#asked !== asked) return;
        this.#asked = null;
        this.error = e instanceof ApiError ? e.message : 'Unexpected error';
      },
    );
  }

  /** A playlist's place in the shown data, or -1. */
  indexOf(name: string) {
    return this.shown?.playlists.findIndex((p) => p.name === name) ?? -1;
  }

  /** A playlist's closest playlists under the current signals, closest first, with what they share. */
  closest(i: number) {
    const shown = this.shown;
    if (!shown || i < 0) return [];
    return shown.links
      .filter((l) => l.a === i || l.b === i)
      .map((l) => ({ l, other: l.a === i ? l.b : l.a, w: this.weights.get(key(l.a, l.b)) ?? 0 }))
      .filter((x) => x.w > 0)
      .sort((a, b) => b.w - a.w)
      .slice(0, 8)
      .map(({ l, other }) => ({
        index: other,
        name: shown.playlists[other].name,
        shares: [
          l.shared_tracks ? plural(l.shared_tracks, 'shared track') : null,
          l.genres.length ? l.genres.join(', ') : null,
          l.shared_artists ? plural(l.shared_artists, 'shared artist') : null,
        ]
          .filter(Boolean)
          .join(' · '),
      }));
  }

  renamed(from: string, to: string) {
    const point = this.data?.proximity.playlists.find((x) => x.name === from);
    if (point) point.name = to;
  }
  deleted(id: string) {
    this.removed = [...this.removed, id];
  }
  restored(id: string) {
    this.removed = this.removed.filter((x) => x !== id);
  }
}

export const insights = new InsightsStore();

/** The Map over everything (the owner, 2026-10-27: the one Insights panel with a full screen): its panel's bar turns it
 *  on, the Map's own corner off. */
export const mapFull = $state({ on: false });

export const plural = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;
