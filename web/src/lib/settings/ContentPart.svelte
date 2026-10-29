<script lang="ts">
  // Content (the owner, 2026-10-25: Home's part and Display's music profiles together). Home (the owner, 2026-10-24):
  // its sections in the user's order, each switched on or off (lib/settings/OrderList,
  // as the player's details and the profile's rows); and its header: a personal greeting in place of the moment's name,
  // and a picture of one's own in place of the weather's sky, kept on this device (lib/localCovers). Acts at once.
  import OrderReset from './OrderReset.svelte';
  import { CRATE_PART, FLOW_PARTS, HOME_PARTS, homeDefault, PROFILE_ROWS, prefs, type HomePart } from '../prefs.svelte';
  import { crates, loadCrates } from '../crates.svelte';
  import { toast } from '../toast.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import OrderList from './OrderList.svelte';
  import { localMode } from '../deviceMusic.svelte';
  import { LOCAL_PARTS } from '../localHome.svelte';
  import { offerReset } from './reset.svelte';

  offerReset(
    () => 'Home’s sections as they came, in their first order; the music profiles with every row.',
    () => {
      prefs.profileOpen = true;
      prefs.profile = true;
      prefs.profileFoldAt = 0;
      prefs.profileRows = PROFILE_ROWS.map((r) => r.id);
      prefs.homeParts = homeDefault();
      prefs.homeCrates = false;
    },
  );

  /** The first section leads Home as the flow on top, so only one made of albums may be first (the owner, 2026-10-24):
   *  another dragged there lands under the first that can lead; the last that can lead stays switched on. */
  function setParts(parts: HomePart[]) {
    const at = parts.findIndex((p) => FLOW_PARTS.includes(p));
    if (at < 0) {
      toast.show({ text: 'One album section stays on top', tone: 'error' });
      return;
    }
    prefs.homeParts = at === 0 ? parts : [parts[at], ...parts.slice(0, at), ...parts.slice(at + 1)];
  }

  // Your crates on Home (0.9.3; the owner, 2026-10-25): on, each of one's own crates joins the sections below (switched
  // on, at the end), to be ordered and switched as every section is; off, they leave Home and the list.
  $effect(() => {
    if (prefs.homeCrates && !crates.list) void loadCrates();
  });
  const ownCrates = $derived((crates.list ?? []).filter((c) => !c.permanent));
  const partOf = (id: string) => `${CRATE_PART}${id}` as HomePart;
  const items = $derived(prefs.homeCrates ? [...HOME_PARTS, ...ownCrates.map((c) => ({ id: partOf(c.id), label: c.name }))] : HOME_PARTS);
  const onParts = $derived(prefs.homeParts.filter((p) => !p.startsWith(CRATE_PART) || (prefs.homeCrates && ownCrates.some((c) => p === partOf(c.id)))));
  async function setHomeCrates(on: boolean) {
    prefs.homeCrates = on;
    const kept = prefs.homeParts.filter((p) => !p.startsWith(CRATE_PART));
    if (!on) {
      prefs.homeParts = kept;
      return;
    }
    if (!crates.list) await loadCrates();
    prefs.homeParts = [...kept, ...ownCrates.map((c) => partOf(c.id))];
  }
</script>

<Group title="Home" more="The first is the flow at the top, so it is one made of albums: Music for the moment, New for you, Blast from the past, Grave digging or Favourite albums.">
  {#snippet tail()}
    <OrderReset
      label="Sections"
      isDefault={prefs.homeParts.join() === homeDefault().join()}
      reset={() => {
        const was = prefs.homeParts;
        prefs.homeParts = homeDefault();
        return () => (prefs.homeParts = was);
      }}
    />
  {/snippet}
  <Item name="Your crates on Home" toggle={{ checked: prefs.homeCrates, onchange: (on) => void setHomeCrates(on) }} />
  <OrderList
    {items}
    on={onParts}
    label="Home's sections"
    onchange={setParts}
    lead={{ may: (id) => FLOW_PARTS.includes(id), label: 'Can be featured on top' }}
    unavailable={localMode() ? { has: (id) => !LOCAL_PARTS.includes(id), note: 'Needs your server' } : null}
  />
</Group>

<Group title="Music profiles">
  {#snippet tail()}
    {#if prefs.profile}<OrderReset
        label="Profile rows"
        isDefault={prefs.profileRows.join() === PROFILE_ROWS.map((r) => r.id).join() && prefs.profileFoldAt === 0}
        reset={() => {
          const was = { rows: prefs.profileRows, at: prefs.profileFoldAt };
          prefs.profileRows = PROFILE_ROWS.map((r) => r.id);
          prefs.profileFoldAt = 0;
          return () => ((prefs.profileRows = was.rows), (prefs.profileFoldAt = was.at));
        }}
      />{/if}
  {/snippet}
  <Item
    name="Shown"
    more={"A playlist's, a crate's or an artist's music at a glance, at the top of its page: its tempo, energy and danceability, its era, its styles, and its moods against your whole library.\n\nMost of it comes from AudioMuse, which listens to every track in Cratarr's library: tracks it has not heard yet count only by their tags (Health shows how far it is).\n\nBelow, choose which rows a profile shows and in what order; the rows above the line stay when one is folded."}
    toggle={{ checked: prefs.profile, onchange: (on) => (prefs.profile = on) }}
  />
  <!-- The rows in their order, the fold line among them: above it stays when a profile is folded. -->
  {#if prefs.profile}<OrderList
    items={PROFILE_ROWS}
    on={prefs.profileRows}
    label="The profile's rows"
    onchange={(v) => (prefs.profileRows = v)}
    fold={{ at: Math.min(prefs.profileFoldAt, prefs.profileRows.length), onchange: (at) => (prefs.profileFoldAt = at) }}
  />{/if}
</Group>
