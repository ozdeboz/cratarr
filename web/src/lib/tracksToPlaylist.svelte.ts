// Add to playlist for several tracks at once (an album's menu, an artist's): which playlists hold them, from each
// track's state (there is no call for several tracks at once), and adding them to one, its card spinning until the last
// is in. A playlist that holds some of them already is asked about first (decision 0025, amended 2026-10-16). The steps
// themselves are lib/PlaylistsStep's.
import { askIfMany } from './many.svelte';
import { plural } from './insights.svelte';
import { appendNew, hasBatches } from './api';
import { trackSteps } from './progress.svelte';
import { appendTracks, setTrackMember, trackState, type Playlist, type TrackState } from './api';
import { refresh } from './live.svelte';
import { toast } from './toast.svelte';
import { withUndo } from './undo.svelte';

export type ToPlaylistStep = 'menu' | 'lists' | 'again';

export class TracksToPlaylist {
  states = $state<Record<string, TrackState>>({});
  /** The playlist they are going into: one at a time. */
  adding = $state<string | null>(null);
  /** The playlist that holds some already, while the sheet asks. */
  again = $state<{ p: Playlist; already: number; of: number } | null>(null);
  // Asked for already (answered or on its way), outside the state, so an answer does not ask again for the others.
  readonly #asked = new Set<string>();
  #failed = false;

  /** `whole`: what they are, for the sheet's words ("this album"). */
  constructor(
    readonly source: () => string,
    readonly ids: () => string[],
    readonly whole: string,
  ) {}

  /** Asks for these tracks' states (their playlists). */
  ask = (list: string[]) => {
    for (const id of list) {
      if (this.#asked.has(id)) continue;
      this.#asked.add(id);
      trackState(this.source(), id).then(
        (v) => (this.states[id] = v),
        (e) => {
          // Asked again the next time the playlists show; one line for the lot, not one per track.
          this.#asked.delete(id);
          if (this.#failed) return;
          this.#failed = true;
          toast.fail(e, `The playlists of ${this.whole} couldn't be loaded`);
        },
      );
    }
  };

  /** The first track's state: whether they can go in a playlist (in Music) and which playlists there are. */
  get first(): TrackState | null {
    const id = this.ids()[0];
    return id ? (this.states[id] ?? null) : null;
  }
  get addable() {
    return !!this.first?.addable;
  }
  get playlists() {
    return this.first?.playlists ?? [];
  }
  /** Every track's state is known (until then no playlist is marked as holding them). */
  get known() {
    const ids = this.ids();
    return ids.length > 0 && ids.every((id) => this.states[id]);
  }
  member = (id: string, p: Playlist) => !!this.states[id]?.playlists.find((x) => x.id === p.id)?.member;
  /** The playlist holds every one of them that can go in one. */
  holds = (p: Playlist) => this.known && this.ids().every((id) => !this.states[id].addable || this.member(id, p));

  /** A tap on a playlist: asks first when it holds some of them (`step` goes to 'again'), else adds the others. */
  add = async (p: Playlist, step: (s: ToPlaylistStep) => void) => {
    if (this.adding || !this.known) return;
    const can = this.ids().filter((id) => this.states[id].addable);
    const already = can.filter((id) => this.member(id, p)).length;
    if (already > 0) {
      this.again = { p, already, of: can.length };
      step('again');
      return;
    }
    // Many ask first (lib/many).
    const n = plural(can.length, 'track');
    askIfMany(can.length, { title: `Add ${n} to ${p.name}?`, label: `Add ${n}`, icon: 'listAdd' }, () => void this.addOthers(p, step));
  };

  /** Every one again at the playlist's end, those it holds included; many ask first (lib/many). */
  addAgain = (p: Playlist, done: () => void) => {
    if (this.adding) return;
    const count = this.ids().filter((id) => this.states[id].addable).length;
    const n = plural(count, 'track');
    askIfMany(count, { title: `Add ${n} to ${p.name}?`, label: `Add ${n}`, icon: 'listAdd' }, () => void this.appendAll(p, done));
  };

  private appendAll = async (p: Playlist, done: () => void) => {
    if (this.adding) return;
    this.adding = p.id;
    try {
      const r = await appendTracks(
        p.id,
        this.source(),
        this.ids().filter((id) => this.states[id].addable),
      );
      withUndo(`Added ${r.added} ${r.added === 1 ? 'track' : 'tracks'} to ${p.name}.`, [r.undo]);
      done();
      refresh();
    } catch (e) {
      toast.fail(e, 'The tracks were not added');
    } finally {
      this.adding = null;
    }
  };

  /** Those the playlist does not hold yet, one by one in their order (those not in Music passed). */
  addOthers = async (p: Playlist, step: (s: ToPlaylistStep) => void) => {
    if (this.adding) return;
    step('lists');
    this.adding = p.id;
    try {
      // One call where the server has it (0.9.4); else one by one, the bar at the top saying how far it is.
      const ids = this.ids();
      if (await hasBatches()) {
        const want = ids.filter((id) => !(this.states[id] && !this.states[id].addable));
        await appendNew(p.id, this.source(), want);
        // Every one of them is in it now (the call's promise): marked so, without asking each again.
        for (const id of want) {
          const s = this.states[id];
          if (s) this.states[id] = { ...s, playlists: s.playlists.map((x) => (x.id === p.id ? { ...x, member: true } : x)) };
        }
        refresh();
        return;
      }
      await trackSteps(ids.length, async (done) => {
        for (const id of ids) {
          const s = this.states[id];
          if (!(s && (!s.addable || this.member(id, p)))) this.states[id] = await setTrackMember(this.source(), id, p.id, true);
          done();
        }
      });
      refresh();
    } catch (e) {
      toast.fail(e, 'Not every track was added');
      refresh();
    } finally {
      this.adding = null;
    }
  };
}
