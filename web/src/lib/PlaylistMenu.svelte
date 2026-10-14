<script lang="ts">
  // A playlist's menu (docs/ui-conventions.md, decision 0021), the same wherever a playlist is long-pressed: a sheet as
  // Finamp's track menu, the playlist on its card (a mosaic of its first albums' covers), Play, Shuffle, Play next and
  // Add to queue as the large actions, then Sync to…, Spoken as…, Rename and Delete, each done inside the sheet.
  // Success shows where it happens (the header, the list); only Delete says something, to offer Undo.
  import {
    deletePlaylist,
    getTargets,
    linkPlaylist,
    restorePlaylist,
    setSpoken,
    spokenNames,
    unlinkPlaylist,
    type Conflict,
    type Direction,
    type Link,
    type PlayMode,
    type TargetChoice,
    incomingCoverUrl,
    playlistCovers,
  } from './api';
  import MenuEntry from './MenuEntry.svelte';
  import { startPlaylist } from './output.svelte';
  import { openPlaylist, panel, playlistDeleted } from './panel.svelte';
  import { renameEverywhere } from './rename.svelte';
  import { insights } from './insights.svelte';
  import { isPlaylistFavorite, loadPlaylistFavorites, togglePlaylistFavorite } from './favourites.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { prefs, togglePin } from './prefs.svelte';
  import { refresh } from './live.svelte';
  import { toast } from './toast.svelte';

  let {
    playlist,
    onclose,
    onrenamed,
    ondeleted,
    onrestored,
  }: {
    playlist: { id: string; name: string; count: number; dead: number; links?: Link[]; spoken?: string[] };
    onclose: () => void;
    onrenamed?: (from: string, to: string) => void;
    ondeleted?: (id: string) => void;
    /** The deletion was undone. */
    onrestored?: (id: string) => void;
  } = $props();

  let mode = $state<'menu' | 'rename' | 'delete' | 'sync' | 'spoken'>('menu');
  $effect(() => void loadPlaylistFavorites());

  // The card's image: the covers of the playlist's first four albums (one cover when it has fewer).
  let covers = $state<string[]>([]);
  $effect(() => {
    const id = playlist.id;
    let gone = false;
    playlistCovers(id)
      .then((list) => {
        if (gone) return;
        const urls = list.map((c) => incomingCoverUrl(c.source, c.album_id));
        covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
      })
      .catch(() => {});
    return () => (gone = true);
  });

  // Spoken as… (decision 0015): other names a voice assistant may hear for it, comma-separated.
  let spokenField = $state('');
  async function saveSpoken() {
    if (working) return;
    working = true;
    try {
      await setSpoken(p.id, spokenNames(spokenField));
      onclose();
    } catch (e) {
      fail(e, 'Could not save the names');
    } finally {
      working = false;
      refresh();
    }
  }

  // Sync to… (decision 0014): every target, on or off, and how it is kept. Each change is made at once.
  let targets = $state<TargetChoice[] | null>(null);
  let links = $state<Link[]>([]);
  async function openSync() {
    mode = 'sync';
    links = [...(playlist.links ?? [])];
    try {
      targets = await getTargets();
    } catch (e) {
      fail(e, 'Could not list the targets');
      mode = 'menu';
    }
  }
  const linkOf = (target: string) => links.find((l) => l.target === target);
  async function setLink(t: TargetChoice, on: boolean, direction: Direction = 'push', conflict: Conflict = 'newest') {
    if (working) return;
    working = true;
    const before = links;
    links = on
      ? [...links.filter((l) => l.target !== t.target), { target: t.target, label: t.label, direction, conflict, status: null }]
      : links.filter((l) => l.target !== t.target);
    try {
      if (on) await linkPlaylist(p.id, t.target, direction, conflict);
      else await unlinkPlaylist(p.id, t.target);
    } catch (e) {
      links = before;
      fail(e, 'Could not change the link');
    } finally {
      working = false;
      refresh();
    }
  }
  let nameField = $state('');
  let working = $state(false);
  const p = $derived(playlist);
  const missing = (n: number) => `${n} missing ${n === 1 ? 'track' : 'tracks'}`;
  const fail = (e: unknown, what: string) => toast.fail(e, what);

  async function play(how: PlayMode) {
    if (working) return;
    working = true;
    onclose();
    try {
      // Here when the output is this browser (decision 0022), else on the server's player.
      await startPlaylist(p.id, how);
    } catch (e) {
      fail(e, 'Could not start playing');
    } finally {
      working = false;
    }
  }

  async function rename() {
    const to = nameField.trim();
    if (!to || to === p.id || working) return;
    working = true;
    try {
      await renameEverywhere(p.id, to);
      onrenamed?.(p.id, to);
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      working = false;
      refresh();
    }
  }

  async function remove() {
    if (working) return;
    working = true;
    const { id, name } = p;
    try {
      await deletePlaylist(id);
      playlistDeleted(id);
      insights.deleted(id);
      ondeleted?.(id);
      toast.show({
        text: `Deleted ${name}.`,
        undo: async () => {
          try {
            await restorePlaylist(id);
            insights.restored(id);
            onrestored?.(id);
          } catch (e) {
            fail(e, 'Could not restore the playlist');
          } finally {
            refresh();
          }
        },
      });
      onclose();
    } catch (e) {
      fail(e, 'Something went wrong');
    } finally {
      working = false;
      refresh();
    }
  }
</script>

<MenuSheet
  label={p.name}
  {onclose}
  images={covers}
  title={p.name}
  chips={[
    `${p.count} ${p.count === 1 ? 'track' : 'tracks'}`,
    ...(p.dead > 0 ? [missing(p.dead)] : []),
    ...(p.links ?? []).map((l) => l.label),
  ]}
  actions={mode === 'menu'
    ? [
        { icon: 'play', label: 'Play', onclick: () => play('play'), disabled: working },
        { icon: 'shuffle', label: 'Shuffle', onclick: () => play('shuffle'), disabled: working },
        { icon: 'playNext', label: 'Play next', onclick: () => play('next'), disabled: working },
        { icon: 'playlists', label: 'Add to queue', onclick: () => play('last'), disabled: working },
      ]
    : []}
  onback={mode === 'menu' ? null : () => (mode = 'menu')}
  step={mode}
>
  {#if mode === 'menu'}
    <div role="menu">
      {#if panel.open !== p.id}
        <MenuEntry
          icon="listOpen"
          label="Open"
          onclick={() => {
            openPlaylist(p.id);
            onclose();
          }}
        />
      {/if}
      <MenuEntry
        icon="heart"
        label={isPlaylistFavorite(p.id) ? 'Remove from favourites' : 'Add to favourites'}
        onclick={() => {
          togglePlaylistFavorite(p.id);
          onclose();
        }}
      />
      <MenuEntry
        icon="pin"
        label={prefs.pinned.includes(p.id) ? 'Unpin' : 'Pin to top'}
        onclick={() => {
          togglePin(p.id);
          onclose();
        }}
      />
      <MenuEntry icon="link" label="Sync to…" onclick={openSync} />
      <MenuEntry
        icon="mic"
        label="Spoken as…"
        onclick={() => {
          spokenField = (p.spoken ?? []).join(', ');
          mode = 'spoken';
        }}
      />
      <MenuEntry
        icon="pencil"
        label="Rename"
        onclick={() => {
          nameField = p.id;
          mode = 'rename';
        }}
      />
      <MenuEntry icon="trash" label="Delete" danger onclick={() => (mode = 'delete')} />
    </div>
  {:else if mode === 'rename'}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        rename();
      }}
    >
      <div class="heading">Rename</div>
      <input type="text" aria-label="New name" maxlength="100" bind:value={nameField} />
      <button type="submit" class="primary" disabled={working || !nameField.trim() || nameField.trim() === p.id} aria-busy={working}>Rename</button>
    </form>
  {:else if mode === 'spoken'}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        saveSpoken();
      }}
    >
      <div class="heading">Spoken as</div>
      <input type="text" aria-label="Other names, comma-separated" placeholder="japanese, kokoro" bind:value={spokenField} />
      <button type="submit" class="primary" disabled={working} aria-busy={working}>Save</button>
    </form>
  {:else if mode === 'sync'}
    <div class="form">
      <div class="heading">Sync to</div>
      {#if !targets}
        <p class="ask">…</p>
      {:else}
        {#each targets as t (t.target)}
          {@const l = linkOf(t.target)}
          <div class="target">
            <label class="on"><input type="checkbox" checked={!!l} disabled={working} onchange={(e) => setLink(t, e.currentTarget.checked)} />{t.label}</label>
            {#if l && !t.push_only}
              <select
                aria-label="Direction"
                value={l.direction}
                disabled={working}
                onchange={(e) => setLink(t, true, e.currentTarget.value as Direction, l.conflict)}
              >
                <option value="push">Push</option>
                <option value="two_way">Two-way</option>
              </select>
              {#if l.direction === 'two_way'}
                <select
                  aria-label="When both changed"
                  value={l.conflict}
                  disabled={working}
                  onchange={(e) => setLink(t, true, l.direction, e.currentTarget.value as Conflict)}
                >
                  <option value="newest">Newest wins</option>
                  <option value="playlist">Cratarr wins</option>
                  <option value="target">{t.label} wins</option>
                </select>
              {/if}
            {/if}
            {#if l?.status?.error}<p class="error">{l.status.error}</p>{/if}
          </div>
        {/each}
      {/if}
      <button class="plain" onclick={onclose}>Done</button>
    </div>
  {:else}
    <div class="form">
      <p class="ask">Delete {p.name}? You can undo this right after.</p>
      <button class="danger solid" disabled={working} aria-busy={working} onclick={remove}>Delete playlist</button>
    </div>
  {/if}
</MenuSheet>

<style>
  .heading {
    font-size: 18px;
    text-align: center;
  }

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .form input[type='text'] {
    min-height: 44px;
    padding: 0 12px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .ask {
    margin: 0;
    font-size: 0.95rem;
    text-align: center;
  }

  /* Sync to…: one line per target, its settings beside it once linked. */
  .target {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    min-height: 40px;
  }

  .target .on {
    display: flex;
    flex: 1 1 140px;
    align-items: center;
    gap: 8px;
    overflow-wrap: anywhere;
  }

  .target select {
    min-height: 34px;
    padding: 0 8px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: 8px;
    font: inherit;
    font-size: 0.85rem;
    color: inherit;
  }

  .target .error {
    flex-basis: 100%;
    margin: 0;
    color: var(--danger);
    font-size: 0.8rem;
  }

  .form button {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    font: inherit;
    cursor: pointer;
  }

  .form button.primary {
    background: var(--art-accent, var(--accent));
    color: var(--accent-text);
    font-weight: 600;
  }

  .form button.plain {
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: inherit;
  }

  .form button.danger.solid {
    background: var(--danger);
    color: #fff;
    font-weight: 600;
  }

  .form button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
