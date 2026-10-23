<script lang="ts">
  // A crate's menu (decision 0058; a genre is a crate of one condition): a long-press on its card on the Crates page, a
  // right-click there, or the ⋮ in its panel. The crate on the card (its picture or its albums' mosaic), Play, Shuffle
  // and Start radio as the large actions (no Play next or Add to queue: a crate's or a playlist's make no sense, the
  // owner) (its tracks as its favourites settings have them),
  // then the shared groups: View; Save as playlist (a genre: Start a crate from it); Keep on this device and Share;
  // Edit (one's own); Delete (one's own, last).
  import { hiddenLibraries } from './listFilter.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import { crateRows, getCrate } from './api';
  import { inApp } from './app.svelte';
  import { CRATE, EDIT_CRATE, crateArt, crateName, crateRadio, crates, deleteOwnCrate, genreOf, likeOf, playCrate, seeded, shuffleCrate, startCrate } from './crates.svelte';
  import { isKept, setKept } from './kept.svelte';
  import { panel, viewPlaylist } from './panel.svelte';
  import { work } from './panelWork.svelte';
  import { prefs } from './prefs.svelte';
  import { linkTo, share } from './share';
  import { toast } from './toast.svelte';
  import { plural } from './insights.svelte';

  let { id, onclose, onsave }: { id: string; onclose: () => void; /** Its panel's own Make playlist, where it shows. */ onsave?: () => void } = $props();

  const name = $derived(crateName(id));
  const genre = $derived(genreOf(id));
  const crate = $derived(crates.list?.find((c) => c.id === id) ?? null);
  const like = $derived(likeOf(id));
  /** One's own (a permanent crate, a genre and what sounds like a track or an album are the library's). */
  const own = $derived(!!crate && !crate.permanent && !seeded(id));
  const viewed = $derived(panel.viewed === CRATE + id);
  const tracks = $derived(crate?.stats?.tracks ?? null);

  // Where its tracks come from (Keep on this device follows that library's).
  let source = $state<string | null>(null);
  $effect(() => {
    const c = id;
    if (!c) return;
    crateRows(c, 0, 1, prefs.source || undefined, undefined, undefined, { without: hiddenLibraries() })
      .then((r) => (source = r.source))
      .catch(() => {});
  });

  let working = $state(false);
  async function run(what: () => Promise<unknown>, done: string | null, fail: string) {
    navigator.vibrate?.(8);
    working = true;
    try {
      await what();
      if (done) toast.show({ text: done, tone: 'done' });
      onclose();
    } catch (e) {
      toast.fail(e, fail);
    } finally {
      working = false;
    }
  }
  // Each reads what it needs before closing: closing lets go of `id` (its opener's state), so a view after it read null.
  function view() {
    const c = id;
    onclose();
    viewPlaylist(CRATE + c);
  }
  // Save as playlist: its panel, asking for the new playlist's name (its Make playlist).
  function saveAsPlaylist() {
    const c = id;
    const save = onsave;
    onclose();
    if (save) return save();
    work.making = { for: CRATE + c, state: { name: '', saving: false } };
    viewPlaylist(CRATE + c);
  }
  function fromGenre() {
    const g = genre;
    if (g === null) return;
    onclose();
    startCrate(g, [{ axis: 'genre', not: false, value: { kind: 'any_of', values: [g] } }]);
  }
  /** Sounds like's own crate: its seeds and how alike, as the server made them. */
  function fromLike() {
    const [c, n] = [id, name];
    onclose();
    getCrate(c)
      .then((d) => startCrate(n, d.conditions))
      .catch((e) => toast.fail(e, "The crate couldn't be started"));
  }
  function edit() {
    const c = id;
    onclose();
    viewPlaylist(EDIT_CRATE + c);
  }
  function shareIt() {
    const [c, n] = [id, name];
    onclose();
    share(n, n, linkTo('crates', CRATE + c));
  }
</script>

<MenuSheet
  label={name}
  {onclose}
  images={crateArt(id)}
  title={name}
  chips={[genre !== null ? 'Genre' : 'Crate', ...(tracks !== null ? [plural(tracks, 'track')] : [])]}
  actions={[
    { icon: 'play', label: 'Play', disabled: working, onclick: () => run(() => playCrate(id, name, 'play'), null, 'Could not start playing') },
    { icon: 'shuffle', label: 'Shuffle', disabled: working, onclick: () => run(() => shuffleCrate(id, name), null, 'Could not start playing') },
    { icon: 'radio', label: 'Start radio', disabled: working, onclick: () => run(() => crateRadio(id, name), null, "The radio didn't start") },
  ]}
>
  <!-- The shared groups, in their order (decision 0058). -->
  <MenuCards>
    <MenuGroup>
      {#if !viewed}<MenuEntry icon="squareArrowOutUpRight" label="View" onclick={view} />{/if}
    </MenuGroup>
    <MenuGroup>
      {#if genre !== null}
        <MenuEntry icon="crates" label="Start a crate from it" onclick={fromGenre} />
      {:else if like !== null}
        <MenuEntry icon="crates" label="Start a crate from it" onclick={fromLike} />
      {:else}
        <MenuEntry icon="intoBox" label="Save as playlist" onclick={saveAsPlaylist} />
      {/if}
    </MenuGroup>
    <MenuGroup>
      {#if inApp && source && genre === null}
        {@const s = source}
        {@const on = isKept('crate', s, id)}
        <MenuEntry icon="download" label="Keep on this device" switched={on} onclick={() => setKept('crate', s, id, !on, name)} />
      {/if}
      <MenuEntry icon="share" label="Share" onclick={shareIt} />
    </MenuGroup>
    <MenuGroup>
      {#if own}<MenuEntry icon="pencil" label="Edit" onclick={edit} />{/if}
    </MenuGroup>
    <MenuGroup>
      {#if own}<MenuEntry icon="trash" label="Delete crate" danger disabled={working} onclick={() => run(() => deleteOwnCrate(id), null, "The crate wasn't deleted")} />{/if}
    </MenuGroup>
  </MenuCards>
</MenuSheet>
