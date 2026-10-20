<script lang="ts">
  // A cable's long press (decision 0056; the owner: "long press to get the option to delete or reroute"): the app's own
  // menu (lib/MenuSheet.svelte, as a track's or an album's), in the graph and the rack alike.
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  let {
    title,
    onremove,
    onreroute,
    onclose,
    pinned = false,
    onpin,
    onselect,
  }: {
    /** The cable's name ("Beat → Glow"). */
    title: string;
    onremove: () => void;
    onreroute: () => void;
    onclose: () => void;
    /** Its pin (`cable:<id>`: Randomize leaves its amount), where it can be pinned here. */
    pinned?: boolean;
    onpin?: (on: boolean) => void;
    /** Select mode, from this cable (the graph's: several cables at once). */
    onselect?: () => void;
  } = $props();

  const then = (act: () => void) => () => {
    onclose();
    act();
  };
</script>

<MenuSheet label={title} {title} {onclose} over>
  <div role="menu">
    {#if onselect}<MenuEntry icon="listChecks" label="Select" onclick={then(onselect)} />{/if}
    {#if onpin}<MenuEntry icon={pinned ? 'pinOff' : 'pin'} label={pinned ? 'Unpin' : 'Pin'} onclick={then(() => onpin(!pinned))} />{/if}
    <MenuEntry icon="swap" label="Reroute" onclick={then(onreroute)} />
    <MenuEntry icon="trash" label="Delete" danger onclick={then(onremove)} />
  </div>
</MenuSheet>
