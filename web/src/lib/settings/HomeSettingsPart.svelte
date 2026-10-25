<script lang="ts">
  // Home (the owner, 2026-10-24): its sections in the user's order, each switched on or off (lib/settings/OrderList,
  // as the player's details and the profile's rows); and its header: a personal greeting in place of the moment's name,
  // and a picture of one's own in place of the weather's sky, kept on this device (lib/localCovers). Acts at once.
  import OrderReset from './OrderReset.svelte';
  import { HOME_HEADER, HOME_HEADER_PX, localCovers, removeLocalPicture, setLocalPicture } from '../localCovers.svelte';
  import { CRATE_PART, FLOW_PARTS, HOME_PARTS, homeDefault, prefs, type HomePart } from '../prefs.svelte';
  import { crates, loadCrates } from '../crates.svelte';
  import { toast } from '../toast.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import OrderList from './OrderList.svelte';
  import { offerReset } from './reset.svelte';

  offerReset(
    () => 'The sections as they came, in their first order; the moment named by its music and its sky the weather’s.',
    () => {
      prefs.homeParts = homeDefault();
      prefs.homeGreeting = false;
      prefs.homeCrates = false;
      void removeLocalPicture(HOME_HEADER);
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

  let busy = $state(false);
  async function pick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    busy = true;
    try {
      await setLocalPicture(HOME_HEADER, file, HOME_HEADER_PX);
    } catch (err) {
      toast.fail(err, "The picture couldn't be kept");
    } finally {
      busy = false;
    }
  }
</script>

<Group title="Sections" more="The first is the flow at the top, so it is one made of albums: Music for the moment, New for you, Blast from the past, Grave digging or Favourite albums.">
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
  <OrderList {items} on={onParts} label="Home's sections" onchange={setParts} lead={{ may: (id) => FLOW_PARTS.includes(id), label: 'Can be featured on top' }} />
</Group>

<Group title="Header">
  <Item
    name="Personal greeting"
    line="Good evening and your name, then Best for now"
    toggle={{ checked: prefs.homeGreeting, onchange: (on) => (prefs.homeGreeting = on) }}
  />
  <Item name="Background" line={localCovers[HOME_HEADER] ? 'Your picture, on this device' : 'The sky for the weather'}>
    <span class="pick">
      <label class="button" class:busy>{localCovers[HOME_HEADER] ? 'Change' : 'Choose a picture'}<input type="file" accept="image/*" hidden disabled={busy} onchange={pick} /></label>
      {#if localCovers[HOME_HEADER]}<button onclick={() => removeLocalPicture(HOME_HEADER)}>The sky</button>{/if}
    </span>
  </Item>
</Group>

<style>
  .pick {
    display: flex;
    gap: 8px;
  }

  /* The file input's label as the row's button, as the settings' other buttons look. */
  .button,
  .pick button {
    padding: 6px 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .button.busy {
    opacity: 0.5;
  }
</style>
