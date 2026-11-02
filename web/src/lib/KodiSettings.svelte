<script lang="ts">
  // Settings → Players → Kodi (the owner, 2026-10-16): the Kodis Cratarr plays on, driven over Kodi's own remote
  // control ("Allow remote control via HTTP"), nothing installed on Kodi. Each becomes one of the players: "Play on"
  // reaches it, and a group takes it in as a follower. Each is a row, its settings on its own page behind it (`thing`:
  // `kodi:<n>`, the settings' North Star, 2026-10-19), a value typed in a sheet. Saved apart from the other settings,
  // as each is changed (the owner, 2026-11-02: no Unsaved changes bar): its address, user and password together in one
  // sheet, never half of them; a Kodi added only once that sheet is saved. Its password is never shown again (blank
  // keeps it).
  import type { Snippet } from 'svelte';
  import { getPlayOnSettings, savePlayOnSettings, type KodiSetting } from './api';
  import { backFromThing, openThing } from './panel.svelte';
  import { playOn } from './playOn.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import FormSheet from './settings/FormSheet.svelte';
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
  let loaded = $state(false);

  function take(s: { webhooks: string[]; kodi: KodiSetting[]; stream_default: string | null }) {
    webhooks = s.webhooks;
    kodis = s.kodi.map((k) => ({ ...k, password: '' }));
    base = s.stream_default;
    loaded = true;
  }

  $effect(() => {
    getPlayOnSettings().then(take, (e) => toast.fail(e, "The players' settings weren't read"));
  });

  /** Saves the Kodis as `next`; refused, what the server keeps is read again and why is said. */
  async function save(next: KodiSetting[] = kodis) {
    try {
      take(await savePlayOnSettings({ webhooks, kodi: next.map(({ name, url, user, password, stream_from }) => ({ name, url, user, password: password ?? '', stream_from })) }));
      return true;
    } catch (e) {
      toast.fail(e, "Kodi wasn't saved");
      getPlayOnSettings().then(take, () => {});
      return false;
    }
  }

  /** One of a Kodi's own values, saved at once. */
  function set(k: KodiSetting, change: Partial<KodiSetting>) {
    Object.assign(k, change);
    void save();
  }

  /** Its connection: a Kodi's (`at`), or a new one (null), saved together. */
  type ConnForm = { at: number | null; name: string; url: string; user: string; password: string; kept: boolean };
  let conn = $state<ConnForm | null>(null);
  let connBusy = $state(false);
  function add() {
    conn = { at: null, name: kodis.length ? `Kodi ${kodis.length + 1}` : 'Kodi', url: 'http://', user: 'kodi', password: '', kept: false };
  }
  async function saveConn(c: ConnForm) {
    connBusy = true;
    const was = c.at === null ? null : kodis[c.at];
    const entry = { ...(was ?? { stream_from: '' }), name: c.name.trim() || 'Kodi', url: c.url.trim(), user: c.user.trim(), password: c.password } as KodiSetting;
    const next = c.at === null ? [...kodis, entry] : kodis.map((k, i) => (i === c.at ? entry : k));
    const ok = await save(next);
    connBusy = false;
    if (!ok) return;
    conn = null;
    if (c.at === null) openThing('players', `kodi:${kodis.length - 1}`);
  }
  const addressGiven = (url: string) => !!url.trim() && !/^https?:\/\/$/.test(url.trim());

  function remove(i: number) {
    void save(kodis.filter((_, j) => j !== i));
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
      {@const openConn = () => (conn = { at, name: k.name, url: k.url, user: k.user, password: '', kept: !!k.password_set })}
      <Item name="Name" value={k.name} onclick={() => edit({ title: 'Name', value: k.name, type: 'text', save: (v) => set(k, { name: v }) })} />
      <Item name="Address" line={k.url} wrap onclick={openConn} />
      <Item name="User" value={k.user} onclick={openConn} />
      <Item name="Password" line={k.password_set ? 'Saved' : 'Not set'} onclick={openConn} />
      <Item
        name="Cratarr for Kodi"
        more="The address Kodi reaches Cratarr on, when it is not the one these pages use."
        line={k.stream_from || `${base ?? 'http://cratarr:8081'} (the default)`}
        wrap
        onclick={() => edit({ title: 'Cratarr for Kodi', value: k.stream_from, type: 'url', placeholder: base ?? 'http://cratarr:8081', save: (v) => set(k, { stream_from: v }) })}
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
  {#if conn}
    {@const c = conn}
    <FormSheet title={c.at === null ? 'Add Kodi' : c.name} submitLabel="Save" canSubmit={addressGiven(c.url)} busy={connBusy} onsubmit={() => void saveConn(c)} onclose={() => (conn = null)}>
      {#if c.at === null}<label>Name<input bind:value={c.name} autocomplete="off" spellcheck="false" /></label>{/if}
      <!-- svelte-ignore a11y_autofocus -->
      <label>Address<input type="url" bind:value={c.url} placeholder="http://kodi:8080" autocomplete="off" spellcheck="false" autofocus /></label>
      <label>User<input bind:value={c.user} autocomplete="off" spellcheck="false" /></label>
      <label>Password<input type="password" bind:value={c.password} placeholder={c.kept ? 'Kept' : ''} autocomplete="off" /></label>
    </FormSheet>
  {/if}
{/if}

{#if typing}
  {@const t = typing}
  <ValueSheet title={t.title} type={t.type} value={t.value} placeholder={t.placeholder ?? ''} allowEmpty={t.title !== 'Name'} onsave={(v) => t.save(v)} onclose={() => (typing = null)} />
{/if}
