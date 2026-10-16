<script lang="ts" module>
  import { local, type AutoplayMode, type RadioBy } from './localPlayer.svelte';

  export const AUTOPLAY_NAMES: Record<AutoplayMode, string> = { similar: 'Similar', artist: 'Same artist', era: 'Same era', random: 'Random' };
  export const RADIO_NAMES: Record<RadioBy, string> = { track: 'Similar to the track', artist: 'Same artist', era: 'Same era' };

  /** What is on, in a line: the queue's floating pill and the sheet's chip ("Radio · ∞ Similar", "Off"). */
  export function moreMusicLine() {
    const parts = [local.radio ? (local.radioBy === 'track' ? 'Radio' : `Radio, ${RADIO_NAMES[local.radioBy].toLowerCase()}`) : null, local.autoplay ? `∞ ${AUTOPLAY_NAMES[local.autoplay.mode]}${local.autoplay.albums ? ', albums' : ''}` : null];
    return parts.filter(Boolean).join(' · ') || 'Off';
  }
</script>

<script lang="ts">
  // More music (decision 0036, amended; one sheet for both, decided 2026-10-16): what plays besides what was chosen.
  // Radio, between: tracks like each of the queue's own tracks (similar to it, its artist or its era), so many after
  // it. Autoplay, at the end: what plays when the queue runs out, then its switches (whole albums while albums were
  // playing; a playlist starting again instead). Each a choice of one, as View's order (a round mark); the sheet stays
  // open while it is set. From the queue's floating pill and the player's radio button.
  import Icon from './Icon.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { RADIO_BYS, RADIO_COUNTS, setAutoplay, setRadio } from './localPlayer.svelte';
  import Seg from './Seg.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const RADIO: { by: RadioBy | 'off'; name: string }[] = [{ by: 'off', name: 'Off' }, ...RADIO_BYS.map((by) => ({ by, name: RADIO_NAMES[by] }))];
  const radioNow = $derived<RadioBy | 'off'>(local.radio ? local.radioBy : 'off');
  // Seg takes words: the counts as such.
  const COUNTS = RADIO_COUNTS.map((n) => ({ value: String(n), label: String(n) }));
  const AUTOPLAY: { mode: AutoplayMode | 'stop'; name: string }[] = [
    { mode: 'stop', name: 'Stop' },
    ...(Object.entries(AUTOPLAY_NAMES) as [AutoplayMode, string][]).map(([mode, name]) => ({ mode, name })),
  ];
  const now = $derived<AutoplayMode | 'stop'>(local.autoplay?.mode ?? 'stop');
  const albums = $derived(local.autoplay?.albums ?? false);
  const restart = $derived(local.autoplay?.restart ?? false);
</script>

<MenuSheet label="More music" icon="radio" title="More music" chips={[moreMusicLine()]} {onclose}>
  <div class="parts">
    <div class="part choices" role="radiogroup" aria-label="Radio">
      <span>Radio <small>after each of your tracks</small></span>
      {#each RADIO as r (r.by)}
        {@const on = radioNow === r.by}
        <button role="radio" aria-checked={on} class:on onclick={() => (r.by === 'off' ? setRadio(false) : setRadio(true, { by: r.by }))}
          ><Icon name={on ? 'circleCheck' : 'circle'} size={22} />{r.name}</button
        >
      {/each}
      <div class="count" class:off={!local.radio}>
        <span>After each track</span>
        <Seg label="How many after each track" options={COUNTS} bind:value={() => String(local.radioCount), (n) => setRadio(local.radio, { count: Number(n) })} />
      </div>
    </div>
    <div class="part choices" role="radiogroup" aria-label="Autoplay">
      <span>Autoplay <small>when the queue ends</small></span>
      {#each AUTOPLAY as a (a.mode)}
        {@const on = now === a.mode}
        <button role="radio" aria-checked={on} class:on onclick={() => setAutoplay(a.mode)}><Icon name={on ? 'circleCheck' : 'circle'} size={22} />{a.name}</button>
      {/each}
    </div>
    <div class="toggles">
      <button class="switch" role="switch" aria-checked={albums} disabled={now === 'stop'} onclick={() => setAutoplay(now, { albums: !albums })}>
        <span>Queue albums</span><i aria-hidden="true"></i>
      </button>
      <button class="switch" role="switch" aria-checked={restart} disabled={now === 'stop'} onclick={() => setAutoplay(now, { restart: !restart })}>
        <span>Restart playlists</span><i aria-hidden="true"></i>
      </button>
    </div>
  </div>
</MenuSheet>

<style>
  /* As View's sheet: each part a rounded card, a choice of one as rows with a round mark. */
  .parts {
    --card: color-mix(in srgb, var(--text) 6%, transparent);
    --card-hover: color-mix(in srgb, var(--text) 11%, transparent);
    display: grid;
    gap: 8px;
    padding: 4px 16px 16px;
  }

  .part {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 6px 12px 6px 16px;
    border-radius: 12px;
    background: var(--card);
  }

  .part > span {
    padding: 10px 0 4px;
    font-size: 15px;
    font-weight: 600;
  }

  .part > span small {
    margin-left: 6px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 400;
  }

  .choices button {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 15px;
    text-align: left;
    cursor: pointer;
  }

  .choices button.on :global(svg) {
    color: var(--art-accent, var(--accent));
  }

  /* How many after each track: the choice of number as View's (Seg), at the part's foot. */
  .count {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 52px;
    margin-top: 4px;
    border-top: 1px solid var(--border);
    font-size: 15px;
  }

  .count.off {
    opacity: 0.55;
  }

  .toggles {
    display: grid;
    gap: 3px;
  }

  /* As the filter sheet's switches (Material 3's, as Symfonium's). */
  .switch {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 56px;
    padding: 0 12px 0 16px;
    border: 0;
    border-radius: 4px;
    background: var(--card);
    color: var(--text);
    font: inherit;
    font-size: 15px;
    text-align: left;
    cursor: pointer;
  }

  .switch:first-child {
    border-top-left-radius: 12px;
    border-top-right-radius: 12px;
  }

  .switch:last-child {
    border-bottom-left-radius: 12px;
    border-bottom-right-radius: 12px;
  }

  .switch:hover {
    background: var(--card-hover);
  }

  .switch:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .switch span {
    flex: 1;
    min-width: 0;
  }

  .switch i {
    position: relative;
    flex: none;
    box-sizing: border-box;
    width: 52px;
    height: 32px;
    border: 2px solid var(--muted);
    border-radius: 16px;
    transition:
      background-color 150ms,
      border-color 150ms;
  }

  .switch i::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 6px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--muted);
    transform: translateY(-50%);
    transition:
      left 150ms,
      width 150ms,
      height 150ms,
      background-color 150ms;
  }

  .switch[aria-checked='true'] i {
    border-color: var(--art-accent, var(--accent));
    background: var(--art-accent, var(--accent));
  }

  .switch[aria-checked='true'] i::after {
    left: 22px;
    width: 24px;
    height: 24px;
    background: var(--accent-text);
  }
</style>
