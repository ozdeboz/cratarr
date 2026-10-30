<script lang="ts">
  // Connections: the servers Cratarr works with, Lidarr, Last.fm and Discogs, and Home Assistant (the owner, 2026-10-24:
  // its own section, its key with it; it acts at once, lib/settings/HomeAssistantPart); the rest saved with the Save bar.
  // Each is a row (its state in a line); its settings are on its own page behind it (`thing`: a server's id, `lidarr`,
  // `lastfm`, `discogs`): its fields as rows, a value typed or chosen in a sheet, its connection checked there, and
  // Remove last (the settings' North Star, 2026-10-19; the owner, 2026-10-25: Last.fm and Discogs the same way, no
  // longer rows of their own kind). Headed by what each is for, generic (the owner, 2026-10-25: Media servers, Download
  // stack, Discovery and metadata, Home automation), which the settings search finds too.
  import { offers } from '../capabilities.svelte';
  import type { Field } from '../api';
  import {
    getAcoustid,
    getDiscogs,
    getLastfmServerKey,
    getLidarrOptions,
    getMusicBrainz,
    getSlskd,
    missingCall,
    setAcoustid,
    setDiscogs,
    setDiscogsEnabled,
    setLastfmEnabled,
    setLastfmServerKey,
    setMusicBrainzEnabled,
    setSlskd,
    setSlskdEnabled,
    type AcoustidSetting,
    type DiscogsSetting,
    type MusicBrainzStatus,
    type SlskdSetting,
  } from '../api';
  import { when } from './when';
  import FormSheet from './FormSheet.svelte';
  import { toast } from '../toast.svelte';
  import { backFromThing, openThing } from '../panel.svelte';
  import { nameThing } from '../settingsSections.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import ConfirmSheet from './ConfirmSheet.svelte';
  import { ed, str, type Draft } from './editor.svelte';
  import Group from './Group.svelte';
  import HomeAssistantPart from './HomeAssistantPart.svelte';
  import Item from './Item.svelte';
  import ValueSheet from './ValueSheet.svelte';

  let { thing = null }: { thing?: string | null } = $props();

  // Shown only once the settings are read (lib/SettingsPanel).
  const s = $derived(ed.s!);
  const LIDARR = 'lidarr';
  const HOME_ASSISTANT = 'home-assistant';
  const LASTFM = 'lastfm';
  const DISCOGS = 'discogs';
  const MUSICBRAINZ = 'musicbrainz';
  const SLSKD = 'slskd';
  const ACOUSTID = 'acoustid';
  const own = (t: string) => t === LIDARR || t === HOME_ASSISTANT || t === LASTFM || t === DISCOGS || t === MUSICBRAINZ || t === SLSKD || t === ACOUSTID;
  const LIDARR_MORE = "With Lidarr's address and API key, Delete in Incoming also stops Lidarr fetching that album again.";
  const WANT_MORE =
    "What Discover's Want adds an album to Lidarr with: its quality and metadata profiles, and the root folder it lands in (Incoming's). As your artists: the ones the artists Lidarr has already use.";
  const AS_ARTISTS = 'As your artists';
  // The server's own Last.fm key (Discover, decision 0069 amended, 0.9): everyone gets the artists like their own most
  // played, a Last.fm account or not. Saved at once, not with the Save bar.
  const LASTFM_MORE =
    "Discover asks Last.fm for artists like each person's most played. With a key here, everyone gets them, with a Last.fm account or without.";
  let fmKey = $state<{ set: boolean; saved: boolean; enabled?: boolean } | null>(null);
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
  /** A server before Discogs (0.9.5) has no such setting: no group, rather than a key it cannot keep. */
  let discogsKnown = $state(false);
  const loadDiscogs = () =>
    getDiscogs().then(
      (d) => ((discogs = d), (discogsKnown = true)),
      (e) => ((discogs = null), (discogsKnown = !missingCall(e))),
    );
  // Asked only where the server's list says it has the setting (0.11, lib/capabilities); a server before it, as before.
  if (offers('discogs_settings')) loadDiscogs();
  let discogsForm = $state<{ key: string; secret: string; busy: boolean; error: string | null } | null>(null);
  // slskd and AcoustID (0.9.8: single-track replacement): a missing track fetched from Soulseek, fingerprinted
  // against the release, added to its album. Each saved at once, as Discogs; slskd's key never comes back, nor AcoustID's.
  const SLSKD_MORE = 'Soulseek through slskd: a single missing track found, fingerprinted with AcoustID against the release, and added to its album.';
  const ACOUSTID_MORE = 'Fingerprints a track fetched for an album, so only the right recording is added. A free key from acoustid.org/new-application.';
  const SLSKD_FOLDER = '/slskd';
  let slskd = $state<SlskdSetting | null>(null);
  /** A server before slskd (0.9.8) has no such setting: no row. */
  let slskdKnown = $state(false);
  const loadSlskd = () =>
    getSlskd().then(
      (d) => ((slskd = d), (slskdKnown = true)),
      (e) => ((slskd = null), (slskdKnown = !missingCall(e))),
    );
  if (offers('slskd_settings')) loadSlskd();
  type SlskdForm = { url: string; key: string; folder: string; busy: boolean; error: string | null };
  let slskdForm = $state<SlskdForm | null>(null);
  let acoustid = $state<AcoustidSetting | null>(null);
  let acoustidKnown = $state(false);
  const loadAcoustid = () =>
    getAcoustid().then(
      (d) => ((acoustid = d), (acoustidKnown = true)),
      (e) => ((acoustid = null), (acoustidKnown = !missingCall(e))),
    );
  if (offers('acoustid_settings')) loadAcoustid();
  let acoustidForm = $state<{ key: string; busy: boolean; error: string | null } | null>(null);
  // MusicBrainz (the owner, 2026-10-25: visible, without a key): how its questions stand, read as the part opens; a
  // server before it has no row.
  const MB_MORE =
    "Asked without a key, at its own pace: original years and release types for albums with a MusicBrainz release, Find more music, artists' links and Renumber's track order.";
  let mb = $state<MusicBrainzStatus | null>(null);
  // Asked again a few times when it did not answer (a busy server), so the row never just stays away; a server without
  // the call (missingCall) has none.
  let mbTries = 0;
  let mbRetry: ReturnType<typeof setTimeout> | undefined;
  const loadMb = () =>
    getMusicBrainz().then(
      (m) => (mb = m),
      (e) => {
        mb = null;
        if (!missingCall(e) && ++mbTries < 4) mbRetry = setTimeout(loadMb, 5000);
      },
    );
  if (offers('musicbrainz_settings')) loadMb();
  $effect(() => () => clearTimeout(mbRetry));
  /** Its last answer failed: an error later than the last answer. */
  const mbFailing = $derived(!!mb?.last_error && (mb.last_ok === null || mb.last_error.at > mb.last_ok));
  const mbLine = $derived(
    !mb
      ? null
      : !mb.enabled
        ? 'Off'
        : [mbFailing ? `Failing since ${when(mb.last_error!.at)}` : mb.last_ok ? `Reached ${when(mb.last_ok)}` : 'Not asked yet', mb.of ? `releases ${mb.asked.toLocaleString()} of ${mb.of.toLocaleString()}` : null]
            .filter(Boolean)
            .join(' · '),
  );
  // Each connection switched on or off (the owner, 2026-10-25: all of them), the key kept; a switch says it at once.
  async function turn(what: 'lastfm' | 'discogs' | 'musicbrainz' | 'slskd', on: boolean) {
    try {
      if (what === 'lastfm') await setLastfmEnabled(on).then(loadFmKey);
      else if (what === 'discogs') discogs = await setDiscogsEnabled(on);
      else if (what === 'slskd') slskd = await setSlskdEnabled(on);
      else mb = await setMusicBrainzEnabled(on);
    } catch (e) {
      toast.fail(e, on ? "It wasn't switched on" : "It wasn't switched off");
    }
  }
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
  const openSlskdForm = () => (slskdForm = { url: slskd?.url ?? '', key: '', folder: slskd?.folder || SLSKD_FOLDER, busy: false, error: null });
  // Kept when slskd did not answer, its error in the sheet's line.
  async function saveSlskd(f: SlskdForm) {
    f.busy = true;
    f.error = null;
    try {
      const key = f.key.trim();
      slskd = await setSlskd({ url: f.url.trim(), folder: f.folder.trim() || SLSKD_FOLDER, ...(key ? { key } : {}) });
      if (slskd.error) f.error = slskd.error;
      else slskdForm = null;
    } catch (e) {
      f.error = e instanceof Error ? e.message : String(e);
    } finally {
      f.busy = false;
    }
  }
  async function saveAcoustid(f: { key: string; busy: boolean; error: string | null }) {
    f.busy = true;
    f.error = null;
    try {
      acoustid = await setAcoustid(f.key.trim());
      acoustidForm = null;
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
  nameThing(() =>
    thing === LIDARR ? 'Lidarr' : thing === HOME_ASSISTANT ? 'Home Assistant' : thing === LASTFM ? 'Last.fm' : thing === DISCOGS ? 'Discogs' : thing === MUSICBRAINZ ? 'MusicBrainz' : thing === SLSKD ? 'slskd' : thing === ACOUSTID ? 'AcoustID' : draft ? label(draft) : null,
  );
  // Last.fm's key and Discogs' app as rows say them.
  const fmOff = $derived(fmKey?.enabled === false);
  const fmLine = $derived(fmOff ? 'Off' : fmKey?.saved ? 'Set, for everyone' : fmKey?.set ? "From the container's settings" : 'Not set');
  const discogsOff = $derived(discogs?.enabled === false);
  const discogsLine = $derived(discogsOff ? 'Off' : discogs?.set ? (discogs.per_minute ? `Set · ${discogs.per_minute} a minute` : 'Set') : 'Not set');
  // slskd's (its address and what it answered) and AcoustID's.
  const slskdHas = $derived(!!slskd?.url || !!slskd?.set);
  const slskdOff = $derived(slskd?.enabled === false);
  const slskdState = $derived.by(() => {
    if (slskdOff) return { lead: 'off', line: 'Off' };
    const said = slskd?.version ? `slskd ${slskd.version}` : (slskd?.error ?? null);
    return { lead: slskd?.error ? 'bad' : slskd?.version ? 'on' : 'off', line: [slskd?.url, said].filter(Boolean).join(' · ') || 'Not set' };
  });
  const acoustidLine = $derived(acoustid?.set ? 'Set' : 'Not set');
  /** Discovery and metadata's rows, those switched off last (as the servers). */
  const metaRows = $derived(
    [
      { id: MUSICBRAINZ, shown: !!mb, off: mb?.enabled === false },
      { id: LASTFM, shown: true, off: fmOff },
      { id: DISCOGS, shown: discogsKnown, off: discogsOff },
      { id: ACOUSTID, shown: acoustidKnown, off: false },
    ]
      .filter((r) => r.shown)
      .sort((a, b) => Number(a.off) - Number(b.off)),
  );
  function typeFmKey() {
    typing = {
      title: 'Last.fm API key',
      type: 'password',
      value: '',
      help: 'Made at last.fm/api.',
      save: async (v) => {
        await setLastfmServerKey(v);
        await loadFmKey();
      },
    };
  }
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
  let removing = $state<'server' | 'lidarr' | 'discogs' | 'lastfm' | 'slskd' | 'acoustid' | null>(null);
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
    <Group title="Adding albums" more={WANT_MORE}>
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
{:else if thing === MUSICBRAINZ}
  {#if mb}
    <Group>
      <Item name="Use MusicBrainz" line={mb.enabled ? null : 'Nothing is asked of it until it is on again'} toggle={{ checked: mb.enabled, onchange: (on) => turn('musicbrainz', on) }} />
    </Group>
    <Group title="Connection" more={MB_MORE}>
      <Item lead={!mb.enabled ? 'off' : mbFailing ? 'bad' : mb.last_ok ? 'on' : 'off'} name={mb.last_ok ? `Reached ${when(mb.last_ok)}` : 'Not reached yet'} line={mb.last_error ? `${when(mb.last_error.at)}: ${mb.last_error.message}` : null} tone={mbFailing ? 'bad' : null} wrap />
      <Item name="Pace" value={`One question every ${mb.pace_secs} s`} />
    </Group>
    {#if mb.passes.length}
      <Group title="In the background">
        {#each mb.passes as x (x.name)}
          <Item
            name={x.name}
            line={`${x.done.toLocaleString()} of ${x.total.toLocaleString()}${x.running ? '' : x.next_at ? ` · next ${when(x.next_at)}` : ''}`}
            progress={x.total ? x.done / x.total : null}
          />
        {/each}
      </Group>
    {/if}
  {/if}
{:else if thing === LASTFM}
  {#if fmKey?.saved || fmKey?.set}
    <Group>
      <Item name="Use Last.fm" line={fmOff ? 'Its key is kept, and not used until it is on again' : null} toggle={{ checked: !fmOff, onchange: (on) => turn('lastfm', on) }} />
    </Group>
  {/if}
  <Group title="Connection" more={LASTFM_MORE}>
    <Item name="API key" line={fmLine} onclick={typeFmKey} />
  </Group>
  {#if fmKey?.saved}
    <Group>
      <Item name="Remove Last.fm" kind="danger" onclick={() => (removing = 'lastfm')} />
    </Group>
  {/if}
  {#if removing === 'lastfm'}
    <ConfirmSheet
      title="Remove Last.fm?"
      label="Remove"
      onconfirm={() =>
        setLastfmServerKey('').then(
          () => (loadFmKey(), backFromThing('connections')),
          (e) => toast.fail(e, "Last.fm wasn't removed"),
        )}
      onclose={() => (removing = null)}
    />
  {/if}
{:else if thing === DISCOGS}
  {#if discogs?.set || discogsOff}
    <Group>
      <Item name="Use Discogs" line={discogsOff ? 'Its keys are kept, and nothing is asked of it until it is on again' : null} toggle={{ checked: !discogsOff, onchange: (on) => turn('discogs', on) }} />
    </Group>
  {/if}
  <Group title="Connection" more={DISCOGS_MORE}>
    <Item name="App key and secret" line={discogsLine} onclick={() => (discogsForm = { key: '', secret: '', busy: false, error: null })} />
  </Group>
  {#if discogs?.set || discogsOff}
    <Group>
      <Item name="Remove Discogs" kind="danger" onclick={() => (removing = 'discogs')} />
    </Group>
  {/if}
  {#if removing === 'discogs'}
    <ConfirmSheet
      title="Remove Discogs?"
      label="Remove"
      onconfirm={() =>
        setDiscogs('', '').then(
          (d) => ((discogs = d), backFromThing('connections')),
          (e) => toast.fail(e, "Discogs wasn't removed"),
        )}
      onclose={() => (removing = null)}
    />
  {/if}
{:else if thing === SLSKD}
  {#if slskdHas}
    <Group>
      <Item name="Use slskd" line={slskdOff ? 'Its key is kept, and nothing is asked of it until it is on again' : null} toggle={{ checked: !slskdOff, onchange: (on) => turn('slskd', on) }} />
    </Group>
  {/if}
  <Group title="Connection" more={SLSKD_MORE}>
    <Item name="Address" line={slskd?.url || 'Not set'} wrap={!!slskd?.url} onclick={openSlskdForm} />
    <Item name="API key" line={slskd?.set ? 'Set' : 'Not set'} onclick={openSlskdForm} />
    <Item name="Folder" line={slskd?.folder || SLSKD_FOLDER} wrap more="Its completed downloads as Cratarr sees them." onclick={openSlskdForm} />
    {#if slskd?.url && !slskdOff}
      <Item lead={slskdState.lead} name={slskd.version ? 'Connected' : 'Not connected'} line={slskd.version ? `slskd ${slskd.version}` : (slskd.error ?? null)} tone={slskd.error ? 'bad' : null} wrap />
    {/if}
  </Group>
  {#if slskdHas}
    <Group>
      <Item name="Remove slskd" kind="danger" onclick={() => (removing = 'slskd')} />
    </Group>
  {/if}
  {#if removing === 'slskd'}
    <ConfirmSheet
      title="Remove slskd?"
      label="Remove"
      onconfirm={() =>
        setSlskd({ url: '', key: '', folder: slskd?.folder || SLSKD_FOLDER }).then(
          (d) => ((slskd = d), backFromThing('connections')),
          (e) => toast.fail(e, "slskd wasn't removed"),
        )}
      onclose={() => (removing = null)}
    />
  {/if}
{:else if thing === ACOUSTID}
  <Group title="Connection" more={ACOUSTID_MORE}>
    <Item name="API key" line={acoustidLine} onclick={() => (acoustidForm = { key: '', busy: false, error: null })} />
  </Group>
  {#if acoustid?.set}
    <Group>
      <Item name="Remove AcoustID" kind="danger" onclick={() => (removing = 'acoustid')} />
    </Group>
  {/if}
  {#if removing === 'acoustid'}
    <ConfirmSheet
      title="Remove AcoustID?"
      label="Remove"
      onconfirm={() =>
        setAcoustid('').then(
          (d) => ((acoustid = d), backFromThing('connections')),
          (e) => toast.fail(e, "AcoustID wasn't removed"),
        )}
      onclose={() => (removing = null)}
    />
  {/if}
{:else}
  <Group title="Media servers" more="Shared by everyone who uses Cratarr, not this device's own.">
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
  {#if ed.showLidarr || slskdKnown}
    <Group title="Download stack" more={ed.showLidarr ? LIDARR_MORE : null}>
      {#if ed.showLidarr}
        {#if s.lidarr || ed.lid.url}
          <Item lead={lidarrState.lead} name="Lidarr" line={lidarrState.line} tone={lidarrState.lead === 'bad' ? 'bad' : null} onclick={() => openThing('connections', LIDARR)} />
        {:else}
          <Item name="Add Lidarr" kind="add" onclick={() => openThing('connections', LIDARR)} />
        {/if}
      {/if}
      {#if slskdKnown}
        {#if slskdHas}
          <Item lead={slskdState.lead} name="slskd" line={slskdState.line} tone={slskdState.lead === 'bad' ? 'bad' : null} onclick={() => openThing('connections', SLSKD)} />
        {:else}
          <Item name="Add slskd" kind="add" onclick={() => openThing('connections', SLSKD)} />
        {/if}
      {/if}
    </Group>
  {/if}
  <!-- Last.fm for Discover's artists, Discogs for covers, photos and genres: both bring what is known about music. -->
  <Group title="Discovery and metadata">
    {#each metaRows as r (r.id)}
      {#if r.id === MUSICBRAINZ && mb}
        <Item lead={!mb.enabled ? 'off' : mbFailing ? 'bad' : 'on'} name="MusicBrainz" line={mbLine} tone={mbFailing && mb.enabled ? 'bad' : null} onclick={() => openThing('connections', MUSICBRAINZ)} />
      {:else if r.id === LASTFM}
        {#if fmKey?.saved || fmKey?.set}
          <Item lead={fmOff ? 'off' : 'on'} name="Last.fm" line={fmLine} onclick={() => openThing('connections', LASTFM)} />
        {:else}
          <Item name="Add Last.fm" kind="add" onclick={() => openThing('connections', LASTFM)} />
        {/if}
      {:else if r.id === DISCOGS}
        {#if discogs?.set || discogsOff}
          <Item lead={discogsOff ? 'off' : 'on'} name="Discogs" line={discogsLine} onclick={() => openThing('connections', DISCOGS)} />
        {:else}
          <Item name="Add Discogs" kind="add" onclick={() => openThing('connections', DISCOGS)} />
        {/if}
      {:else if r.id === ACOUSTID}
        {#if acoustid?.set}
          <Item lead="on" name="AcoustID" line={acoustidLine} onclick={() => openThing('connections', ACOUSTID)} />
        {:else}
          <Item name="Add AcoustID" kind="add" onclick={() => openThing('connections', ACOUSTID)} />
        {/if}
      {/if}
    {/each}
  </Group>
  <HomeAssistantPart />
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
{#if slskdForm}
  {@const f = slskdForm}
  <!-- The key typed again only to change it: blank keeps the one kept. -->
  <FormSheet title="slskd" submitLabel="Save" canSubmit={!!f.url.trim() && (!!f.key.trim() || !!slskd?.set)} busy={f.busy} error={f.error} onsubmit={() => saveSlskd(f)} onclose={() => (slskdForm = null)}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>Address<input type="url" bind:value={f.url} placeholder="http://slskd:5030" autocomplete="off" spellcheck="false" autofocus /></label>
    <label>API key<input type="password" bind:value={f.key} placeholder={slskd?.set ? 'Kept' : ''} autocomplete="off" /></label>
    <label>Folder<input bind:value={f.folder} placeholder={SLSKD_FOLDER} autocomplete="off" spellcheck="false" /></label>
  </FormSheet>
{/if}
{#if acoustidForm}
  {@const f = acoustidForm}
  <FormSheet title="AcoustID" submitLabel="Save" canSubmit={!!f.key.trim()} busy={f.busy} error={f.error} onsubmit={() => saveAcoustid(f)} onclose={() => (acoustidForm = null)}>
    <!-- svelte-ignore a11y_autofocus -->
    <label>API key<input type="password" bind:value={f.key} autocomplete="off" autofocus /></label>
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
