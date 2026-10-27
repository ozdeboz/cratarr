<script lang="ts">
  // Play on an album's cover (the owner, 2026-10-27: every cover that plays has it, in its upper right corner, and only
  // it plays; a tap on the rest of the cover opens the album). Decision 0071's Bandcamp suggestions had it first ("a tad
  // larger", it plays at once). The accent's round button, beside the cover's own (a button cannot hold another), over
  // it; what holds it is positioned. Starting (`busy`): the ring turning in it. `small`: on a list's small cover.
  // `album`: the album it plays; while that plays, it is the cover's one sign of it (the owner, 2026-10-27: one playing
  // sign on covers, lib/PlayingDots), and a tap pauses; paused, a tap goes on. `shuffle`: it shuffles what it is on,
  // and shows Shuffle's icon (a crate's card, the owner 2026-10-27).
  import Icon from './Icon.svelte';
  import { playing as nowPlaying, playPause, shownPaused } from './remote.svelte';
  import PlayingDots from './PlayingDots.svelte';
  import ProgressRing from './ProgressRing.svelte';

  /** `problem`: why it last failed to play: a red "!" in its place, a tap on it `onproblem` (the owner, 2026-10-26).
   *  `disabled`: another is starting. `bandcamp`: it plays a Bandcamp preview, and says so. */
  let {
    label,
    busy = false,
    disabled = false,
    bandcamp = false,
    shuffle = false,
    small = false,
    problem = null,
    album = null,
    onplay,
    onproblem = null,
  }: {
    label: string;
    busy?: boolean;
    disabled?: boolean;
    bandcamp?: boolean;
    shuffle?: boolean;
    small?: boolean;
    problem?: string | null;
    album?: string | null;
    onplay: () => void;
    onproblem?: (() => void) | null;
  } = $props();
  const current = $derived(album !== null && nowPlaying()?.album_id === album);
  const sounding = $derived(current && !shownPaused());
</script>

<button
  class="cover-play"
  class:small
  class:problem={!!problem}
  aria-label={problem ? `${label} did not play: ${problem}` : sounding ? `Pause ${label}` : bandcamp ? `Play ${label} on Bandcamp` : shuffle ? `Shuffle ${label}` : `Play ${label}`}
  title={problem ? 'It did not play' : sounding ? 'Playing' : bandcamp ? 'Play on Bandcamp' : shuffle ? 'Shuffle' : 'Play'}
  disabled={busy || disabled}
  aria-busy={busy}
  onclick={(e) => {
    e.stopPropagation();
    if (problem && onproblem) onproblem();
    else if (current) playPause();
    else onplay();
  }}
  >{#if busy}<ProgressRing progress={null} size={small ? 16 : 22} spin onAccent />{:else}{#if sounding}<PlayingDots count={3} {small} />{:else}<Icon
        name={problem ? 'bang' : shuffle ? 'shuffle' : 'play'}
        size={problem ? (small ? 15 : 20) : small ? 13 : 17}
      />{/if}{/if}</button
>

<style>
  .cover-play.problem {
    background: var(--danger) !important;
    color: #fff !important;
  }

  .cover-play {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.35);
    cursor: pointer;
  }

  .cover-play.small {
    top: 3px;
    right: 3px;
    width: 22px;
    height: 22px;
  }

  /* Its own ring, not the page's busy one (app.css) over it. */
  .cover-play[aria-busy='true']::after {
    content: none;
  }

  .cover-play:disabled {
    cursor: progress;
  }

  .cover-play:disabled:not([aria-busy='true']) {
    opacity: 0.7;
  }
</style>
