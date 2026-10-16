<script lang="ts">
  // The filter, after Symfonium's source filter (opened from the funnel in a floating pill, lib/TopPill): first what
  // the list it was opened on leaves out, for that list only (decided 2026-10-16: Playlists' Favourites only, the
  // panel's Hide missing tracks; not remembered), then, last, the one filter that is remembered and holds everywhere: a
  // card per server this browser can follow, with its kind and its libraries. On the followed server's card the
  // libraries are switches on the Library (decision 0034), every used one, remembered as the server is; the last one on
  // stays on, so the page never filters down to nothing; elsewhere a chip only shows the library's use. A tap on a server's card follows it; a tap on the followed one goes back to no choice, which is
  // Cratarr's own source where the server has it (lib/sources). The libraries are only shown, with their use: Cratarr
  // has no per-library visibility, and a use is set only in Settings (decision 0009).
  import { pageView } from './pageView.svelte';
  import { panel } from './panel.svelte';
  import { getSettings, getStatus, type LibraryUse, type Settings } from './api';
  import Icon from './Icon.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { prefs } from './prefs.svelte';
  import { known, libraryShown, noChoice, remember, showLibrary } from './sources.svelte';
  import { toast } from './toast.svelte';

  // `page`: the page it was opened on, whose own filters it shows (Playlists: favourites only); null in the panel
  // (Hide missing tracks).
  let { page = null, onclose }: { page?: string | null; onclose: () => void } = $props();

  // The servers: the status call fills the list when no playlist poll has yet.
  $effect(() => {
    getStatus()
      .then((s) => known.list.length === 0 && remember(s.sources))
      .catch(() => {
        /* the page's own polls report connection problems */
      });
  });

  // Each server's own name and its libraries, as Settings → Libraries reads them. The cards show without them until
  // they come: the list of servers alone is enough to choose one.
  let settings = $state<Settings | null>(null);
  $effect(() => {
    let gone = false;
    getSettings().then(
      (s) => !gone && (settings = s),
      (e) => !gone && toast.fail(e, "The servers' libraries couldn't be loaded"),
    );
    return () => (gone = true);
  });

  // A use by the icon of the page that shows it (Collection shows Music, Incoming its own), so a chip needs no second
  // word; the word is still there for screen readers.
  const USE_ICON = { music: 'library', incoming: 'incoming' } as const;
  const USE_WORD: Record<LibraryUse, string> = { music: 'Music', incoming: 'Incoming', unused: 'Not used' };

  const cards = $derived(
    known.list.map((s) => {
      const view = settings?.sources.find((v) => v.id === s.id);
      // A tab's label is its kind ("Jellyfin"); the server's own name comes with the settings. Two of one kind without
      // it are told apart by their id.
      const twin = known.list.filter((x) => x.label === s.label).length > 1;
      const own = view?.status.server_name ?? null;
      const libraries = (view?.server_libraries ?? []).map((l) => ({
        id: l.id,
        name: l.name,
        use: (view!.libraries.music.includes(l.id) ? 'music' : view!.libraries.incoming.includes(l.id) ? 'incoming' : 'unused') as LibraryUse,
      }));
      return { id: s.id, name: own ?? (twin ? `${s.label} (${s.id})` : s.label), kind: own ? s.label : null, playing: s.playing, libraries };
    }),
  );

  // On the Library (decision 0034) every used library's chip is a switch; elsewhere a chip only shows the use.
  const switchable = (use: LibraryUse) => page === 'library' && use !== 'unused';
  const followedId = $derived(known.list.some((s) => s.id === prefs.source) ? prefs.source : noChoice());

  function follow(id: string) {
    prefs.source = prefs.source === id ? noChoice() : id;
  }
</script>

<MenuSheet label="Filter" icon="funnel" title="Filter" {onclose}>
  <div class="cards">
    {#if page === 'playlists'}
      <div class="toggles">
        <button class="switch" role="switch" aria-checked={pageView.favouritesOnly} onclick={() => (pageView.favouritesOnly = !pageView.favouritesOnly)}>
          <span>Favourites only</span><i aria-hidden="true"></i>
        </button>
      </div>
    {:else if page === null}
      <div class="toggles">
        <button class="switch" role="switch" aria-checked={panel.hideMissing} onclick={() => (panel.hideMissing = !panel.hideMissing)}>
          <span>Hide missing tracks</span><i aria-hidden="true"></i>
        </button>
      </div>
    {/if}

    {#each cards as c (c.id)}
      {@const on = prefs.source === c.id}
      <div class="server" class:on>
        <button
          class="pick"
          aria-pressed={on}
          aria-label={[c.name, c.kind, c.playing ? 'playing' : null].filter(Boolean).join(', ')}
          onclick={() => follow(c.id)}
        >
          <Icon name="server" size={24} />
          <span class="names">
            <span class="name">{c.name}{#if c.playing}<Icon name="play" size={12} />{/if}</span>
            {#if c.kind}<span class="kind">{c.kind}</span>{/if}
          </span>
          {#if on}<Icon name="check" size={22} />{/if}
        </button>
        {#if c.libraries.length}
          <ul class="libs">
            {#each c.libraries as l (l.id)}
              {#if c.id === followedId && switchable(l.use) && l.use !== 'unused'}
                {@const shown = libraryShown(c.id, l.id)}
                {@const last = shown && c.libraries.filter((x) => switchable(x.use) && libraryShown(c.id, x.id)).length === 1}
                <li>
                  <button class="lib" class:off={!shown} aria-pressed={shown} disabled={last} aria-label={`${l.name}, ${USE_WORD[l.use]}`} onclick={() => showLibrary(c.id, l.id, !shown)}
                    ><Icon name={USE_ICON[l.use]} size={16} /><span>{l.name}</span></button
                  >
                </li>
              {:else}
                <li class="tag" class:unused={l.use === 'unused'} aria-label={`${l.name}, ${USE_WORD[l.use]}`}>
                  {#if l.use !== 'unused'}<Icon name={USE_ICON[l.use]} size={16} />{/if}<span>{l.name}</span>
                </li>
              {/if}
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  </div>
</MenuSheet>

<style>
  /* Symfonium's sheet: rounded cards on the sheet, a little apart. */
  .cards {
    --card: color-mix(in srgb, var(--text) 8%, transparent);
    --card-hover: color-mix(in srgb, var(--text) 13%, transparent);
    display: grid;
    gap: 12px;
    padding: 4px 12px 8px;
  }

  /* The switches share one card: each a row of it, a hairline apart, the card's corners on the first and last. */
  .toggles {
    display: grid;
    gap: 3px;
  }

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

  .switch span {
    flex: 1;
    min-width: 0;
  }

  /* Material 3's switch, as Symfonium's: a 52 × 32 track, its thumb growing and moving over when on, the track then
     filled with the accent. */
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

  /* A server's card: the followed one bordered in the accent (the row's "ticked" state) and checked. */
  .server {
    position: relative;
    display: grid;
    gap: 12px;
    padding: 14px 16px;
    border: 2px solid transparent;
    border-radius: 20px;
    background: var(--card);
  }

  .server:hover {
    background: var(--card-hover);
  }

  .server.on {
    border-color: var(--art-accent, var(--accent));
  }

  .server:has(.pick:focus-visible) {
    outline: 2px solid var(--art-accent, var(--accent));
    outline-offset: 2px;
  }

  .pick {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    padding: 0;
    background: none;
    border: 0;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .pick:focus-visible {
    outline: none;
  }

  /* The whole card is the button: the chips are only shown, so a tap on them follows the server too. */
  .pick::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: 18px;
  }

  .pick :global(svg) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .names {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    font-size: 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name :global(svg) {
    margin-left: 8px;
    vertical-align: middle;
  }

  .kind {
    color: var(--muted);
    font-size: 0.85rem;
  }

  /* Under the name, as Symfonium's library chips: the use's icon and the library's name; a library not used is only
     outlined. */
  .libs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0 0 0 40px;
    list-style: none;
  }

  .libs li {
    display: flex;
  }

  .tag,
  .lib {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 18%, transparent);
    color: inherit;
    font: inherit;
    font-size: 0.9rem;
  }

  /* A switch: on as the chips are, off only outlined; the last one on cannot be switched off. */
  .lib {
    cursor: pointer;
  }

  .lib.off {
    border-color: var(--border);
    background: none;
    color: var(--muted);
  }

  .lib:disabled {
    cursor: default;
  }

  .libs li.unused {
    border-color: var(--border);
    background: none;
    color: var(--muted);
  }

  .libs :global(svg) {
    flex: none;
  }
</style>
