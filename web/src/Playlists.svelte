<script lang="ts">
  import { createPlaylist, setMember, type Playlist } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import { live, refresh } from './lib/live.svelte';
  import Notice from './lib/Notice.svelte';
  import { toast, type Toast } from './lib/toast.svelte';
  import Popover from './lib/Popover.svelte';
  import PlaylistMenu from './lib/PlaylistMenu.svelte';
  import { longpress } from './lib/longpress';

  // What is playing and the playlists come from the shell's poll (live.svelte.ts).
  const view = $derived(live.view);
  const error = $derived(live.error);
  let filter = $state('');
  // Size = chip width, which sets how many columns fit; one width per step in prefs.ZOOM_STEPS.
  const WIDTHS = [110, 140, 170, 210, 260, 340];
  const setZoom = (next: number) => (prefs.zoom = Math.min(WIDTHS.length - 1, Math.max(0, next)));
  // Taps flip the chip at once; the server's answer replaces this guess.
  let optimistic = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});

  // The open menu: a playlist's (lib/PlaylistMenu) or the "New playlist" one; anchored to what opened it.
  type Menu = { kind: 'playlist'; p: Playlist; anchor: DOMRect } | { kind: 'new'; anchor: DOMRect };
  let menu = $state<Menu | null>(null);
  let nameField = $state('');
  let working = $state(false);

  const np = $derived(view?.now_playing ?? null);
  const items = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    let list = (view?.playlists ?? []).map((p) => {
      const member = optimistic[p.id] ?? p.member;
      return { ...p, member, count: p.count + (member === p.member ? 0 : member ? 1 : -1) };
    });
    if (q) list = list.filter((p) => p.name.toLowerCase().includes(q));
    if (prefs.membersFirst) list = [...list].sort((a, b) => Number(b.member) - Number(a.member));
    return list;
  });

  const memberCount = $derived((view?.playlists ?? []).filter((p) => optimistic[p.id] ?? p.member).length);
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
    busy[p.id] = true;
    optimistic[p.id] = !p.member;
    try {
      live.view = await setMember(p.id, !p.member, prefs.source || undefined);
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
    <ul class="grid" style:--min="{WIDTHS[prefs.zoom]}px">
      {#each items as p (p.id)}
        <li>
          <button
            class="chip"
            class:member={p.member}
            class:busy={busy[p.id]}
            class:idle={!np}
            class:blocked={!!np && !np.addable && !p.member}
            aria-pressed={p.member}
            title={p.dead > 0 ? missing(p.dead) : undefined}
            onclick={() => tap(p)}
            use:longpress={(anchor) => (menu = { kind: 'playlist', p, anchor })}
          >
            <span class="mark" aria-hidden="true">{p.member ? '✓' : '+'}</span>
            <span class="name">{p.name}</span>
            <span class="n">{p.count}{#if p.dead > 0}<i class="dead" aria-label={missing(p.dead)}></i>{/if}</span>
          </button>
        </li>
      {/each}
    </ul>
    {#if view && items.length === 0}<p class="empty">No playlists match.</p>{/if}
  {/if}
</main>

{#if menu?.kind === 'new'}
  <Popover anchor={menu.anchor} label="New playlist" onclose={() => (menu = null)}>
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        create();
      }}
    >
      <input type="text" placeholder="Name" aria-label="Name of the new playlist" maxlength="100" bind:value={nameField} />
      <div class="row">
        <button type="submit" class="primary" disabled={working || !nameField.trim()}>Create playlist</button>
        <button type="button" onclick={() => (menu = null)}>Cancel</button>
      </div>
    </form>
  </Popover>
{:else if menu?.kind === 'playlist'}
  <PlaylistMenu playlist={menu.p} anchor={menu.anchor} onclose={() => (menu = null)} />
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

  .chip {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 56px;
    padding: 8px 14px;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    cursor: pointer;
    transition:
      background 120ms,
      border-color 120ms,
      transform 80ms;
  }

  .chip:active {
    transform: scale(0.97);
  }

  .chip.member {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  /* Nothing playing: the chips only open their menu, so they look quieter. */
  .chip.idle {
    opacity: 0.6;
  }

  .chip.busy {
    opacity: 0.7;
  }

  /* Track outside Music: adding is refused, so the add chips look blocked (tapping explains why). */
  .chip.blocked {
    opacity: 0.5;
    border-style: dashed;
  }

  .mark {
    flex: none;
    width: 1.2em;
    text-align: center;
    font-weight: 700;
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

  .n {
    flex: none;
    margin-left: auto;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.65;
  }

  .dead {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 5px;
    border-radius: 50%;
    background: var(--warn);
    vertical-align: middle;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }

  /* Popover contents */

  .form {
    display: grid;
    gap: 10px;
    padding: 6px;
  }

  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .row button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .row button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .row button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
