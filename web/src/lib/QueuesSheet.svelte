<script lang="ts">
  // The queues (decision 0033, amended: Symfonium's queue switcher; the owner, 2026-10-19: "build it as decided"): every
  // play's queue kept whole, newest first, the one that plays on top and tinted; each with its name, how many tracks and
  // when it was left. A tap takes it up where it was left (playing if it played); a long-press or its ⋮ is its menu, in
  // the shared groups (decision 0058): Play from here as its large action, Save as playlist (the one that plays),
  // Rename, and Delete (not the one that plays) last. Whose queues these are lib/queueHistory decides (this browser's own,
  // the player it drives, or the app's). Reached from the queue's head (lib/QueueSheet). No notice on success; an error
  // in one line. After when, where it was heard (amended 2026-10-19): Play On's icon and the outputs' own names (the
  // car's Bluetooth), else the players'.
  import type { QueueItem } from './api';
  import Icon, { type IconName } from './Icon.svelte';
  import { longpress } from './longpress';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import QueuePlaylistSheet from './QueuePlaylistSheet.svelte';
  import { deleteQueue, loadQueues, queues, queuesPlayer, queuesVersion, renameQueue, switchQueue } from './queueHistory.svelte';
  import Row from './Row.svelte';
  import { when } from './settings/when';

  let { onclose }: { onclose: () => void } = $props();

  // Read when opened, and again whenever the player says its list changed.
  $effect(() => {
    void queuesVersion();
    loadQueues();
  });

  const FROM: Record<string, IconName> = { playlist: 'playlists', album: 'albums', favourites: 'heart', library: 'library', tracks: 'music', player: 'speaker' };
  const line = (q: QueueItem) =>
    [q.playing ? 'Playing' : null, `${q.tracks} ${q.tracks === 1 ? 'track' : 'tracks'}`, q.playing ? null : when(q.updated)].filter(Boolean).join(' · ');
  /** Where it was heard: an output's own name, else its player's; not this list's own player on its own speaker. */
  const playedOn = (q: QueueItem) => {
    const own = queuesPlayer();
    return [...new Set((q.played_on ?? []).flatMap((o) => (o.name ? [o.name] : o.player === own ? [] : [o.player])))].join(', ');
  };

  // A queue's menu (a step of this sheet), and its new name (a sheet of its own, from the top: the keyboard).
  let menuFor = $state<QueueItem | null>(null);
  let renaming = $state<QueueItem | null>(null);
  /** Save as playlist: the queue that plays, as a new playlist (lib/QueuePlaylistSheet). */
  let saving = $state(false);
  let name = $state('');
  let busy = $state(false);

  async function take(q: QueueItem) {
    if (q.playing) return onclose();
    busy = true;
    await switchQueue(q.id);
    busy = false;
    if (!queues.error) onclose();
  }

  async function rename() {
    const q = renaming;
    if (!q || !name.trim()) return;
    busy = true;
    await renameQueue(q.id, name.trim());
    busy = false;
    if (!queues.error) renaming = null;
  }

  async function remove(q: QueueItem) {
    menuFor = null;
    await deleteQueue(q.id);
  }

  /** A long-press on a row as an attachment (lib/Row's `attach`). */
  const pressing = (q: QueueItem) => (el: HTMLElement) => longpress(el, () => (menuFor = q))?.destroy;
</script>

{#if saving}
  <QueuePlaylistSheet onclose={() => (saving = false)} />
{:else if !renaming}
  <MenuSheet
    label="Queues"
    icon="list"
    title={menuFor ? menuFor.name : 'Queues'}
    step={menuFor ? 'menu' : 'list'}
    onback={menuFor ? () => (menuFor = null) : null}
    actions={menuFor && !menuFor.playing ? [{ icon: 'play', label: 'Play from here', disabled: busy, onclick: () => menuFor && take(menuFor) }] : []}
    {onclose}
  >
    {#if menuFor}
      {@const q = menuFor}
      <MenuCards>
        <MenuGroup>
          {#if q.playing}
            <MenuEntry
              icon="intoBox"
              label="Save as playlist"
              onclick={() => {
                menuFor = null;
                saving = true;
              }}
            />
          {/if}
        </MenuGroup>
        <MenuGroup>
          <MenuEntry
            icon="pencil"
            label="Rename"
            onclick={() => {
              name = q.name;
              renaming = q;
              menuFor = null;
            }}
          />
        </MenuGroup>
        <MenuGroup>
          {#if !q.playing}<MenuEntry icon="trash" label="Delete" danger onclick={() => remove(q)} />{/if}
        </MenuGroup>
      </MenuCards>
    {:else}
      {#if queues.error}<p class="error" role="alert">{queues.error}</p>{/if}
      <ol class="queues" aria-busy={busy}>
        {#each queues.list as q (q.id)}
          <li>
            <Row size="medium" title={q.name} strong={q.playing} onclick={() => take(q)} label={q.playing ? q.name : `Take up ${q.name}`} attach={pressing(q)}>
              {#snippet sub()}{@const on = playedOn(q)}{line(q)}{#if on}<span class="on">{' · '}<Icon name="speaker" size={12} />{' '}{on}</span>{/if}{/snippet}
              {#snippet lead()}<span class="from"><Icon name={FROM[q.from ?? ''] ?? 'music'} size={22} /></span>{/snippet}
              {#snippet action()}<button class="more" aria-label={`Menu of ${q.name}`} title="Menu" onclick={() => (menuFor = q)}><Icon name="moreVert" size={22} /></button>{/snippet}
            </Row>
          </li>
        {:else}
          {#if queues.known}<li class="none">No queues yet</li>{/if}
        {/each}
      </ol>
    {/if}
  </MenuSheet>
{:else}
  <MenuSheet label="Rename" title="Rename" onclose={() => (renaming = null)}>
    <form class="form" onsubmit={(e) => (e.preventDefault(), rename())}>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="text" aria-label="Name of the queue" maxlength="100" autofocus bind:value={name} />
      <button type="submit" class="primary" disabled={busy || !name.trim()} aria-busy={busy}>Rename</button>
    </form>
    {#if queues.error}<p class="error" role="alert">{queues.error}</p>{/if}
  </MenuSheet>
{/if}

<style>
  .queues {
    display: grid;
    /* A long line ends in an ellipsis rather than widening the list. */
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    margin: 0;
    padding: 0 12px 8px;
    overflow-y: auto;
    list-style: none;
  }

  .from {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--muted);
  }

  .on :global(svg) {
    vertical-align: -1px;
  }

  .more {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .more:hover,
  .more:focus-visible {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .none {
    padding: 8px 12px 12px;
    color: var(--muted);
  }

  .error {
    margin: 0;
    padding: 0 24px 10px;
    color: var(--danger-ink);
    font-size: 14px;
  }

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .form input {
    min-height: 44px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    color: inherit;
    font: inherit;
  }

  .form .primary {
    min-height: 44px;
    border: 0;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .form .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
