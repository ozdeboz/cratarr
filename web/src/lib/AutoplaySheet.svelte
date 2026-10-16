<script lang="ts">
  // Autoplay (decision 0036, amended 2026-10-16; the owner's name, ∞ its icon): what plays when the queue runs out. Its
  // choice as toggle cards (Stop, or how the music goes on), then the two switches, as Symfonium's Smart queue: whole
  // albums while albums were playing, and a playlist starting again instead. The sheet stays open while it is set.
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { local, setAutoplay } from './localPlayer.svelte';

  let { onclose }: { onclose: () => void } = $props();

  type Choice = 'stop' | 'similar' | 'artist' | 'era' | 'random';
  const CHOICES: { mode: Choice; name: string }[] = [
    { mode: 'stop', name: 'Stop' },
    { mode: 'similar', name: 'Similar' },
    { mode: 'artist', name: 'Same artist' },
    { mode: 'era', name: 'Same era' },
    { mode: 'random', name: 'Random' },
  ];
  const now = $derived<Choice>(local.autoplay?.mode ?? 'stop');
  const albums = $derived(local.autoplay?.albums ?? false);
  const restart = $derived(local.autoplay?.restart ?? false);
</script>

<MenuSheet label="Autoplay" icon="infinity" title="Autoplay" chips={[CHOICES.find((c) => c.mode === now)?.name ?? 'Stop']} {onclose}>
  <div class="cards">
    <div role="radiogroup" aria-label="When the queue ends">
      {#each CHOICES as c (c.mode)}
        <MenuEntry label={c.name} checked={now === c.mode} onclick={() => setAutoplay(c.mode)} />
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
  .cards {
    --card: color-mix(in srgb, var(--text) 8%, transparent);
    --card-hover: color-mix(in srgb, var(--text) 13%, transparent);
    display: grid;
    gap: 12px;
    padding: 4px 12px 8px;
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
    min-height: 64px;
    padding: 0 16px 0 20px;
    border: 0;
    border-radius: 6px;
    background: var(--card);
    color: var(--text);
    font: inherit;
    font-size: 16px;
    text-align: left;
    cursor: pointer;
  }

  .switch:first-child {
    border-top-left-radius: 20px;
    border-top-right-radius: 20px;
  }

  .switch:last-child {
    border-bottom-left-radius: 20px;
    border-bottom-right-radius: 20px;
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
