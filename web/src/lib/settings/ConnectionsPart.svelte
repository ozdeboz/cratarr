<script lang="ts">
  // Connections: the servers Cratarr works with, Lidarr, and Home Assistant (the owner, 2026-10-24: its own section,
  // its key with it; it acts at once, lib/settings/HomeAssistantPart); the rest saved with the Save bar. Each is a row (its state in a
  // line); its settings are on its own page behind it (`thing`: a server's id, or `lidarr`): its fields as rows, a value
  // typed or chosen in a sheet, its connection checked there, and Remove last (the settings' North Star, 2026-10-19).
  import type { Field } from '../api';
  import { getDiscogs, getLastfmServerKey, getLidarrOptions, setDiscogs, setLastfmServerKey, type DiscogsSetting } from '../api';
  import FormSheet from './FormSheet.svelte';
  import { toast } from '../toast.svelte';
  import { backFromThing, openThing } from '../panel.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import { ed, str, type Draft } from './editor.svelte';
  import Group from './Group.svelte';
  import HomeAssistantPart from './HomeAssistantPart.svelte';
  import ListeningPart from './ListeningPart.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  // Shown only once the settings are read (lib/SettingsPanel).
  const s = $derived(ed.s!);
  const LIDARR = 'lidarr';
  const HOME_ASSISTANT = 'home-assistant';
  // The listening services' own pages (lib/settings/ListeningPart).
  const LISTENING: Record<string, string> = { listenbrainz: 'ListenBrainz', lastfm: 'Last.fm' };
  const own = (t: string) => t === LIDARR || t === HOME_ASSISTANT || t in LISTENING;
  const LIDARR_MORE = "With Lidarr's address and API key, Delete in Incoming also stops Lidarr fetching that album again.";
  const WANT_MORE =
    "What Discover's Want adds an album to Lidarr with: its quality and metadata profiles, and the root folder it lands in (Incoming's). As your artists: the ones the artists Lidarr has already use.";
  const AS_ARTISTS = 'As your artists';
  // The server's own Last.fm key (Discover, decision 0069 amended, 0.9): everyone gets the artists like their own most
  // played, a Last.fm account or not. Saved at once, not with the Save bar.
  const LASTFM_MORE =
    "Discover asks Last.fm for artists like each person's most played. With a key here, everyone gets them, with a Last.fm account or without.";
  let fmKey = $state<{ set: boolean; saved: boolean } | null>(null);
  const loadFmKey = () =>
    getLastfmServerKey().then(
      (k) => (fmKey = k),
      () => (fmKey = null),
    );
  loadFmKey();
  // The server's Discogs app (0.9.5, the owner): covers and artist photos where there are none, genres where the files
  // have none, and other covers to choose from. Its key and secret typed together; the secret never comes back.
  const DISCOGS_MORE =
    "Covers and artist photos where Cratarr has none, genres for albums whose files have none, and more covers to choose from. Made at discogs.com/settings/developers.";
  let discogs = $state<DiscogsSetting | null>(null);
  const loadDiscogs = () =>
    getDiscogs().then(
      (d) => (discogs = d),
      () => (discogs = null),
    );
  loadDiscogs();
  let discogsForm = $state<{ key: string; secret: string; busy: boolean; error: string | null } | null>(null);
  async function saveDiscogs(f: { key: string; secret: string; busy: boolean; error: string | null }) {
    f.busy = true;
    f.error = null;
    try {
      discogs = await setDiscogs(f.key.trim(), f.secret.trim());
      discogsForm = null;
    } catch (e) {
      f.error = e instanceof Error ? e.message : String(e);
    } finally {
      f.busy = false;
    }
  }
  // Lidarr's own profiles and root folders, read when its page opens.
  let lidOptions = $state<{ quality: [number, string][]; metadata: [number, string][]; roots: string[] } | null>(null);
  $effect(() => {
    if (thing !== LIDARR || !s.lidarr) return;
    getLidarrOptions().then(
      (o) => (lidOptions = o),
      () => (lidOptions = null),
    );
  });
  const named = (list: [number, string][] | undefined, id: number | null) => (id === null ? AS_ARTISTS : (list?.find((x) => x[0] === id)?.[1] ?? `#${id}`));
  function chooseLidarr(which: 'quality' | 'metadata' | 'root') {
    const o = lidOptions;
    const options =
      which === 'root'
        ? (o?.roots ?? []).map((r) => ({ value: r, label: r }))
        : (o?.[which] ?? []).map(([id, name]) => ({ value: String(id), label: name }));
    const now = which === 'root' ? ed.lid.root : ed.lid[which];
    choosing = {
      title: which === 'quality' ? 'Quality profile' : which === 'metadata' ? 'Metadata profile' : 'Root folder',
      options: [{ value: '', label: AS_ARTISTS }, ...options],
      value: now === null ? '' : String(now),
      save: (v) => {
        if (which === 'root') ed.lid.root = v || null;
        else ed.lid[which] = v ? Number(v) : null;
      },
    };
  }

  // Its id only beside another server of its kind.
  const label = (d: Draft) => `${ed.kindOf(d)?.label ?? d.kind}${ed.drafts.filter((x) => x.kind === d.kind).length > 1 ? ` (${d.id})` : ''}`;
  const draft = $derived(thing && !own(thing) ? (ed.drafts.find((d) => d.id === thing) ?? null) : null);
  nameThing(() => (thing === LIDARR ? 'Lidarr' : thing === HOME_ASSISTANT ? 'Home Assistant' : thing && thing in LISTENING ? LISTENING[thing] : draft ? label(draft) : null));
  // A server removed (or never there): back to the list.
  $effect(() => {
    if (thing && !own(thing) && !draft) backFromThing('connections');
  });

  /** A server's state in a line: off, connected (to what), or what is wrong. */
  function stateOf(d: Draft) {
    const st = ed.statusLine(d.view);
    if (!d.enabled) return { lead: 'off', line: st?.ok ? `Off · ${st.text.replace(/^Connected to /, '')}` : 'Off' };
    if (!st) return { lead: 'off', line: 'Not checked yet' };
    return { lead: st.ok ? 'on' : 'bad', line: st.text };
  }
  const lidarrState = $derived.by(() => {
    const v = ed.lid.view;
    if (v && !v.enabled) return { lead: 'off', line: 'Off' };
    if (!v) return { lead: 'off', line: 'Not checked yet' };
    return v.version ? { lead: 'on', line: `Connected to Lidarr ${v.version}` } : { lead: 'bad', line: v.error ?? 'Not connected' };
  });

  // A value typed in its sheet, or one chosen of several.
  type Typing = { title: string; type: 'text' | 'url' | 'password'; value: string; placeholder?: string; help?: string | null; save: (v: string) => void };
  type Choosing = { title: string; options: { value: string; label: string }[]; value: string; save: (v: string) => void };
  let typing = $state<Typing | null>(null);
  let choosing = $state<Choosing | null>(null);
  let removing = $state<'server' | 'lidarr' | 'discogs' | null>(null);
  let addingKind = $state(false);

  function add(kind: string) {
    ed.addServer(kind);
    const d = ed.drafts.at(-1);
    if (d) openThing('connections', d.id);
  }

  /** A secret's line: saved, changed here, to be typed again, or none. */
  function secretLine(d: Draft, f: Field) {
    const isSaved = !!ed.saved(d)?.secrets_set.includes(f.key);
    const needed = ed.secretNeeded(d, f);
    if (d.secrets[f.key]?.trim()) return { line: 'Changed', tone: null };
    if (needed && isSaved) return { line: `Enter it again: you changed the ${ed.labelOf(d, f.bound_to)}`, tone: 'warn' as const };
    if (isSaved) return { line: 'Saved', tone: null };
    return { line: f.required ? 'Not set' : 'Optional', tone: f.required ? ('warn' as const) : null };
  }
  const choiceLabel = (d: Draft, f: Field) => {
    const v = str(d.values[f.key]);
    return ed.choicesFor(d, f).find((c) => c.value === v)?.label ?? (v || 'Choose…');
  };
</script>

{#snippet fieldRow(d: Draft, f: Field)}
  {#if f.kind === 'secret'}
    {@const sl = secretLine(d, f)}
    <Item
      name={f.label}
      line={sl.line}
      tone={sl.tone}
      more={f.help}
      onclick={() => (typing = { title: f.label, type: 'password', value: d.secrets[f.key] ?? '', help: f.help, save: (v) => (d.secrets[f.key] = v) })}
    />
  {:else if f.kind === 'choice' && ed.choicesFor(d, f).length}
    <Item
      name={f.label}
      value={choiceLabel(d, f)}
      more={f.help}
      onclick={() => (choosing = { title: f.label, options: ed.choicesFor(d, f), value: str(d.values[f.key]), save: (v) => (d.values[f.key] = v) })}
    />
  {:else}
    {@const v = str(d.values[f.key])}
    <Item
      name={f.label}
      line={v || (f.kind === 'choice' ? 'Check the connection first' : f.required ? 'Not set' : 'Optional')}
      tone={!v && f.required ? 'warn' : null}
      wrap={!!v}
      more={f.help}
      onclick={() => (typing = { title: f.label, type: f.kind === 'url' ? 'url' : 'text', value: v, help: f.help, save: (x) => (d.values[f.key] = x) })}
    />
  {/if}
{/snippet}

{#if draft}
  {@const d = draft}
  {@const k = ed.kindOf(d)}
  {@const st = ed.statusLine(d.view)}
  {@const fields = k?.fields ?? []}
  {@const plain = fields.filter((f) => f.kind === 'text' || f.kind === 'url' || f.kind === 'secret')}
  {@const choices = fields.filter((f) => f.kind === 'choice')}
  <Group>
    <Item name="Use this server" toggle={{ checked: d.enabled, onchange: (on) => (d.enabled = on) }} />
  </Group>
  <Group title="Connection">
    {#each plain as f (f.key)}{@render fieldRow(d, f)}{/each}
    <Item
      lead={st ? (st.ok ? 'on' : 'bad') : 'off'}
      name={st ? (st.ok ? 'Connected' : 'Not connected') : 'Not checked yet'}
      line={st ? st.text.replace(/^Connected to /, '') : null}
      tone={st && !st.ok ? 'bad' : null}
      wrap
    >
      <button disabled={d.busy} aria-busy={d.busy} onclick={() => ed.check(d)}>Check</button>
    </Item>
  </Group>
  {#if choices.length}
    <Group>
      {#each choices as f (f.key)}{@render fieldRow(d, f)}{/each}
    </Group>
  {/if}
  {#each fields.filter((f) => f.kind === 'multi_choice') as f (f.key)}
    {@const chosen = Array.isArray(d.values[f.key]) ? (d.values[f.key] as string[]) : []}
    <Group title={f.label} more={f.help}>
      {#each ed.choicesFor(d, f) as c (c.value)}
        <Item
          name={c.label}
          toggle={{ checked: chosen.includes(c.value), onchange: (on) => (d.values[f.key] = on ? [...chosen, c.value] : chosen.filter((x) => x !== c.value)) }}
        />
      {:else}
        <Item name="Check the connection first" />
      {/each}
    </Group>
  {/each}
  <Group>
    <Item name={`Remove ${label(d)}`} kind="danger" onclick={() => (removing = 'server')} />
  </Group>
  {#if removing === 'server'}
    <ConfirmSheet
      title={`Remove ${label(d)}?`}
      label="Remove"
      onconfirm={() => {
        ed.drafts = ed.drafts.filter((x) => x !== d);
        backFromThing('connections');
      }}
      onclose={() => (removing = null)}
    />
  {/if}
{:else if thing === HOME_ASSISTANT}
  <HomeAssistantPart page />
{:else if thing && thing in LISTENING}
  <ListeningPart {thing} />
{:else if thing === LIDARR}
  {#if s.lidarr}
    <Group>
      <Item name="Use Lidarr" toggle={{ checked: ed.lid.enabled, onchange: (on) => (ed.lid.enabled = on) }} />
    </Group>
  {/if}
  <Group title="Connection">
    <Item
      name="Server address"
      line={ed.lid.url || 'Not set'}
      wrap={!!ed.lid.url}
      onclick={() => (typing = { title: 'Server address', type: 'url', value: ed.lid.url, placeholder: 'http://lidarr:8686', save: (v) => (ed.lid.url = v) })}
    />
    <Item
      name="API key"
      line={ed.lidarrKeyNeeded ? 'Enter it again: you changed the server address' : ed.lid.key ? 'Changed' : s.lidarr?.key_set ? 'Saved' : 'Not set'}
      tone={ed.lidarrKeyNeeded ? 'warn' : null}
      more="In Lidarr under Settings, General."
      onclick={() => (typing = { title: 'API key', type: 'password', value: ed.lid.key, help: 'In Lidarr under Settings, General', save: (v) => (ed.lid.key = v) })}
    />
    <Item lead={lidarrState.lead} name={ed.lid.view?.version ? 'Connected' : ed.lid.view ? 'Not connected' : 'Not checked yet'} line={ed.lid.view ? lidarrState.line : null} tone={lidarrState.lead === 'bad' ? 'bad' : null} wrap>
      <button disabled={ed.lid.busy || !ed.lid.url.trim()} aria-busy={ed.lid.busy} onclick={ed.checkLidarr}>Check</button>
    </Item>
  </Group>
  {#if s.lidarr && ed.lid.url}
    <!-- What Discover's Want adds an album with, as Seerr asks of each server (the owner, 2026-10-24). -->
    <Group title="Want" more={WANT_MORE}>
      <Item name="Quality profile" value={named(lidOptions?.quality, ed.lid.quality)} onclick={() => chooseLidarr('quality')} />
      <Item name="Metadata profile" value={named(lidOptions?.metadata, ed.lid.metadata)} onclick={() => chooseLidarr('metadata')} />
      <Item name="Root folder" value={ed.lid.root ?? AS_ARTISTS} wrap onclick={() => chooseLidarr('root')} />
    </Group>
    <Group>
      <Item name="Remove Lidarr" kind="danger" onclick={() => (removing = 'lidarr')} />
    </Group>
  {/if}
  {#if removing === 'lidarr'}
    <ConfirmSheet
      title="Remove Lidarr?"
      label="Remove"
      onconfirm={() => {
        ed.lid.url = '';
        ed.lid.key = '';
        ed.lid.view = null;
        backFromThing('connections');
      }}
      onclose={() => (removing = null)}
    />
  {/if}
{:else}
  <Group title="Servers" more="Shared by everyone who uses Cratarr, not this device's own.">
    {#if s.origin === 'env'}<Item name="From the container's settings" line="Until you save here" />{/if}
    <!-- Those switched off last (the owner, 2026-10-24), each side in its own order. -->
    {#each [...ed.drafts.filter((d) => d.enabled), ...ed.drafts.filter((d) => !d.enabled)] as d (d)}
      {@const st = stateOf(d)}
      <Item lead={st.lead} name={label(d)} line={st.line} tone={st.lead === 'bad' ? 'bad' : null} onclick={() => openThing('connections', d.id)} />
    {:else}
      <Item name="No server yet" />
    {/each}
    {#if s.kinds.length === 1}
      <Item name={`Add ${s.kinds[0].label} server`} kind="add" onclick={() => add(s.kinds[0].id)} />
    {:else if s.kinds.length > 1}
      <Item name="Add server" kind="add" onclick={() => (addingKind = true)} />
    {/if}
  </Group>
  {#if ed.showLidarr}
    <Group title="Downloads" more={LIDARR_MORE}>
      {#if s.lidarr || ed.lid.url}
        <Item lead={lidarrState.lead} name="Lidarr" line={lidarrState.line} tone={lidarrState.lead === 'bad' ? 'bad' : null} onclick={() => openThing('connections', LIDARR)} />
      {:else}
        <Item name="Add Lidarr" kind="add" onclick={() => openThing('connections', LIDARR)} />
      {/if}
    </Group>
  {/if}
  <Group title="Discover" more={LASTFM_MORE}>
    <Item
      name="Last.fm key for everyone"
      value={fmKey?.saved ? 'Set' : fmKey?.set ? "From the container's settings" : 'None'}
      onclick={() =>
        (typing = {
          title: 'Last.fm API key',
          type: 'password',
          value: '',
          help: 'Made at last.fm/api. Empty takes the key away.',
          save: async (v) => {
            await setLastfmServerKey(v);
            await loadFmKey();
          },
        })}
    />
  </Group>
  <Group title="Discogs" more={DISCOGS_MORE}>
    <Item
      name="App key and secret"
      value={discogs?.set ? (discogs.per_minute ? `Set · ${discogs.per_minute} a minute` : 'Set') : 'None'}
      onclick={() => (discogsForm = { key: '', secret: '', busy: false, error: null })}
    />
    {#if discogs?.set}<Item name="Remove Discogs" kind="danger" onclick={() => (removing = 'discogs')} />{/if}
  </Group>
  {#if removing === 'discogs'}
    <ConfirmSheet
      title="Remove Discogs?"
      label="Remove"
      onconfirm={() => setDiscogs('', '').then((d) => (discogs = d), (e) => toast.fail(e, "Discogs wasn't removed"))}
      onclose={() => (removing = null)}
    />
  {/if}
  <HomeAssistantPart />
  <ListeningPart />
  {#if addingKind}
    <ChoiceSheet title="Add server" options={s.kinds.map((k) => ({ value: k.id, label: k.label }))} value={''} onchange={add} onclose={() => (addingKind = false)} />
  {/if}
{/if}

{#if discogsForm}
  {@const f = discogsForm}
  <FormSheet title="Discogs" submitLabel="Save" canSubmit={!!f.key.trim() && !!f.secret.trim()} busy={f.busy} error={f.error} onsubmit={() => saveDiscogs(f)} onclose={() => (discogsForm = null)}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>Consumer key<input bind:value={f.key} autocomplete="off" spellcheck="false" autofocus /></label>
    <label>Consumer secret<input type="password" bind:value={f.secret} autocomplete="off" /></label>
  </FormSheet>
{/if}
{#if typing}
  {@const t = typing}
  <ValueSheet title={t.title} type={t.type} value={t.value} placeholder={t.placeholder ?? ''} help={t.help ?? null} onsave={(v) => t.save(v)} onclose={() => (typing = null)} />
{/if}
{#if choosing}
  {@const c = choosing}
  <ChoiceSheet title={c.title} options={c.options} value={c.value} onchange={(v) => c.save(v)} onclose={() => (choosing = null)} />
{/if}
