<script lang="ts">
  // A playlist's menu (docs/ui-conventions.md), the same wherever a playlist is long-pressed: Play, Shuffle, Play next,
  // Add to queue, then Rename and Delete, both done inside the popover. Success shows where it happens (the header,
  // the list); only Delete says something, to offer Undo.
  import { ApiError, deletePlaylist, playPlaylist, renamePlaylist, restorePlaylist, type PlayMode } from './api';
  import type { NoticeData } from './Notice.svelte';
  import Popover from './Popover.svelte';
  import { prefs } from './prefs.svelte';
  import { refresh } from './live.svelte';

  let {
    playlist,
    anchor,
    onclose,
    onnotice,
    onrenamed,
    ondeleted,
    onrestored,
  }: {
    playlist: { id: string; name: string; count: number; dead: number };
    anchor: DOMRect;
    onclose: () => void;
    onnotice: (n: NoticeData) => void;
    onrenamed?: (from: string, to: string) => void;
    ondeleted?: (id: string) => void;
    /** The deletion was undone. */
    onrestored?: (id: string) => void;
  } = $props();

  let mode = $state<'menu' | 'rename' | 'delete'>('menu');
  let nameField = $state('');
  let working = $state(false);
  const p = $derived(playlist);
  const missing = (n: number) => `${n} missing ${n === 1 ? 'track' : 'tracks'}`;
  const fail = (e: unknown, what: string) => onnotice({ text: e instanceof ApiError ? e.message : what, tone: 'error' });

  async function play(how: PlayMode) {
    if (working) return;
    working = true;
    onclose();
    try {
      await playPlaylist(p.id, how, prefs.source || undefined);
      refresh();
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
      onnotice({
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
  <div class="head">{p.name} <span>{p.count} {p.count === 1 ? 'track' : 'tracks'}{p.dead > 0 ? ` · ${missing(p.dead)}` : ''}</span></div>
  {#if mode === 'menu'}
    <div class="items" role="menu">
      <button role="menuitem" disabled={working} onclick={() => play('play')}>Play</button>
      <button role="menuitem" disabled={working} onclick={() => play('shuffle')}>Shuffle</button>
      <button role="menuitem" disabled={working} onclick={() => play('next')}>Play next</button>
      <button role="menuitem" disabled={working} onclick={() => play('last')}>Add to queue</button>
      <hr />
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
        <button type="submit" class="primary" disabled={working || !nameField.trim() || nameField.trim() === p.id}>Rename</button>
        <button type="button" onclick={onclose}>Cancel</button>
      </div>
    </form>
  {:else}
    <div class="form">
      <p class="ask">Delete {p.name}? You can undo this right after.</p>
      <div class="row">
        <button class="danger solid" disabled={working} onclick={remove}>Delete playlist</button>
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
