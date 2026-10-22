<script lang="ts">
  // A heart's long press (the owner, 2026-10-19): Favourite (its switch), the Excludes that apply where it was pressed
  // (a crate's: the track, its album; none in the Queue sheet), and Exclude globally (decision 0024, amended). The same
  // sheet from every heart: the full and the mini player's, the queue's playing card's, a track row's.
  import type { ExcludeEntry } from './crates.svelte';
  import { excludeGlobally } from './exclusions.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { toast } from './toast.svelte';

  let {
    track,
    cover = null,
    favourite,
    outs = [],
    onclose,
  }: {
    track: { source: string; id: string; name: string; artist: string | null };
    cover?: string | null;
    /** The heart as it stands there, and what flips it. */
    favourite: { on: boolean; disabled?: boolean; flip: () => void };
    outs?: ExcludeEntry[];
    onclose: () => void;
  } = $props();

  function everywhere() {
    const t = track;
    onclose();
    excludeGlobally(t.source, [t.id]).catch((e) => toast.fail(e, "It wasn't excluded"));
  }
</script>

<MenuSheet label="Favourite and exclude" title={track.name} images={cover ? [cover] : []} chips={track.artist ? [track.artist] : []} {onclose}>
  <div role="menu">
    <MenuEntry icon="heart" label="Favourite" switched={favourite.on} disabled={favourite.disabled} onclick={favourite.flip} />
    {#each outs as o (o.label)}
      <MenuEntry
        icon={o.icon}
        label={o.label}
        danger
        onclick={() => {
          onclose();
          o.run();
        }}
      />
    {/each}
    <MenuEntry icon="ban" label="Exclude everywhere" danger onclick={everywhere} />
  </div>
</MenuSheet>
