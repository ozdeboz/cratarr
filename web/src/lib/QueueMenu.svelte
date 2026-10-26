<script lang="ts">
  // The queue's menu (decision 0058; the owner, 2026-10-23): the Queue sheet's ⋮ beside its search, and a long-press on
  // the full player's Queue button. Shuffle at its header's end (the owner, 2026-10-26: beside Repeat once that moves
  // here; once: a queue always has an order, Undo puts the order before back), and under the name what the queue is
  // made of: how many of its tracks Radio put in and how many Autoplay serves; then Save as playlist; Export…; Queues…
  // (the switcher); Clear the queue last (with its Undo).
  import MenuSheet from './MenuSheet.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import ExportStep from './ExportStep.svelte';
  import QueuePlaylistSheet from './QueuePlaylistSheet.svelte';
  import QueuesSheet from './QueuesSheet.svelte';
  import { clearUpcoming, local, restoreUpcoming, shuffleUpcoming } from './localPlayer.svelte';
  import { isLocal } from './output.svelte';
  import { playing } from './remote.svelte';
  import { plural } from './insights.svelte';
  import { toast } from './toast.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const np = $derived(playing());
  const here = $derived(isLocal());
  const name = $derived((here ? local.from : null) ?? 'Queue');
  // What it holds from the playing track on: this page's own, or what the server's player said.
  const listed = $derived(here ? local.queue.slice(Math.max(0, local.index)) : np ? [np, ...np.upcoming] : []);
  const tracks = $derived(listed.map((t) => ({ source: 'source' in t ? t.source : np!.source, id: t.id })));
  const fromRadio = $derived(listed.filter((t) => 'lane' in t && t.lane === 'radio').length);
  const fromAutoplay = $derived(listed.filter((t) => 'lane' in t && t.lane === 'autoplay').length);
  const upcoming = $derived(here ? local.queue.length - local.index - 1 : (np?.upcoming.length ?? 0));

  let step = $state<'menu' | 'export'>('menu');
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
    end={step === 'menu' && here ? { icon: 'shuffle', label: 'Shuffle', disabled: upcoming < 2, onclick: shuffle } : null}
    onback={step === 'menu' ? null : () => (step = 'menu')}
    {step}
  >
    {#if step === 'export'}
      <ExportStep what={{ kind: 'tracks', name, tracks }} {onclose} />
    {:else}
      <!-- The shared groups, in their order (decision 0058). -->
      <MenuCards>
        <MenuGroup>
          <MenuEntry icon="intoBox" label="Save as playlist" onclick={() => (saving = true)} />
        </MenuGroup>
        <MenuGroup>
          {#if tracks.length}<MenuEntry icon="export" label="Export…" onclick={() => (step = 'export')} />{/if}
        </MenuGroup>
        <MenuGroup>
          <MenuEntry icon="list" label="Queues…" onclick={() => (switching = true)} />
        </MenuGroup>
        <MenuGroup>
          {#if here}<MenuEntry icon="clearQueue" label="Clear the queue" danger disabled={upcoming < 1} onclick={clear} />{/if}
        </MenuGroup>
      </MenuCards>
    {/if}
  </MenuSheet>
{/if}
