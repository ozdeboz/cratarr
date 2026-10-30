<script lang="ts">
  // A work's menu (0.12, decision 0086; the owner, 2026-10-30: works reachable as everything else is): a work's heading
  // on an album, a work found in search. Play, Play next and Add to queue as its large actions: the movements given (an
  // album's, in its order), else the work's first recording's (lib/WorkSheet's order); then Go to the work, which opens
  // it (lib/WorkSheet) where it is not open already. No notice on success; an error in one line.
  import { getWork, type PlayHow } from './api';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import { playCollection } from './play.svelte';
  import { toast } from './toast.svelte';
  import { firstRecording } from './works';

  let {
    mbid,
    title,
    ids = null,
    onopen = null,
    onclose,
  }: {
    mbid: string;
    title: string;
    /** The movements to play (Cratarr's library), in order; none: the work's first recording's. */
    ids?: string[] | null;
    /** Go to the work: opens it; none where it is open already. */
    onopen?: (() => void) | null;
    onclose: () => void;
  } = $props();

  const SOURCE = 'cratarr';
  let busy = $state(false);

  /** The movements this menu plays: the ones given, else the first recording's in the work's order. */
  async function movements(): Promise<string[]> {
    return ids?.length ? ids : firstRecording(await getWork(mbid));
  }
  async function run(how: PlayHow) {
    busy = true;
    try {
      const list = await movements();
      if (!list.length) return void toast.show({ text: 'No recording of it in the library.', tone: 'error' });
      await playCollection({ kind: 'tracks', source: SOURCE, ids: list }, how, title, how === 'play' ? list[0] : undefined);
      onclose();
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      busy = false;
    }
  }
  const actions: SheetAction[] = $derived([
    { icon: 'play', label: 'Play', disabled: busy, onclick: () => run('play') },
    { icon: 'playNext', label: 'Play next', disabled: busy, onclick: () => run('next') },
    { icon: 'listAdd', label: 'Add to queue', disabled: busy, onclick: () => run('last') },
  ]);
</script>

<MenuSheet label={title} {title} {actions} {onclose}>
  {#if onopen}
    <MenuEntry
      icon="music"
      label="Go to the work"
      onclick={() => {
        const go = onopen;
        onclose();
        go?.();
      }}
    />
  {/if}
</MenuSheet>
