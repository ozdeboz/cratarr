<script lang="ts">
  // What stands behind every flow (the owner, 2026-10-26: photos to pick, no grey canvas), as the theme's pictures
  // (lib/settings/AppearancePicker): small pictures in a row, a round tick under the one shown. Cratarr's photos
  // (lib/backdrop), and one's own picture once there is one (chosen under it, Your picture). With Shuffle on (the owner,
  // 2026-10-27) the round ticks are boxes: the ticked ones take turns; at least one stays ticked; the one there now has
  // the accent's frame.
  import { BACKDROPS, backdropPhoto, shuffleNow } from '../backdrop.svelte';
  import { HOME_HEADER, localCovers } from '../localCovers.svelte';
  import { prefs, type FlowBackdrop } from '../prefs.svelte';

  const choices = $derived([
    ...BACKDROPS.map((b) => ({ value: b.value, label: b.label, src: backdropPhoto(b.value) as string | null })),
    ...(localCovers[HOME_HEADER] ? [{ value: 'own', label: 'Yours', src: localCovers[HOME_HEADER] as string | null }] : []),
  ] as { value: FlowBackdrop; label: string; src: string | null }[]);

  function tick(b: FlowBackdrop) {
    const ticked = prefs.flowShuffled;
    if (!ticked.includes(b)) prefs.flowShuffled = [...ticked, b];
    else if (ticked.length > 1) prefs.flowShuffled = ticked.filter((x) => x !== b);
  }
</script>

<div class="looks" class:shuffle={prefs.flowShuffle} role={prefs.flowShuffle ? 'group' : 'radiogroup'} aria-label="Behind the flows">
  {#each choices as c (c.value)}
    {@const on = prefs.flowShuffle ? prefs.flowShuffled.includes(c.value) : prefs.flowBackdrop === c.value}
    <button
      type="button"
      class="look"
      class:on
      class:now={prefs.flowShuffle && shuffleNow() === c.value}
      role={prefs.flowShuffle ? 'checkbox' : 'radio'}
      aria-checked={on}
      onclick={() => (prefs.flowShuffle ? tick(c.value) : (prefs.flowBackdrop = c.value))}
    >
      <span class="picture" style:background-image={c.src ? `url("${c.src}")` : null} aria-hidden="true"></span>
      <span class="name">{c.label}</span>
      <span class="tick" aria-hidden="true"></span>
    </button>
  {/each}
</div>

<style>
  .looks {
    display: flex;
    gap: 18px;
    padding: 16px 16px 12px;
    overflow-x: auto;
  }

  .look {
    display: grid;
    flex: none;
    justify-items: center;
    gap: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  /* The backdrop itself, as a flow shows it: the photo cropped towards its top. */
  .picture {
    box-sizing: border-box;
    width: 74px;
    height: 74px;
    border: 2px solid var(--border);
    border-radius: 14px;
    background: center 20% / cover no-repeat;
  }

  .looks:not(.shuffle) .look.on .picture,
  .look.now .picture {
    border-color: var(--accent);
  }

  .look:focus-visible .picture {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .name {
    font-size: 14px;
  }

  /* A round mark, ticked under the one shown (as the theme's pictures). */
  .tick {
    display: grid;
    place-items: center;
    box-sizing: border-box;
    width: 22px;
    height: 22px;
    border: 2px solid var(--border);
    border-radius: 50%;
  }

  /* Shuffle's: a box, as a check box is. */
  .shuffle .tick {
    border-radius: 6px;
  }

  .look.on .tick {
    border-color: var(--accent);
    background: var(--accent);
  }

  .look.on .tick::after {
    content: '';
    width: 5px;
    height: 10px;
    border: solid var(--accent-text);
    border-width: 0 2px 2px 0;
    transform: translateY(-1px) rotate(45deg);
  }
</style>
