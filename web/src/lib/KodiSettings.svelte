<script lang="ts">
  // Settings → Players → Kodi (the owner, 2026-10-16): the Kodis Cratarr plays on, driven over Kodi's own remote
  // control ("Allow remote control via HTTP"), nothing installed on Kodi. Each becomes one of the players: "Play on"
  // reaches it, and a group takes it in as a follower. Each is a row, its settings on its own page behind it (`thing`:
  // `kodi:<n>`, the settings' North Star, 2026-10-19), a value typed in a sheet. Saved apart from the other settings,
  // with its own Save bar. Its password is never shown again (blank keeps it).
  import type { Snippet } from 'svelte';
  import { getPlayOnSettings, savePlayOnSettings, type KodiSetting } from './api';
  import { backFromThing, openThing } from './panel.svelte';
  import { playOn } from './playOn.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import SaveBar from './settings/SaveBar.svelte';
  import ValueSheet from './settings/ValueSheet.svelte';
  import { nameThing } from './settingsSections.svelte';
  import { toast } from './toast.svelte';

  /** `delay`: a Kodi's speaker delay row, on its page (lib/PlayersSettings' own). */
  let { thing = null, delay = null }: { thing?: string | null; delay?: Snippet<[string]> | null } = $props();

  const KODI_MORE =
    "Cratarr drives a Kodi over Kodi's own remote control (Settings → Services → Control → Allow remote control via HTTP); nothing is installed on Kodi. In a group it follows, and never leads.";

  let webhooks = $state<string[]>([]);
  let kodis = $state<KodiSetting[]>([]);
  let base = $state<string | null>(null);
  let dirty = $state(false);
  let saving = $state(false);
  let loaded = $state(false);

  function take(s: { webhooks: string[]; kodi: KodiSetting[]; stream_default: string | null }) {
    webhooks = s.webhooks;
    kodis = s.kodi.map((k) => ({ ...k, password: '' }));
    base = s.stream_default;
    dirty = false;
    loaded = true;
  }

  $effect(() => {
    getPlayOnSettings().then(take, (e) => toast.fail(e, "The players' settings weren't read"));
  });

  function add() {
    kodis = [...kodis, { name: kodis.length ? `Kodi ${kodis.length + 1}` : 'Kodi', url: 'http://', user: 'kodi', password: '', stream_from: '' }];
    dirty = true;
    openThing('players', `kodi:${kodis.length - 1}`);
  }

  function remove(i: number) {
    kodis = kodis.filter((_, j) => j !== i);
    dirty = true;
  }

  async function save() {
    saving = true;
    try {
      take(await savePlayOnSettings({ webhooks, kodi: kodis.map(({ name, url, user, password, stream_from }) => ({ name, url, user, password: password ?? '', stream_from })) }));
    } catch (e) {
      toast.fail(e, "Kodi wasn't saved");
    } finally {
      saving = false;
    }
  }

  const at = $derived(thing?.startsWith('kodi:') ? Number(thing.slice(5)) : null);
  const open = $derived(at !== null && kodis[at] ? kodis[at] : null);
  nameThing(() => (open ? open.name : null));
  // A Kodi gone (removed, or not there once read): back to the list.
  $effect(() => {
    if (loaded && at !== null && !open) backFromThing('players');
  });

  const host = (url: string) => url.replace(/^\w+:\/\//, '').replace(/\/.*$/, '');
  const online = (name: string) => playOn.players.some((p) => p.kind === 'kodi' && p.name === name && p.online);

  type Typing = { title: string; value: string; type: 'text' | 'url' | 'password'; placeholder?: string; save: (v: string) => void };
  let typing = $state<Typing | null>(null);
  let removing = $state(false);
  const edit = (t: Typing) => (typing = t);
</script>

{#if loaded}
  {#if open && at !== null}
    {@const k = open}
    <Group>
      <Item name="Name" value={k.name} onclick={() => edit({ title: 'Name', value: k.name, type: 'text', save: (v) => ((k.name = v), (dirty = true)) })} />
      <Item name="Address" line={k.url} wrap onclick={() => edit({ title: 'Address', value: k.url, type: 'url', placeholder: 'http://kodi:8080', save: (v) => ((k.url = v), (dirty = true)) })} />
      <Item name="User" value={k.user} onclick={() => edit({ title: 'User', value: k.user, type: 'text', save: (v) => ((k.user = v), (dirty = true)) })} />
      <Item
        name="Password"
        line={k.password ? 'Changed' : k.password_set ? 'Saved' : 'Not set'}
        onclick={() => edit({ title: 'Password', value: k.password ?? '', type: 'password', save: (v) => ((k.password = v), (dirty = true)) })}
      />
      <Item
        name="Cratarr for Kodi"
        more="The address Kodi reaches Cratarr on, when it is not the one these pages use."
        line={k.stream_from || `${base ?? 'http://cratarr:8081'} (the default)`}
        wrap
        onclick={() => edit({ title: 'Cratarr for Kodi', value: k.stream_from, type: 'url', placeholder: base ?? 'http://cratarr:8081', save: (v) => ((k.stream_from = v), (dirty = true)) })}
      />
    </Group>
    {#if delay}<Group>{@render delay(k.name)}</Group>{/if}
    <Group>
      <Item name={`Remove ${k.name}`} kind="danger" onclick={() => (removing = true)} />
    </Group>
    {#if removing}
      {@const i = at}
      <ConfirmSheet
        title={`Remove ${k.name}?`}
        label="Remove"
        onconfirm={() => {
          remove(i);
          backFromThing('players');
        }}
        onclose={() => (removing = false)}
      />
    {/if}
  {:else if !thing}
    <Group title="Kodi" more={KODI_MORE}>
      {#each kodis as k, i (i)}
        <Item lead={online(k.name) ? 'on' : 'off'} name={k.name} line={host(k.url) || 'No address'} onclick={() => openThing('players', `kodi:${i}`)} />
      {/each}
      <Item name="Add Kodi" kind="add" onclick={add} />
    </Group>
  {/if}
  {#if dirty}
    <SaveBar {saving} onsave={save} ondiscard={() => getPlayOnSettings().then(take)} />
  {/if}
{/if}

{#if typing}
  {@const t = typing}
  <ValueSheet title={t.title} type={t.type} value={t.value} placeholder={t.placeholder ?? ''} allowEmpty={t.title !== 'Name'} onsave={(v) => t.save(v)} onclose={() => (typing = null)} />
{/if}
