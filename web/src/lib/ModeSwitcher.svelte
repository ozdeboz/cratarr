<script lang="ts">
  // Cratarr | Local (decision 0052, the switch; the owner chose the mocks): at the top of the sidebar (B, "One
  // switcher": a bordered button, the mark on its tile, the mode over "● Connected" or the device's name, the up-down
  // chevrons), and on a phone a slim row over the tabs (C, the same in one line). A tap opens the two modes, the current
  // one lit, no icons. Shown only with a local folder on (the owner: "only ... if the user has enabled any local
  // directories"), never in a browser; otherwise the top stays as it was. While Local plays, a note before the device's
  // name (the owner: "Add a music icon to the left of Lenovo tab in case of local playback, if it's currently playing").
  // A switch is the whole app: what plays pauses and is put by, the other mode's queue comes back paused (the app's).
  import { appInfo } from './app.svelte';
  import { localMode, setMode } from './deviceMusic.svelte';
  import GlassMenu from './GlassMenu.svelte';
  import Icon from './Icon.svelte';
  import { local } from './localPlayer.svelte';
  import Mark from './Mark.svelte';

  /** `slim`: the phone's row. `connection`: the server's, as the sidebar says it. `up`: the menu opens upwards (the bar
   *  along the bottom). */
  let { slim = false, up = false, connection }: { slim?: boolean; up?: boolean; connection: { ok: boolean; text: string } | null } = $props();
  let open = $state(false);
  let root = $state<HTMLElement>();
  let button = $state<HTMLElement>();
  // The modes in the glass menu (lib/GlassMenu, as the Library's view), from the button's place.
  let at = $state<{ top?: number; bottom?: number; left: number; minWidth: number }>({ left: 0, minWidth: 0 });
  function place() {
    const r = button?.getBoundingClientRect();
    if (!r) return;
    const left = Math.max(8, r.left);
    at = up ? { bottom: innerHeight - r.top + 8, left, minWidth: 230 } : { top: r.bottom + 8, left, minWidth: Math.min(Math.max(230, r.width), innerWidth - 16) };
  }
  const here = $derived(appInfo.player ?? 'This device');
  const playingHere = $derived(localMode() && !local.paused && local.index >= 0);
  const conn = $derived(connection?.ok ? 'Connected' : (connection?.text ?? 'Connecting'));

  function choose(mode: 'cratarr' | 'device') {
    open = false;
    setMode(mode);
  }
</script>

{#snippet cratarrLine()}<i class="dot" class:down={!connection?.ok} aria-hidden="true"></i>{conn}{/snippet}
{#snippet localLine()}{#if playingHere}<span class="note" role="img" aria-label="Playing"><Icon name="music" size={slim ? 12 : 14} /></span>{/if}{here}{/snippet}

<div class="switcher" class:slim bind:this={root}>
  <button class="current" class:open aria-haspopup="menu" aria-expanded={open} aria-label="Cratarr or Local" bind:this={button} onclick={() => (place(), (open = !open))}>
    <span class="tile"><Mark /></span>
    <span class="lines">
      <strong>{localMode() ? 'Local' : 'Cratarr'}</strong>
      <span class="sub">{#if localMode()}{@render localLine()}{:else}{@render cratarrLine()}{/if}</span>
    </span>
    <span class="chev" aria-hidden="true"><Icon name="chevronsUpDown" size={18} /></span>
  </button>
  {#if open}
    <GlassMenu {at} keep={() => button} label="Cratarr or Local" role="menu" onclose={() => (open = false)}>
      <button class="option" role="menuitemradio" aria-checked={!localMode()} onclick={() => choose('cratarr')}>
        <span class="mode"><span>Cratarr</span><span class="sub">{@render cratarrLine()}</span></span>{#if !localMode()}<Icon name="check" size={18} />{/if}
      </button>
      <button class="option" role="menuitemradio" aria-checked={localMode()} onclick={() => choose('device')}>
        <span class="mode"><span>Local</span><span class="sub">{@render localLine()}</span></span>{#if localMode()}<Icon name="check" size={18} />{/if}
      </button>
    </GlassMenu>
  {/if}
</div>

<style>
  .switcher {
    position: relative;
    margin: 0 0 18px;
  }

  /* B: the bordered button, the mark on a 38 px tile, two lines, the chevrons at the end. */
  .current {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px 10px 8px 8px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 4%, transparent);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .current:hover,
  .current:focus-visible,
  .current.open {
    border-color: var(--accent);
  }

  .tile {
    flex: none;
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    padding: 3px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    box-sizing: border-box;
  }

  .tile :global(svg) {
    width: 100%;
    height: 100%;
  }

  .lines {
    flex: 1;
    min-width: 0;
    display: grid;
  }

  strong {
    font-size: 1.05rem;
    font-weight: 700;
    line-height: 1.25;
  }

  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 0.82rem;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }

  .dot.down {
    background: var(--danger);
  }

  .note {
    flex: none;
    display: grid;
    color: var(--accent-ink);
  }

  .chev {
    flex: none;
    display: grid;
    color: var(--muted);
  }

  /* A mode in the glass menu: its name over its line. */
  .mode {
    display: grid;
    gap: 2px;
    padding: 6px 0;
  }

  /* C: the phone's slim row over the tabs, the same in one line (about 34 px). */
  .slim {
    margin: 0;
  }

  .slim .current {
    gap: 8px;
    min-height: 34px;
    padding: 0 6px;
    border: 0;
    border-radius: 0;
    background: none;
  }

  .slim .tile {
    width: 22px;
    height: 22px;
    padding: 0;
    background: none;
  }

  .slim .lines {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .slim strong {
    font-size: 0.95rem;
  }
</style>
