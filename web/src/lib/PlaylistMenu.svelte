<script lang="ts">
  // A playlist's menu (docs/ui-conventions.md), the same wherever a playlist is long-pressed: Play, Shuffle, Play next,
  // Add to queue, then Sync to…, Rename and Delete, each done inside the popover. Success shows where it happens (the
  // header, the list); only Delete says something, to offer Undo.
  import {
    deletePlaylist,
    getTargets,
    linkPlaylist,
    playPlaylist,
    renamePlaylist,
    restorePlaylist,
    setSpoken,
    unlinkPlaylist,
    type Conflict,
    type Direction,
    type Link,
    type PlayMode,
    type TargetChoice,
  } from './api';
  import Popover from './Popover.svelte';
  import { prefs } from './prefs.svelte';
  import { refresh, untilNewTrack } from './live.svelte';
  import { toast } from './toast.svelte';

  let {
    playlist,
    anchor,
    onclose,
    onrenamed,
    ondeleted,
    onrestored,
  }: {
    playlist: { id: string; name: string; count: number; dead: number; links?: Link[]; spoken?: string[] };
    anchor: DOMRect;
    onclose: () => void;
    onrenamed?: (from: string, to: string) => void;
    ondeleted?: (id: string) => void;
    /** The deletion was undone. */
    onrestored?: (id: string) => void;
  } = $props();

  let mode = $state<'menu' | 'rename' | 'delete' | 'sync' | 'spoken'>('menu');

  // Spoken as… (decision 0015): other names a voice assistant may hear for it, comma-separated.
  let spokenField = $state('');
  async function saveSpoken() {
    if (working) return;
    working = true;
    try {
      await setSpoken(p.id, spokenField.split(',').map((s) => s.trim()).filter(Boolean));
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
      await playPlaylist(p.id, how, prefs.source || undefined);
      // Play and Shuffle change what plays: show the new track once it is there, not the states on the way.
      if (how === 'play' || how === 'shuffle') await untilNewTrack();
      else refresh();
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
      await renamePlaylist(p.id, to);
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
      ondeleted?.(id);
      toast.show({
        text: `Deleted ${name}.`,
        undo: async () => {
          try {
            await restorePlaylist(id);
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

<Popover {anchor} label={p.name} {onclose}>
  <div class="head">
    {p.name}
    <span
      >{p.count} {p.count === 1 ? 'track' : 'tracks'}{p.dead > 0 ? ` · ${missing(p.dead)}` : ''}{mode === 'sync' ? '' : (p.links ?? []).map((l) => ` · ${l.label}`).join('')}</span
    >
  </div>
  {#if mode === 'menu'}
    <div class="items" role="menu">
      <button role="menuitem" disabled={working} onclick={() => play('play')}>Play</button>
      <button role="menuitem" disabled={working} onclick={() => play('shuffle')}>Shuffle</button>
      <button role="menuitem" disabled={working} onclick={() => play('next')}>Play next</button>
      <button role="menuitem" disabled={working} onclick={() => play('last')}>Add to queue</button>
      <hr />
      <button role="menuitem" onclick={openSync}>Sync to…</button>
      <button
        role="menuitem"
        onclick={() => {
          spokenField = (p.spoken ?? []).join(', ');
          mode = 'spoken';
        }}>Spoken as…</button
      >
      <button
        role="menuitem"
        onclick={() => {
          nameField = p.id;
          mode = 'rename';
        }}>Rename</button
      >
      <button role="menuitem" class="danger" onclick={() => (mode = 'delete')}>Delete</button>
    </div>
  {:else if mode === 'rename'}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        rename();
      }}
    >
      <input type="text" aria-label="New name" maxlength="100" bind:value={nameField} />
      <div class="row">
        <button type="submit" class="primary" disabled={working || !nameField.trim() || nameField.trim() === p.id} aria-busy={working}>Rename</button>
        <button type="button" onclick={onclose}>Cancel</button>
      </div>
    </form>
  {:else if mode === 'spoken'}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        saveSpoken();
      }}
    >
      <input type="text" aria-label="Other names, comma-separated" placeholder="japanese, kokoro" bind:value={spokenField} />
      <div class="row">
        <button type="submit" class="primary" disabled={working} aria-busy={working}>Save</button>
        <button type="button" onclick={onclose}>Cancel</button>
      </div>
    </form>
  {:else if mode === 'sync'}
    <div class="form">
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
      <div class="row"><button onclick={onclose}>Done</button></div>
    </div>
  {:else}
    <div class="form">
      <p class="ask">Delete {p.name}? You can undo this right after.</p>
      <div class="row">
        <button class="danger solid" disabled={working} aria-busy={working} onclick={remove}>Delete playlist</button>
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {/if}
</Popover>

<style>
  .head {
    padding: 8px 10px 6px;
    font-weight: 650;
    overflow-wrap: anywhere;
  }

  .head span {
    display: block;
    color: var(--muted);
    font-size: 0.8rem;
    font-weight: 400;
  }

  .items {
    display: grid;
  }

  .items button {
    min-height: 44px;
    padding: 0 10px;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 8px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .items hr {
    margin: 4px 6px;
    border: 0;
    border-top: 1px solid var(--border);
  }

  .items button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .items button:hover,
  .items button:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .danger,
  .items button.danger {
    color: var(--danger);
  }

  .form {
    display: grid;
    gap: 10px;
    padding: 6px;
  }

  .form input {
    min-height: 40px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .ask {
    margin: 0;
    font-size: 0.92rem;
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
    background: var(--bg);
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

  .row button.danger.solid {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
    font-weight: 600;
  }

  .row button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
