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
  }: {
    /** The cable's name ("Beat → Glow"). */
    title: string;
    onremove: () => void;
    onreroute: () => void;
    onclose: () => void;
  } = $props();

  const then = (act: () => void) => () => {
    onclose();
    act();
  };
</script>

<MenuSheet label={title} {title} {onclose} over>
  <div role="menu">
    <MenuEntry icon="swap" label="Reroute" onclick={then(onreroute)} />
    <MenuEntry icon="trash" label="Delete" danger onclick={then(onremove)} />
  </div>
</MenuSheet>
