<script lang="ts">
  import { createPlaylist, type Playlist } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { live, queueTap, refresh } from './lib/live.svelte';
  import Notice from './lib/Notice.svelte';
  import { toast, type Toast } from './lib/toast.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import Icon from './lib/Icon.svelte';
  import { isLocal, setPlayingMember, shownPlaylists } from './lib/output.svelte';
  import { pending, playing } from './lib/remote.svelte';
  import { longpress } from './lib/longpress';
  import { openPlaylist } from './lib/panel.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites } from './lib/favourites.svelte';

  // What is playing and the playlists come from the shell's poll (live.svelte.ts).
  const view = $derived(live.view);
  // The favourites' hearts on the chips (decision 0023).
  $effect(() => void loadPlaylistFavorites());
  const error = $derived(live.error);
  let filter = $state('');
  // Size = chip width, which sets how many columns fit; one width per step in prefs.ZOOM_STEPS.
  const WIDTHS = [110, 140, 170, 210, 260, 340];
  const setZoom = (next: number) => (prefs.zoom = Math.min(WIDTHS.length - 1, Math.max(0, next)));
  // Taps flip the chip at once; the server's answer replaces this guess.
  let optimistic = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});

  // The open menu (a sheet, decision 0021): a playlist's (lib/PlaylistMenu) or the "New playlist" one.
  type Menu = { kind: 'playlist'; p: Playlist; anchor: DOMRect } | { kind: 'new'; anchor: DOMRect };
  let menu = $state<Menu | null>(null);
  let nameField = $state('');
  let working = $state(false);

  const np = $derived(playing());
  const items = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    let list = shownPlaylists().map((p) => {
      const member = optimistic[p.id] ?? p.member;
      return { ...p, member, count: p.count + (member === p.member ? 0 : member ? 1 : -1) };
    });
    if (q) list = list.filter((p) => p.name.toLowerCase().includes(q));
    // Pinned ones first, in the order pinned; "✓ first" applies within the pinned and within the rest.
    const pin = (id: string) => {
      const i = prefs.pinned.indexOf(id);
      return i < 0 ? Infinity : i;
    };
    return [...list].sort((a, b) => {
      const [pa, pb] = [pin(a.id), pin(b.id)];
      if ((pa === Infinity) !== (pb === Infinity)) return pa === Infinity ? 1 : -1;
      if (prefs.membersFirst && a.member !== b.member) return Number(b.member) - Number(a.member);
      return pa === Infinity ? 0 : pa - pb;
    });
  });

  const memberCount = $derived(shownPlaylists().filter((p) => optimistic[p.id] ?? p.member).length);
  const NOT_MUSIC = "Not in your Music, so it can't go in playlists.";

  async function tap(p: Playlist) {
    if (busy[p.id]) return;
    if (!np) {
      toast.show({ text: 'Nothing is playing. Start a track to add it to playlists.', tone: 'warn' });
      return;
    }
    if (!np.addable && !p.member) {
      toast.show({ text: NOT_MUSIC, tone: 'warn' });
      return;
    }
    // The server's player is moving to another track: the tap waits for it (live.svelte.ts). The local track's
    // playlists are a moment away: the tap is not taken.
    if (pending()) {
      if (!isLocal()) queueTap(p.id);
      return;
    }
    busy[p.id] = true;
    optimistic[p.id] = !p.member;
    try {
      await setPlayingMember(p.id, !p.member);
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      delete optimistic[p.id];
      delete busy[p.id];
    }
  }

  const missing = (n: number) => `${n} missing ${n === 1 ? 'track' : 'tracks'}`;

  async function act(run: () => Promise<unknown>, done: Toast) {
    if (working) return;
    working = true;
    try {
      await run();
      toast.show(done);
      menu = null;
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      working = false;
      await refresh();
    }
  }

  function openNew(e: MouseEvent) {
    nameField = '';
    menu = { kind: 'new', anchor: (e.currentTarget as HTMLElement).getBoundingClientRect() };
  }

  const create = () => {
    const name = nameField.trim();
    if (name) act(() => createPlaylist(name), { text: `Created ${name}.` });
  };

</script>

<main>

  <div class="tools">
    <input type="search" placeholder="Filter playlists" aria-label="Filter playlists" bind:value={filter} />
    <div class="seg" role="group" aria-label="Order">
      <button aria-pressed={!prefs.membersFirst} onclick={() => (prefs.membersFirst = false)}>A–Z</button>
      <button aria-pressed={prefs.membersFirst} onclick={() => (prefs.membersFirst = true)} title="Playlists with the playing track first"
        >✓ first</button
      >
    </div>
    <div class="seg" role="group" aria-label="Size">
      <span class="lbl" aria-hidden="true">Size</span>
      <button aria-label="Smaller" disabled={prefs.zoom === 0} onclick={() => setZoom(prefs.zoom - 1)}>−</button>
      <button aria-label="Larger" disabled={prefs.zoom === WIDTHS.length - 1} onclick={() => setZoom(prefs.zoom + 1)}>+</button>
    </div>
    <button class="new" onclick={openNew}>New playlist</button>
  </div>

  {#if error}
    <Notice text={error} />
  {/if}

  {#if !view && !error}
    <p class="empty">Loading…</p>
  {:else}
    <ul class="grid" class:pending={pending()} style:--min="{WIDTHS[prefs.zoom]}px">
      {#each items as p (p.id)}
        <li>
          <!-- The label opens the playlist (decision 0023); the circle adds or removes the playing track. -->
          <div
            class="chip"
            class:member={p.member}
            class:busy={busy[p.id]}
            class:queued={live.queued.includes(p.id)}
            class:idle={!np}
            class:blocked={!!np && !np.addable && !p.member}
            title={p.dead > 0 ? missing(p.dead) : undefined}
            use:longpress={(anchor) => (menu = { kind: 'playlist', p, anchor })}
          >
            <button class="label" aria-label={`Open ${p.name}`} onclick={() => openPlaylist(p.id)}>
              <span class="name">{p.name}</span>
              {#if prefs.pinned.includes(p.id)}<span class="pinned" title="Pinned"><Icon name="pin" size={13} /></span>{/if}
              {#if p.links.some((l) => l.target !== 'shared')}<span class="own" title={p.links.map((l) => l.label).join(', ')}><Icon name="user" size={13} /></span>{/if}
              <span class="n">{p.count}</span>
            <!-- A favourite's small heart above the problems' dot, in the dot's place: the chip gets no wider. -->
            {#if isPlaylistFavorite(p.id) || p.dead > 0}
              <span class="flags">
                {#if isPlaylistFavorite(p.id)}<span class="heart" aria-label="Favourite"><Icon name="heart" size={10} /></span>{/if}
                {#if p.dead > 0}<i class="dead" aria-label={missing(p.dead)}></i>{/if}
              </span>
            {/if}
            </button>
            <span class="sep" aria-hidden="true"></span>
            <button
              class="state"
              aria-pressed={p.member}
              aria-label={p.member ? `Take the playing track out of ${p.name}` : `Add the playing track to ${p.name}`}
              onclick={() => tap(p)}
              ><Icon name={p.member ? 'circleCheck' : np && !np.addable ? 'circleDashedPlus' : 'circlePlus'} size={26} /></button
            >
          </div>
        </li>
      {/each}
    </ul>
    {#if view && items.length === 0}<p class="empty">No playlists match.</p>{/if}
  {/if}
</main>

{#if menu?.kind === 'new'}
  <MenuSheet label="New playlist" title="New playlist" onclose={() => (menu = null)}>
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        create();
      }}
    >
      <input type="text" placeholder="Name" aria-label="Name of the new playlist" maxlength="100" bind:value={nameField} />
      <button type="submit" class="primary" disabled={working || !nameField.trim()} aria-busy={working}>Create playlist</button>
    </form>
  </MenuSheet>
{:else if menu?.kind === 'playlist'}
  <PlaylistMenu playlist={menu.p} onclose={() => (menu = null)} />
{/if}

<style>
  main {
    padding: 0 clamp(12px, 2vw, 32px) 48px;
  }

  .tools {
    display: flex;
    gap: 12px;
    margin: 14px 0;
    flex-wrap: wrap;
  }

  input[type='search'],
  input[type='text'] {
    min-height: 44px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  input[type='search'] {
    flex: 1 1 220px;
    background: var(--surface);
  }

  .seg {
    display: flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--surface);
  }

  .seg .lbl {
    padding: 0 4px 0 14px;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .seg button {
    min-height: 44px;
    padding: 0 16px;
    background: var(--surface);
    border: 0;
    cursor: pointer;
  }

  .seg button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .seg button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button.new {
    min-height: 44px;
    padding: 0 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    cursor: pointer;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--min, 170px), 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Finamp's add-to-playlist entries (its toggleable list tile): tinted in the accent, 10 % when the playing track is not
     in the playlist and 30 % when it is; the name, the count, a divider and the check or plus circle. */
  .chip {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 48px;
    padding: 6px 8px 6px 12px;
    text-align: left;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border: 0;
    border-radius: 12px;
    color: var(--text);
    cursor: pointer;
    transition:
      background 120ms,
      transform 80ms;
  }

  .chip:active {
    transform: scale(0.97);
  }

  .chip.member {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }

  /* Nothing playing: the chips only open their menu, so they look quieter. */
  .chip.idle {
    opacity: 0.6;
  }

  .chip.busy {
    opacity: 0.7;
  }

  /* The player is moving to another track: its playlists are not known yet, so the chips are dimmed, and a tapped one
     pulses until the tap goes through (live.svelte.ts). */
  .grid.pending .chip:not(.queued) {
    opacity: 0.45;
  }

  .chip.queued {
    outline: 1.5px solid var(--accent);
    animation: waiting 1s ease-in-out infinite alternate;
  }

  @keyframes waiting {
    from {
      opacity: 1;
    }
    to {
      opacity: 0.55;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .chip.queued {
      animation: none;
    }
  }

  /* Track outside Music: adding is refused, so the add chips are quieter and their plus circle dashed (Finamp's mark
     for a playlist it cannot add to); tapping explains why. */
  .chip.blocked {
    opacity: 0.6;
  }

  .sep {
    flex: none;
    width: 1.5px;
    height: 24px;
    background: color-mix(in srgb, var(--text) 35%, transparent);
  }

  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    align-self: stretch;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .state {
    flex: none;
    display: flex;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  /* Kept in an account's own playlists too (decision 0014). */
  .own,
  .pinned {
    display: flex;
    flex: none;
    opacity: 0.65;
  }

  .n {
    flex: none;
    margin-left: auto;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.65;
  }

  .flags {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    width: 10px;
    margin-left: -4px;
  }

  .heart {
    display: flex;
    color: #e5484d;
  }

  .heart :global(svg) {
    fill: currentColor;
  }

  .dead {
    display: block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warn);
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }

  /* The "New playlist" sheet's form */

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .form input {
    min-height: 44px;
    padding: 0 12px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .form button.primary {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .form button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
