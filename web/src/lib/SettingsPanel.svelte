<script lang="ts">
  // A part of the settings in the panel (decided 2026-10-16: "the settings page should really be a portal with cards that
  // open settings-specific panels"): the Settings page's card opens it, as a playlist opens. Each part is its own file
  // (lib/settings/*Part) over one editor (lib/settings/editor), so what is changed in one part shows the Save bar in
  // all of them and survives going from one to another. Authentication's account, Import, this device's settings and
  // Display act at once.
  import Icon from './Icon.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import PanelPlace from './PanelPlace.svelte';
  import { appInfo, askDevice, askDownloads, inApp } from './app.svelte';
  import { closePlaylist } from './panel.svelte';
  import { SECTIONS, type SettingsSection } from './settingsSections.svelte';
  import { ed } from './settings/editor.svelte';
  import AuthPart from './settings/AuthPart.svelte';
  import ConnectionsPart from './settings/ConnectionsPart.svelte';
  import DevicePart from './settings/DevicePart.svelte';
  import DisplayPart from './settings/DisplayPart.svelte';
  import SyncPart from './settings/SyncPart.svelte';
  import ImportPart from './settings/ImportPart.svelte';
  import LibrariesPart from './settings/LibrariesPart.svelte';
  import PlayersSettings from './PlayersSettings.svelte';

  let { section }: { section: SettingsSection } = $props();
  const title = $derived(section === 'device' ? (appInfo.player ?? 'This device') : (SECTIONS.find((x) => x.id === section)?.title ?? 'Settings'));

  // Inside the Android app: its settings as they are now (Android's background permission may have changed meanwhile).
  if (inApp) {
    askDevice();
    askDownloads();
  }
  ed.load();
</script>

<div class="view">
  <div class="bar">
    <span class="navpill"><button aria-label="Close" title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button></span>
    <h2 class="title">{title}</h2>
    <span class="end"><PanelPlace /></span>
  </div>
  <div class="scroll">
    {#if ed.loadError}<Notice text={ed.loadError} />{/if}
    {#if section === 'device'}
      <DevicePart />
    {:else if section === 'sync'}
      <SyncPart />
    {:else if section === 'display'}
      <DisplayPart />
    {:else if section === 'players'}
      <PlayersSettings />
    {:else if !ed.s}
      {#if !ed.loadError}<PageLoading />{/if}
    {:else if section === 'connections'}
      <ConnectionsPart />
    {:else if section === 'libraries'}
      <LibrariesPart />
    {:else if section === 'auth'}
      <AuthPart />
    {:else if section === 'import'}
      <ImportPart />
    {/if}

    {#if ed.dirty}
      <div class="savebar" role="region" aria-label="Unsaved changes">
        <span>Unsaved changes</span>
        <button class="primary" disabled={ed.saving || !ed.s?.settings_file} aria-busy={ed.saving} onclick={ed.save}>Save</button>
        <button disabled={ed.saving} onclick={() => ed.s && ed.fill(ed.s)}>Discard</button>
      </div>
    {/if}
  </div>
</div>

<style>
  /* The panel: its bar on top, the part under it scrolling (room left for the mini player). */
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .bar .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 20px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .end {
    display: flex;
    align-items: center;
  }

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px clamp(12px, 2vw, 24px) calc(96px + var(--mini-h, 0px));
  }

  /* The part's own heading is the bar's: kept for its name (aria-labelledby), not shown. */
  .scroll :global(section > h2:first-child) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .scroll :global(section) {
    margin-bottom: 20px;
    padding: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
  }

  .scroll :global(h2) {
    margin: 0 0 6px;
    font-size: 1.05rem;
  }

  .scroll :global(h3) {
    margin: 0;
    margin-right: auto;
    font-size: 0.98rem;
  }

  .scroll :global(h3 small),
  .scroll :global(.l small) {
    margin-left: 6px;
    color: var(--muted);
    font-weight: 400;
    font-size: 0.8rem;
  }

  .scroll :global(.mono) {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85rem;
  }

  .scroll :global(.example) {
    overflow-wrap: anywhere;
  }

  .scroll :global(.lede) {
    margin: 0 0 14px;
    color: var(--muted);
    font-size: 0.88rem;
  }

  .scroll :global(.lede strong) {
    color: var(--text);
  }

  .scroll :global(dl) {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 8px 16px;
    margin: 0;
    font-size: 0.92rem;
  }

  .scroll :global(dt) {
    color: var(--muted);
  }

  .scroll :global(dl.account) {
    margin-bottom: 14px;
  }

  .scroll :global(dd) {
    margin: 0;
    overflow-wrap: anywhere;
  }

  @media (max-width: 520px) {
    .scroll :global(dl) {
      grid-template-columns: minmax(0, 1fr);
      gap: 2px;
    }

    .scroll :global(dd) {
      margin-bottom: 8px;
    }
  }

  .scroll :global(code) {
    font-size: 0.85em;
    overflow-wrap: anywhere;
  }

  .scroll :global(.dot) {
    display: inline-block;
    flex: none;
    width: 8px;
    height: 8px;
    margin-right: 8px;
    border-radius: 50%;
    background: var(--danger);
  }

  .scroll :global(.dot.ok) {
    background: var(--accent);
  }

  .scroll :global(.bad) {
    color: var(--danger);
  }

  .scroll :global(.server) {
    display: grid;
    gap: 14px;
    margin-bottom: 14px;
    padding: 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
  }

  .scroll :global(.libs-of + .libs-of) {
    margin-top: 12px;
  }

  .scroll :global(.libs) {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* A server library: where it is beside its use, its own settings under both (a grid: nothing wraps into a column). */
  .scroll :global(.libs li) {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .scroll :global(.policies) {
    grid-column: 1 / -1;
  }

  .scroll :global(.libs li:last-child) {
    border-bottom: 0;
  }

  .scroll :global(.swatches) {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .scroll :global(.swatch) {
    position: relative;
    width: 28px;
    height: 28px;
    min-height: 0;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 50%;
    cursor: pointer;
  }

  .scroll :global(.swatch[aria-checked='true']),
  .scroll :global(.swatch.own.on) {
    box-shadow:
      0 0 0 2px var(--bg),
      0 0 0 4px var(--text);
  }

  /* Any other colour: the system's picker, behind a round of the chosen one (else a rainbow). */
  .scroll :global(.swatch.own) {
    display: block;
    overflow: hidden;
    background: conic-gradient(#e0a23a, #d4567f, #9a6fdc, #5b8def, #3fae9a, #e0a23a);
  }

  .scroll :global(.swatch.own input) {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .scroll :global(.sr) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  /* Phones: every row the same, the name above and the choice full width below. */
  @media (max-width: 520px) {
    .scroll :global(.libs li) {
      grid-template-columns: minmax(0, 1fr);
    }

    .scroll :global(.libs .seg button) {
      flex: 1;
    }
  }

  .scroll :global(.where) {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .scroll :global(.where .warn code) {
    color: inherit;
  }

  .scroll :global(.sub) {
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .scroll :global(.sub h3) {
    margin-bottom: 6px;
  }

  .scroll :global(details.sub summary) {
    cursor: pointer;
    font-weight: 600;
    font-size: 0.98rem;
  }

  .scroll :global(details.sub[open] summary) {
    margin-bottom: 8px;
  }

  .scroll :global(textarea) {
    width: 100%;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    resize: vertical;
  }

  .scroll :global(.server.off .fields) {
    opacity: 0.6;
  }

  .scroll :global(.server header) {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .scroll :global(.ask) {
    font-size: 0.9rem;
  }

  .scroll :global(.state) {
    display: flex;
    align-items: center;
    margin: 0;
    font-size: 0.88rem;
    overflow-wrap: anywhere;
  }

  .scroll :global(.fields) {
    display: grid;
    gap: 14px;
  }

  .scroll :global(label) {
    display: grid;
    gap: 6px;
  }

  .scroll :global(.l) {
    font-size: 0.9rem;
    font-weight: 600;
  }

  .scroll :global(.hint) {
    color: var(--muted);
    font-size: 0.8rem;
  }

  .scroll :global(.hint.warn) {
    color: var(--warn);
  }

  .scroll :global(.hint.bad) {
    color: var(--danger);
  }

  .scroll :global(.checks) {
    display: grid;
  }

  .scroll :global(.pref :global(.switch .label)) {
    font-size: 0.9rem;
    font-weight: 600;
  }

  .scroll :global(.checks :global(.switch .label)) {
    font-size: 0.92rem;
  }

  .scroll :global(input[type='url']),
  .scroll :global(input[type='number']),
  .scroll :global(input[type='text']),
  .scroll :global(input[type='password']),
  .scroll :global(select) {
    width: 100%;
    min-height: 44px;
    padding: 0 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .scroll :global(.row) {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .scroll :global(.row select) {
    width: auto;
  }

  .scroll :global(button) {
    min-height: 44px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .scroll :global(.server button) {
    background: var(--surface);
  }

  .scroll :global(button.primary) {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .scroll :global(button.danger) {
    border-color: var(--danger);
    color: var(--danger);
  }

  .scroll :global(button:disabled) {
    opacity: 0.5;
    cursor: default;
  }

  .scroll :global(.pref) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    min-height: 52px;
    border-bottom: 1px solid var(--border);
  }

  .scroll :global(.pref:last-child) {
    border-bottom: 0;
  }

  .scroll :global(.seg) {
    display: flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }

  .scroll :global(.seg button) {
    border: 0;
    border-radius: 0;
  }

  .scroll :global(.seg button[aria-pressed='true']) {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  /* Appears only with unsaved changes, at the bottom of the screen. */
  .scroll :global(.savebar) {
    position: sticky;
    bottom: calc(8px + env(safe-area-inset-bottom));
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--surface);
    border: 1px solid var(--accent);
    border-radius: 12px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  .scroll :global(.savebar span) {
    margin-right: auto;
    font-weight: 600;
  }

  @media (min-width: 1024px) {
    .scroll :global(.savebar) {
      bottom: 16px;
    }
  }

  .scroll :global(.days) {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .scroll :global(.import) {
    display: grid;
    gap: 14px;
  }

  .scroll :global(.import h3) {
    margin-bottom: 0;
  }

  .scroll :global(.import .state),
  .scroll :global(.import .hint) {
    margin: 0;
  }

  @media (min-width: 521px) {
    .scroll :global(.fields.pair) {
      grid-template-columns: 1fr 1fr;
    }
  }

  .scroll :global(.days input) {
    width: 6em;
  }
</style>
