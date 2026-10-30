<script lang="ts">
  // The queue's menu (decision 0058; the owner, 2026-10-23): the Queue sheet's ⋮ beside its search, and a long-press on
  // the full player's Queue button. Shuffle and Repeat at its header's end (the owner, 2026-10-26: Repeat out of the
  // player's row, 0.9.8; Repeat off, all, one, lit while on; once: a queue always has an order, Undo puts the order before back), and under the name what the queue is
  // made of: how many of its tracks Radio put in and how many Autoplay serves; then in the table's order (lib/menuTable)
  // Save as playlist; Download…; Queues… (the switcher); Clear the queue last (with its Undo).
  import MenuSheet from './MenuSheet.svelte';
  import { port } from './player/port.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuTable from './MenuTable.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import { BANDCAMP } from './bandcampPreview';
  import type { EntryId } from './menuTable';
  import QueuePlaylistSheet from './QueuePlaylistSheet.svelte';
  import QueuesSheet from './QueuesSheet.svelte';
  import { clearUpcoming, local, restoreUpcoming, setRepeat, shuffleUpcoming } from './localPlayer.svelte';
  import { playing } from './remote.svelte';
  import { plural } from './insights.svelte';
  import { toast } from './toast.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const np = $derived(playing());
  const held = $derived(port().queue());
  const here = $derived(held !== null);
  const name = $derived(held?.from ?? 'Queue');
  // What it holds from the playing track on: this page's own, or what the server's player said.
  const listed = $derived(held ? held.tracks.slice(Math.max(0, held.index)) : np ? [np, ...np.upcoming] : []);
  // Bandcamp's previews are never downloaded (decision 0071): Download… leaves them out.
  const kept = $derived(
    listed
      .map((t) => ({ source: 'source' in t ? t.source : np!.source, id: t.id, name: t.name, artist: t.artist ?? null, album: t.album ?? null, album_id: t.album_id ?? null }))
      .filter((t) => t.source !== BANDCAMP),
  );
  const tracks = $derived(listed.map((t) => ({ source: 'source' in t ? t.source : np!.source, id: t.id })));
  const fromRadio = $derived(listed.filter((t) => 'lane' in t && t.lane === 'radio').length);
  const fromAutoplay = $derived(listed.filter((t) => 'lane' in t && t.lane === 'autoplay').length);
  const upcoming = $derived(held ? held.tracks.length - held.index - 1 : (np?.upcoming.length ?? 0));

  let step = $state<'menu' | 'download'>('menu');
  let saving = $state(false);
  let switching = $state(false);

  function shuffle() {
    const before = shuffleUpcoming();
    onclose();
    if (before) toast.show({ text: 'Shuffled.', tone: 'done', act: { label: 'Undo', run: () => restoreUpcoming(before) } });
  }
  function clear() {
    const removed = clearUpcoming();
    onclose();
    if (removed.length) toast.show({ text: 'Queue cleared.', tone: 'done', act: { label: 'Undo', run: () => restoreUpcoming(removed) } });
  }
</script>

{#snippet entry(e: EntryId)}
  {#if e === 'saveAsPlaylist'}
    <MenuEntry icon="intoBox" label="Save as playlist" onclick={() => (saving = true)} />
  {:else if e === 'download'}
    {#if kept.length}<MenuEntry icon="arrowDownToLine" label="Download…" onclick={() => (step = 'download')} />{/if}
  {:else if e === 'queues'}
    <MenuEntry icon="list" label="Queues…" onclick={() => (switching = true)} />
  {:else if e === 'clearQueue'}
    {#if here}<MenuEntry icon="clearQueue" label="Clear the queue" danger disabled={upcoming < 1} onclick={clear} />{/if}
  {/if}
{/snippet}

{#if saving}
  <QueuePlaylistSheet {onclose} />
{:else if switching}
  <QueuesSheet {onclose} />
{:else}
  <MenuSheet
    label={name}
    {onclose}
    icon="list"
    title={name}
    chips={[plural(tracks.length, 'track'), ...(fromRadio ? [`${fromRadio} from Radio`] : []), ...(fromAutoplay ? [`${fromAutoplay} from Autoplay`] : [])]}
    end={step === 'menu' && here
      ? [
          { icon: 'shuffle', label: 'Shuffle', disabled: upcoming < 2, onclick: shuffle },
          {
            icon: local.repeat === 'one' ? 'repeatOne' : 'repeat',
            label: local.repeat === 'one' ? 'Repeat one' : local.repeat === 'all' ? 'Repeat all' : 'Repeat',
            on: local.repeat !== 'off',
            onclick: () => setRepeat(local.repeat === 'off' ? 'all' : local.repeat === 'all' ? 'one' : 'off'),
          },
        ]
      : null}
    onback={step === 'menu' ? null : () => (step = 'menu')}
    {step}
  >
    {#if step === 'download'}
      <DownloadStep what={{ kind: 'tracks', name, tracks: kept.map((t) => ({ source: t.source, id: t.id })) }} tracks={kept} {onclose} />
    {:else}
      <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
      <MenuTable kind="queue" {entry} />
    {/if}
  </MenuSheet>
{/if}
