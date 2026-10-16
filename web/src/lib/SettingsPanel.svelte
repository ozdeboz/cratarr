<script lang="ts">
  // A part of the settings in the panel (decided 2026-10-16: "the settings page should really be a portal with cards that
  // open settings-specific panels"): the Settings page's card opens it, as a playlist opens. What is edited here is
  // saved with the Save bar at its foot; Subsonic's accounts, this device's settings and Display act at once.
  import Switch from './Switch.svelte';
  import Icon from './Icon.svelte';
  import PanelPlace from './PanelPlace.svelte';
  import PlayersSettings from './PlayersSettings.svelte';
  import { closePlaylist } from './panel.svelte';
  import { SECTIONS, settingsRead, settingsSaved, type SettingsSection } from './settingsSections.svelte';

  let { section }: { section: SettingsSection } = $props();
  const title = $derived(section === 'device' ? (appInfo.player ?? 'This device') : (SECTIONS.find((x) => x.id === section)?.title ?? 'Settings'));

  import {
    ApiError,
    changeSubsonicPassword,
    checkNaming,
    createSubsonicAccount,
    getCatalogue,
    getImport,
    getImportSources,
    getSubsonicAccounts,
    getSettings,
    previewLidarr,
    previewSource,
    saveSettings,
    startImport,
    type Choice,
    type Field,
    type ImportJob,
    type ImportSource,
    type ImportWhat,
    type Kind,
    type LibraryUse,
    type LidarrView,
    type Settings,
    type SourceConfig,
    type SourceView,
    type TagRules,
    type CatalogueStatus,
    type NamingView,
  } from './api';
  import { prefs, type Theme } from './prefs.svelte';
  import { allowBackground, appInfo, askDevice, askDownloads, downloads, inApp, setDevice } from './app.svelte';

  // Inside the Android app: its settings as they are now (Android's background permission may have changed meanwhile).
  if (inApp) {
    askDevice();
    askDownloads();
  }
  import Meter from './Meter.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import { toast } from './toast.svelte';

  // The downloads' size, as This device shows it.
  const downloadedSize = (bytes: number) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`);

  // A server (a source, in the code) as the page edits it. Secrets typed here are kept apart: the server never sends saved ones.
  type Draft = SourceConfig & { secrets: Record<string, string>; view: SourceView | null; busy: boolean };

  let s = $state<Settings | null>(null);
  let drafts = $state<Draft[]>([]);
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let removing = $state<string | null>(null);
  let addKind = $state('');

  const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x));

  // Saved with the servers: how tags are split.
  // Edited as text (so Enter and spaces can be typed) and parsed when compared or saved.
  let ruleText = $state({ artistSeps: '', genreSeps: '', artistKeep: '', genreKeep: '' });
  let cat = $state<CatalogueStatus | null>(null);
  let trashDays = $state(14);

  // Music's naming template (decision 0013): the saved one, else the one the library follows, measured as it is typed.
  let naming = $state('');
  let namingInUse = $state('');
  let namingCheck = $state<NamingView | null>(null);
  let namingError = $state<string | null>(null);
  let namingTimer: ReturnType<typeof setTimeout> | undefined;

  async function loadNaming() {
    try {
      const v = await checkNaming();
      namingInUse = v.template;
      naming = v.template;
      namingCheck = v;
      namingError = null;
    } catch {
      /* no Music yet: the field stays hidden */
    }
  }

  function typedNaming() {
    clearTimeout(namingTimer);
    namingTimer = setTimeout(async () => {
      const typed = naming;
      try {
        const v = await checkNaming(typed);
        if (typed !== naming) return;
        namingCheck = v;
        namingError = null;
      } catch (e) {
        if (typed !== naming) return;
        namingCheck = null;
        namingError = e instanceof ApiError ? e.message : 'Unexpected error';
      }
    }, 300);
  }

  // Subsonic apps' accounts (decision 0030): acted on at once, not through the Save bar.
  let sub = $state({ accounts: null as string[] | null, user: '', current: '', password: '', busy: false });
  getSubsonicAccounts()
    .then((a) => (sub.accounts = a.accounts))
    .catch(() => (sub.accounts = null));

  async function createAccount() {
    sub.busy = true;
    try {
      sub.accounts = (await createSubsonicAccount(sub.user.trim(), sub.password)).accounts;
      sub.password = '';
      loadImports();
    } catch (e) {
      toast.fail(e, 'Could not create the account');
    } finally {
      sub.busy = false;
    }
  }

  async function changePassword() {
    if (!sub.accounts?.length) return;
    sub.busy = true;
    try {
      await changeSubsonicPassword(sub.accounts[0], sub.current, sub.password);
      sub.current = '';
      sub.password = '';
    } catch (e) {
      toast.fail(e, 'Could not change the password');
    } finally {
      sub.busy = false;
    }
  }

  // Importing plays and favourites from a server into a Subsonic account: the choices per server, and the import running
  // now or the last one (one at a time), followed while it runs.
  type ImportChoice = { user: string; account: string; what: ImportWhat };
  let imp = $state({ sources: [] as ImportSource[], accounts: [] as string[], choice: {} as Record<string, ImportChoice>, job: null as ImportJob | null });
  const IMPORT_WHAT: { key: keyof ImportWhat; label: string }[] = [
    { key: 'plays', label: 'Plays' },
    { key: 'favorites', label: 'Favourites' },
    { key: 'ratings', label: 'Ratings' },
    { key: 'added', label: 'Added dates' },
  ];
  const STEPS = { tracks: 'Tracks', groups: 'Favourite albums and artists', added: 'Added dates' };
  const importing = $derived(!!imp.job && imp.job.finished === null);

  async function loadImports() {
    try {
      const [srcs, last] = await Promise.all([getImportSources(), getImport()]);
      imp.accounts = srcs.accounts;
      for (const src of srcs.sources) {
        // Kept while the page is open, unless it had no account to go into yet.
        if (imp.choice[src.id]?.account) continue;
        imp.choice[src.id] = {
          user: src.user ?? src.users[0]?.id ?? '',
          account: src.account ?? srcs.accounts[0] ?? '',
          what: { plays: true, favorites: true, ratings: true, added: true, ...src.what },
        };
      }
      imp.sources = srcs.sources;
      imp.job = last.job;
    } catch {
      /* no servers to import from: the part stays hidden */
    }
  }
  loadImports();

  async function runImport(source: string, preview: boolean) {
    const c = imp.choice[source];
    try {
      imp.job = (await startImport({ source, user: c.user, account: c.account, what: { ...c.what }, preview })).job;
    } catch (e) {
      toast.fail(e, 'Could not start the import');
    }
  }

  // While an import runs, its progress.
  $effect(() => {
    if (!importing) return;
    const t = setInterval(async () => {
      try {
        imp.job = (await getImport()).job;
      } catch {
        /* the last known progress stays */
      }
    }, 2000);
    return () => clearInterval(t);
  });

  function importLine(j: ImportJob) {
    const c = j.counts;
    const parts = [
      c.plays && `${count(c.plays)} plays on ${count(c.songs)} ${c.songs === 1 ? 'song' : 'songs'}`,
      c.starred && `${count(c.starred)} ${c.starred === 1 ? 'favourite' : 'favourites'}`,
      c.unstarred && `${count(c.unstarred)} no longer ${c.unstarred === 1 ? 'a favourite' : 'favourites'}`,
      c.rated && `${count(c.rated)} ${c.rated === 1 ? 'rating' : 'ratings'}`,
      c.added && `${count(c.added)} added ${c.added === 1 ? 'date' : 'dates'}`,
      c.skipped && `${count(c.skipped)} not in the library`,
    ].filter(Boolean);
    const line = parts.length ? parts.join(' · ') : 'Nothing new';
    return j.preview ? `Preview: ${line}` : line;
  }

  function stepLine(j: ImportJob) {
    const st = j.step;
    if (!st) return { label: 'Starting', value: 0 };
    const of = st.total ? ` ${count(st.read)} of ${count(st.total)}` : st.read ? ` ${count(st.read)}` : '';
    return { label: `${STEPS[st.name]}${of}`, value: st.total ? st.read / st.total : 0 };
  }

  const count = (n: number) => n.toLocaleString();

  // Lidarr as the page edits it; the saved key is never sent here.
  let lid = $state({ url: '', key: '', view: null as LidarrView | null, busy: false, removing: false });

  function fill(next: Settings) {
    s = next;
    ruleText = {
      artistSeps: next.tag_rules.artist_separators.join(' '),
      genreSeps: next.tag_rules.genre_separators.join(' '),
      artistKeep: next.tag_rules.artist_keep.join('\n'),
      genreKeep: next.tag_rules.genre_keep.join('\n'),
    };
    cat = next.catalogue;
    lid = { url: next.lidarr?.url ?? '', key: '', view: next.lidarr, busy: false, removing: false };
    trashDays = next.trash_days;
    if (next.music_folder) loadNaming();
    drafts = next.sources.map((v) => ({
      id: v.id,
      kind: v.kind,
      enabled: v.enabled,
      libraries: clone(v.libraries),
      values: clone(v.values),
      secrets: {},
      view: v,
      busy: false,
    }));
    addKind = next.kinds[0]?.id ?? '';
  }

  // The last answer at once (the cards' read), then the server's latest, unless something was edited meanwhile.
  if (settingsRead.s) fill(settingsRead.s);
  getSettings()
    .then((v) => {
      settingsRead.s = v;
      if (!s || !dirty) fill(v);
    })
    .catch((e) => (loadError = e instanceof ApiError ? e.message : 'Unexpected error'));

  const kindOf = (d: { kind: string }): Kind | undefined => s?.kinds.find((k) => k.id === d.kind);
  const saved = (d: Draft) => s?.sources.find((v) => v.id === d.id && v.kind === d.kind);
  const str = (v: unknown) => (typeof v === 'string' ? v : '');

  /** What is sent: blank secrets are left out, so the saved ones are kept (where the server allows). */
  function payload(d: Draft): SourceConfig {
    const values = { ...d.values };
    for (const [k, v] of Object.entries(d.secrets)) if (v.trim()) values[k] = v.trim();
    return { id: d.id, kind: d.kind, enabled: d.enabled, libraries: clone(d.libraries), values };
  }

  const useOf = (d: Draft, id: string): LibraryUse =>
    d.libraries.music.includes(id) ? 'music' : d.libraries.incoming.includes(id) ? 'incoming' : 'unused';

  function setUse(d: Draft, id: string, use: LibraryUse) {
    d.libraries.music = d.libraries.music.filter((x) => x !== id);
    d.libraries.incoming = d.libraries.incoming.filter((x) => x !== id);
    if (use !== 'unused') d.libraries[use].push(id);
  }

  const USES: { id: LibraryUse; label: string }[] = [
    { id: 'music', label: 'Music' },
    { id: 'incoming', label: 'Incoming' },
    { id: 'unused', label: 'Not used' },
  ];

  const fromWords = (text: string) => text.split(/\s+/).filter(Boolean);
  const fromLines = (text: string) => text.split('\n').map((l) => l.trim()).filter(Boolean);
  const rules = $derived<TagRules>({
    artist_separators: fromWords(ruleText.artistSeps),
    genre_separators: fromWords(ruleText.genreSeps),
    artist_keep: fromLines(ruleText.artistKeep),
    genre_keep: fromLines(ruleText.genreKeep),
  });
  const lidarrChanged = $derived(!!s && (lid.url.trim().replace(/\/+$/, '') !== (s.lidarr?.url ?? '') || lid.key.trim() !== ''));
  /** The saved key is only reused for the same address. */
  const lidarrKeyNeeded = $derived(!!s?.lidarr?.key_set && lid.url.trim().replace(/\/+$/, '') !== s.lidarr.url && lid.url.trim() !== '');
  // Lidarr only matters to Incoming.
  const showLidarr = $derived(!!s?.lidarr || drafts.some((d) => d.enabled && d.libraries.incoming.length > 0));

  async function checkLidarr() {
    lid.busy = true;
    try {
      lid.view = await previewLidarr({ url: lid.url, api_key: lid.key });
      // The answer to a check is a toast, either way.
      if (lid.view?.error) toast.show({ text: `Lidarr: ${lid.view.error}`, tone: 'error' });
      else if (lid.view) toast.show({ text: `Connected to Lidarr${lid.view.version ? ` ${lid.view.version}` : ''}.` });
    } catch (e) {
      toast.fail(e, 'Could not check Lidarr');
    } finally {
      lid.busy = false;
    }
  }

  const dirty = $derived(
    !!s &&
      (drafts.some((d) => Object.values(d.secrets).some((v) => v.trim())) ||
        lidarrChanged ||
        JSON.stringify(rules) !== JSON.stringify(s.tag_rules) ||
        trashDays !== s.trash_days ||
        (!!namingInUse && naming.trim() !== namingInUse) ||
        JSON.stringify(drafts.map(payload)) !==
          JSON.stringify(s.sources.map((v) => payload({ ...v, secrets: {}, view: v, busy: false })))),
  );

  /** A secret must be typed again when the source is new, has none saved, or a field it is bound to changed. */
  function secretNeeded(d: Draft, f: Field) {
    const before = saved(d);
    if (!before || !before.secrets_set.includes(f.key)) return f.required;
    return f.bound_to.some((b) => str(d.values[b]) !== str(before.values[b]));
  }

  /** The choices for a field, always including the current value so nothing is silently dropped. */
  function choicesFor(d: Draft, f: Field): Choice[] {
    const list = d.view?.choices[f.key] ?? [];
    const cur = str(d.values[f.key]);
    return cur && !list.some((c) => c.value === cur) ? [{ value: cur, label: cur }, ...list] : list;
  }

  async function check(d: Draft) {
    d.busy = true;
    try {
      d.view = await previewSource(payload(d));
      // The answer to a check is a toast, either way.
      const st = d.view.status;
      if (st.connected) {
        const name = [st.server_name ?? kindOf(d)?.label ?? d.kind, st.version].filter(Boolean).join(' ');
        toast.show({ text: `Connected to ${name}.` });
      } else {
        toast.show({ text: st.error ?? d.view.error ?? 'Not connected', tone: 'error' });
      }
    } catch (e) {
      toast.fail(e, 'Could not check the connection');
    } finally {
      d.busy = false;
    }
  }

  function addServer(kind: string) {
    const k = s?.kinds.find((x) => x.id === kind);
    if (!k) return;
    let id = k.id;
    for (let n = 2; drafts.some((d) => d.id === id); n++) id = `${k.id}-${n}`;
    drafts.push({ id, kind: k.id, enabled: true, libraries: { music: [], incoming: [] }, values: {}, secrets: {}, view: null, busy: false });
  }

  async function save() {
    saving = true;
    try {
      const lidarr = lidarrChanged ? { url: lid.url.trim(), api_key: lid.key.trim() } : undefined;
      const namingChanged = !!namingInUse && naming.trim() !== namingInUse;
      fill(await saveSettings(drafts.map(payload), { tag_rules: rules, lidarr, trash_days: trashDays, naming: namingChanged ? naming.trim() : undefined }));
      settingsRead.s = s;
      settingsSaved.n++;
      toast.show({ text: 'Saved and in use.' });
    } catch (e) {
      toast.fail(e, 'Could not save');
    } finally {
      saving = false;
    }
  }

  function statusLine(v: SourceView | null) {
    if (!v) return null;
    const st = v.status;
    if (st.connected) return { ok: true, text: ['Connected to', st.server_name ?? v.label, st.version, st.detail].filter(Boolean).join(' ') };
    return { ok: false, text: st.error ?? v.error ?? 'Not connected' };
  }

  // The catalogue's progress, while Libraries shows.
  $effect(() => {
    if (section !== 'libraries') return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        cat = await getCatalogue();
      } catch {
        /* the status line keeps its last value */
      }
      if (!stopped) timer = setTimeout(poll, 5000);
    }
    timer = setTimeout(poll, 5000);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  const n = (x: number) => x.toLocaleString();
  const serverName = (id: string) => {
    const v = s?.sources.find((x) => x.id === id);
    return v ? (s!.sources.length > 1 ? `${v.label} (${id})` : v.label) : id;
  };
  function catalogueLine(c: CatalogueStatus) {
    const read = c.files - c.tags_pending;
    const parts = [`${n(c.files)} tracks`];
    if (c.tags_pending > 0) parts.push(`tags read for ${n(read)} (${Math.floor((100 * read) / Math.max(c.files, 1))}%)`);
    for (const [id, count] of Object.entries(c.linked)) parts.push(`${n(count)} known to ${serverName(id)}`);
    const problems = Object.values(c.issues).reduce((a, b) => a + (b ?? 0), 0);
    if (problems > 0) parts.push(`${n(problems)} with problems (see Repairs)`);
    const when = c.progress.last_pass ? new Date(c.progress.last_pass * 1000).toLocaleTimeString(undefined, { timeStyle: 'short' }) : null;
    return { parts, doing: c.progress.doing, when };
  }

  /** The labels of the given fields, for "you changed the server address". */
  const labelOf = (d: Draft, keys: string[]) =>
    keys.map((k) => kindOf(d)?.fields.find((f) => f.key === k)?.label.toLowerCase() ?? k).join(' and ');

  const THEMES: { id: Theme; label: string }[] = [
    { id: 'auto', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
  const since = (epoch: number) => new Date(epoch * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
</script>

<div class="view">
  <div class="bar">
    <span class="navpill"><button aria-label="Close" title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button></span>
    <h2 class="title">{title}</h2>
    <span class="end"><PanelPlace /></span>
  </div>
  <div class="scroll">
  {#if loadError}<Notice text={loadError} />{/if}

  <!-- Inside the Android app (decision 0031): its own settings, this device's only, at the top. -->
  {#if inApp}
    {#if section === 'device'}
    <section aria-labelledby="device-h" class="device">
      <h2 id="device-h">{appInfo.player ?? 'This device'}</h2>
      <p class="lede">This device only.</p>
      <label class="pref">
        <span class="l">Player name</span>
        <input type="text" value={appInfo.player ?? ''} onchange={(e) => e.currentTarget.value.trim() && setDevice({ player: e.currentTarget.value.trim() })} />
      </label>
      <div class="pref">
        <Switch label="Stay ready" hint="Keeps listening for Play On while idle" checked={appInfo.stayReady} onchange={(on) => setDevice({ stayReady: on })} />
      </div>
      <div class="pref">
        <span class="l">Background <small>{appInfo.batteryAllowed ? 'Allowed to run' : 'Android may stop Cratarr'}</small></span>
        {#if !appInfo.batteryAllowed}<button onclick={allowBackground}>Allow</button>{/if}
      </div>
      <label class="pref">
        <span class="l">Server</span>
        <input
          type="url"
          value={appInfo.server ?? ''}
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          onchange={(e) => {
            const server = e.currentTarget.value.trim();
            if (server && server !== appInfo.server) setDevice({ server });
          }}
        />
      </label>
      <label class="pref">
        <span class="l">Downloads <small>{downloads.tracks.length} tracks · {downloadedSize(downloads.bytes)}</small></span>
        <select value={appInfo.downloadQuality} onchange={(e) => setDevice({ downloadQuality: e.currentTarget.value })}>
          <option value="original">Originals</option>
          <option value="smaller">Smaller copies</option>
        </select>
      </label>
      <div class="pref">
        <Switch label="Download over mobile data" checked={appInfo.downloadOverMobile} onchange={(on) => setDevice({ downloadOverMobile: on })} />
      </div>
    </section>
    {/if}
  {/if}

  {#if section === 'players'}<PlayersSettings />{/if}

  {#if s}
    {@const many = drafts.length > 1}
    {#if section === 'connections'}
    <section aria-labelledby="conn-h">
      <h2 id="conn-h">Connections</h2>
      <p class="lede">
        The servers Cratarr works with. Shared by everyone who uses Cratarr.
        {#if s.origin === 'env'}Until you save here, these come from the container's settings.{/if}
      </p>

      {#each drafts as d (d)}
        {@const k = kindOf(d)}
        {@const line = statusLine(d.view)}
        <article class="server" class:off={!d.enabled}>
          <header>
            <h3>{k?.label ?? d.kind}{#if many}<small>{d.id}</small>{/if}</h3>
            <Switch label={`Use ${k?.label ?? d.kind}`} hideLabel checked={d.enabled} onchange={(on) => (d.enabled = on)} />
            {#if removing === d.id}
              <span class="ask">Remove this server?</span>
              <button class="danger" onclick={() => ((drafts = drafts.filter((x) => x !== d)), (removing = null))}>Remove</button>
              <button onclick={() => (removing = null)}>Cancel</button>
            {:else}
              <button onclick={() => (removing = d.id)}>Remove</button>
            {/if}
          </header>

          {#if line}<p class="state" class:bad={!line.ok}><span class="dot" class:ok={line.ok} aria-hidden="true"></span>{line.text}</p>{/if}

          <div class="fields">
            {#each k?.fields ?? [] as f (f.key)}
              <label>
                <span class="l">{f.label}{#if !f.required}<small>optional</small>{/if}</span>
                {#if f.kind === 'secret'}
                  {@const needed = secretNeeded(d, f)}
                  {@const isSaved = saved(d)?.secrets_set.includes(f.key)}
                  <input
                    type="password"
                    autocomplete="new-password"
                    bind:value={d.secrets[f.key]}
                    required={needed}
                    placeholder={isSaved && !needed ? 'Saved; leave empty to keep it' : ''}
                  />
                  {#if needed && isSaved}
                    <span class="hint warn">You changed the {labelOf(d, f.bound_to)}, so enter the {f.label.toLowerCase()} again.</span>
                  {/if}
                {:else if f.kind === 'choice' && choicesFor(d, f).length}
                  <select value={str(d.values[f.key])} onchange={(e) => (d.values[f.key] = e.currentTarget.value)} required={f.required}>
                    {#if !choicesFor(d, f).some((c) => c.value === '')}<option value="" disabled={f.required}>Choose…</option>{/if}
                    {#each choicesFor(d, f) as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
                  </select>
                {:else if f.kind === 'multi_choice'}
                  {@const chosen = Array.isArray(d.values[f.key]) ? (d.values[f.key] as string[]) : []}
                  <span class="checks">
                    {#each choicesFor(d, f) as c (c.value)}
                      <Switch
                        label={c.label}
                        checked={chosen.includes(c.value)}
                        onchange={(on) => (d.values[f.key] = on ? [...chosen, c.value] : chosen.filter((x) => x !== c.value))}
                      />
                    {/each}
                  </span>
                {:else}
                  <input
                    type={f.kind === 'url' ? 'url' : 'text'}
                    value={str(d.values[f.key])}
                    oninput={(e) => (d.values[f.key] = e.currentTarget.value)}
                    required={f.required}
                    spellcheck="false"
                    autocomplete="off"
                    placeholder={f.kind === 'choice' ? 'Check the connection first' : ''}
                  />
                {/if}
                {#if f.help}<span class="hint">{f.help}</span>{/if}
              </label>
            {/each}
          </div>

          <div class="row">
            <button type="button" disabled={d.busy} aria-busy={d.busy} onclick={() => check(d)}>Check connection</button>
          </div>
        </article>
      {:else}
        <p class="hint">No server yet.</p>
      {/each}

      {#if s.kinds.length === 1}
        <button class="add" onclick={() => addServer(s!.kinds[0].id)}>Add {s.kinds[0].label} server</button>
      {:else if s.kinds.length > 1}
        <div class="row">
          <select bind:value={addKind} aria-label="Kind of server">
            {#each s.kinds as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
          </select>
          <button class="add" onclick={() => addServer(addKind)}>Add server</button>
        </div>
      {/if}

      {#if showLidarr}
        <article class="server lidarr">
          <header>
            <h3>Lidarr<small>optional</small></h3>
            {#if s.lidarr && lid.url}
              {#if lid.removing}
                <span class="ask">Remove Lidarr?</span>
                <button class="danger" onclick={() => ((lid.url = ''), (lid.key = ''), (lid.view = null), (lid.removing = false))}>Remove</button>
                <button onclick={() => (lid.removing = false)}>Cancel</button>
              {:else}
                <button onclick={() => (lid.removing = true)}>Remove</button>
              {/if}
            {/if}
          </header>
          {#if lid.view && lid.url}
            <p class="state" class:bad={!lid.view.version}>
              <span class="dot" class:ok={!!lid.view.version} aria-hidden="true"></span>{lid.view.version ? `Connected to Lidarr ${lid.view.version}` : (lid.view.error ?? 'Not connected')}
            </p>
          {/if}
          <div class="fields">
            <label>
              <span class="l">Server address</span>
              <input type="url" bind:value={lid.url} spellcheck="false" autocomplete="off" placeholder="http://lidarr:8686" />
            </label>
            <label>
              <span class="l">API key</span>
              <input
                type="password"
                autocomplete="new-password"
                bind:value={lid.key}
                placeholder={s.lidarr?.key_set && !lidarrKeyNeeded ? 'Saved; leave empty to keep it' : ''}
              />
              {#if lidarrKeyNeeded}
                <span class="hint warn">You changed the server address, so enter the API key again.</span>
              {:else}
                <span class="hint">In Lidarr under Settings, General. Lets Delete in Incoming stop Lidarr fetching the album again.</span>
              {/if}
            </label>
          </div>
          <div class="row">
            <button type="button" disabled={lid.busy || !lid.url.trim()} aria-busy={lid.busy} onclick={checkLidarr}>Check connection</button>
          </div>
        </article>
      {/if}
    </section>
    {/if}

    {#if section === 'libraries'}
    <section aria-labelledby="lib-h">
      <h2 id="lib-h">Libraries</h2>
      <p class="lede">
        <strong>Music</strong>: the playlists and the tracks that can go in them. <strong>Incoming</strong>: new music
        waiting to be heard.
      </p>

      {#each drafts.filter((d) => d.enabled) as d (d)}
        <div class="libs-of">
          {#if many}<h3>{kindOf(d)?.label ?? d.kind}<small>{d.id}</small></h3>{/if}
          {#if !d.view}
            <p class="hint">Check the connection to see this server's libraries.</p>
          {:else}
            <ul class="libs">
              {#each d.view.server_libraries as l (l.id)}
                {@const u = useOf(d, l.id)}
                <li>
                  <div class="where">
                    <span>{l.name}</span>
                    {#if l.folders.length}
                      <code class="hint">{l.folders.join(', ')}</code>
                      {#if l.read_only && u !== 'unused'}
                        <span class="hint warn">
                          Cratarr can't write here, so {u === 'music' ? 'playlists can’t be saved' : 'albums can’t be deleted'}. Mount it
                          writable and give Cratarr's user write access.
                        </span>
                      {/if}
                    {:else if u !== 'unused'}
                      <span class="hint warn">
                        Cratarr can't see these files.{#if l.paths.length}
                          Mount {kindOf(d)?.label ?? d.kind}'s folder <code>{l.paths.join(', ')}</code> into Cratarr and restart it.{/if}
                      </span>
                    {/if}
                  </div>
                  <div class="seg" role="group" aria-label={`Use of ${l.name}`}>
                    {#each USES as x (x.id)}
                      <button aria-pressed={u === x.id} onclick={() => setUse(d, l.id, x.id)}>{x.label}</button>
                    {/each}
                  </div>
                </li>
              {:else}
                <li class="hint">No music libraries on this server.</li>
              {/each}
            </ul>
          {/if}
        </div>
      {:else}
        <p class="hint">Add a server under Connections first.</p>
      {/each}
      {#if drafts.some((d) => d.enabled && d.view) && !drafts.some((d) => d.enabled && d.libraries.music.length)}
        <p class="hint warn">Choose a Music library.</p>
      {/if}

      {#if namingInUse}
        <div class="sub fields">
          <label>
            <span class="l">Names in Music</span>
            <input type="text" class="mono" bind:value={naming} oninput={typedNaming} spellcheck="false" autocomplete="off" />
            {#if namingError}
              <span class="hint bad">{namingError}</span>
            {:else if namingCheck}
              <span class="hint">
                Matches {count(namingCheck.folders.matched)} of {count(namingCheck.folders.total)} folders and
                {count(namingCheck.files.matched)} of {count(namingCheck.files.total)} files.
              </span>
              {#if namingCheck.example}<code class="hint example">{namingCheck.example}</code>{/if}
            {/if}
          </label>
        </div>
      {/if}

      {#if drafts.some((d) => d.enabled && d.libraries.incoming.length)}
        <div class="sub fields">
          <label>
            <span class="l">Keep deleted albums for</span>
            <span class="days"><input type="number" min="1" max="3650" step="1" bind:value={trashDays} /> days</span>
          </label>
        </div>
      {/if}

      <div class="sub">
        <h3>Catalogue</h3>
        {#if cat}
          {@const line = catalogueLine(cat)}
          <p class="state"><span class="dot" class:ok={!line.doing && cat.progress.problems.length === 0} aria-hidden="true"></span>{line.parts.join(' · ')}</p>
          <p class="hint">{line.doing ? `Working: ${line.doing}…` : line.when ? `Up to date, checked at ${line.when}.` : 'Starting…'}</p>
          {#each cat.progress.problems as p (p)}<p class="hint bad">{p}</p>{/each}
        {:else}
          <p class="hint">Not available.</p>
        {/if}
      </div>

      <details class="sub">
        <summary>Tag splitting</summary>
        <p class="hint">How tags with several artists or genres in one value are read. The files are never changed.</p>
        <div class="fields">
          <label>
            <span class="l">Artist separators</span>
            <input type="text" bind:value={ruleText.artistSeps} spellcheck="false" />
            <span class="hint">Separated by spaces. "&amp;", "," and "feat." are left alone: they are part of credits.</span>
          </label>
          <label>
            <span class="l">Genre separators</span>
            <input type="text" bind:value={ruleText.genreSeps} spellcheck="false" />
          </label>
          <label>
            <span class="l">Artists never split</span>
            <textarea rows="3" bind:value={ruleText.artistKeep} spellcheck="false" placeholder="AC/DC"></textarea>
            <span class="hint">One per line.</span>
          </label>
          <label>
            <span class="l">Genres never split</span>
            <textarea rows="3" bind:value={ruleText.genreKeep} spellcheck="false" placeholder="Funk / Soul"></textarea>
          </label>
        </div>
      </details>
    </section>
    {/if}
  {:else if !loadError && section !== 'players'}
    <PageLoading />
  {/if}

  {#if s && sub.accounts}
    {#if section === 'subsonic'}
    <section aria-labelledby="sub-h">
      <h2 id="sub-h">Subsonic</h2>
      <p class="lede">Server address for Subsonic apps: <code>{location.origin}</code></p>
      {#if sub.accounts.length === 0}
        <div class="fields">
          <label>
            <span class="l">Username</span>
            <input type="text" bind:value={sub.user} spellcheck="false" autocomplete="username" autocapitalize="off" />
          </label>
          <label>
            <span class="l">Password</span>
            <input type="password" bind:value={sub.password} autocomplete="new-password" />
          </label>
        </div>
        <div class="row">
          <button class="primary" disabled={sub.busy || !sub.user.trim() || !sub.password} aria-busy={sub.busy} onclick={createAccount}>Create account</button>
        </div>
      {:else}
        <dl class="account">
          <dt>Account</dt>
          <dd>{sub.accounts.join(', ')}</dd>
        </dl>
        <div class="fields">
          <label>
            <span class="l">Current password</span>
            <input type="password" bind:value={sub.current} autocomplete="current-password" />
          </label>
          <label>
            <span class="l">New password</span>
            <input type="password" bind:value={sub.password} autocomplete="new-password" />
          </label>
        </div>
        <div class="row">
          <button disabled={sub.busy || !sub.current || !sub.password} aria-busy={sub.busy} onclick={changePassword}>Change password</button>
        </div>

        {#each imp.sources as src (src.id)}
          {@const c = imp.choice[src.id]}
          {@const job = imp.job?.source === src.id ? imp.job : null}
          <div class="sub import">
            <h3>Import from {serverName(src.id)}</h3>
            {#if src.error}
              <p class="hint bad">{src.error}</p>
            {:else if c}
              {@const none = !Object.values(c.what).some(Boolean) || !c.user || !c.account}
              <div class="fields pair">
                <label>
                  <span class="l">From</span>
                  <select bind:value={c.user}>
                    {#each src.users as u (u.id)}<option value={u.id}>{u.name}</option>{/each}
                  </select>
                </label>
                <label>
                  <span class="l">Into</span>
                  <select bind:value={c.account}>
                    {#each imp.accounts as a (a)}<option value={a}>{a}</option>{/each}
                  </select>
                </label>
              </div>
              <div class="checks">
                {#each IMPORT_WHAT as w (w.key)}
                  <Switch label={w.label} checked={c.what[w.key]} onchange={(on) => (c.what[w.key] = on)} />
                {/each}
              </div>
              <div class="row">
                <button disabled={importing || none} onclick={() => runImport(src.id, true)}>Preview</button>
                <button class="primary" disabled={importing || none} onclick={() => runImport(src.id, false)}>Import</button>
              </div>
              {#if job}
                {#if job.finished === null}
                  {@const st = stepLine(job)}
                  <Meter label={st.label} value={st.value} />
                {:else if job.error}
                  <p class="hint bad">{job.error}</p>
                {:else}
                  <p class="state"><span class="dot ok" aria-hidden="true"></span>{importLine(job)}</p>
                {/if}
                {#if job.note}<p class="hint warn">{job.note}</p>{/if}
              {/if}
            {/if}
          </div>
        {/each}
      {/if}
    </section>
    {/if}
  {/if}

  <!-- Shown with the rest, so the sections never jump around while the page loads. -->
  {#if s || loadError}
    {#if section === 'display'}
    <section aria-labelledby="display-h">
      <h2 id="display-h">Display</h2>
      <p class="lede">Remembered by this browser only.</p>

      <div class="pref">
        <span class="l" id="theme-l">Theme</span>
        <div class="seg" role="group" aria-labelledby="theme-l">
          {#each THEMES as t (t.id)}
            <button aria-pressed={prefs.theme === t.id} onclick={() => (prefs.theme = t.id)}>{t.label}</button>
          {/each}
        </div>
      </div>

      <div class="pref">
        <Switch label="Progress bar under the playing track" checked={prefs.showProgress} onchange={(on) => (prefs.showProgress = on)} />
      </div>
    </section>
    {/if}
  {/if}

  {#if s}
    {#if section === 'about'}
    <section aria-labelledby="about-h">
      <h2 id="about-h">About</h2>
      <dl>
        <dt>Cratarr</dt>
        <dd>{s.version}, running since {since(s.started)}</dd>
        {#if s.music_folder}
          <dt>Playlists</dt>
          <dd>{s.playlists} in <code>{s.music_folder}</code></dd>
        {/if}
        <dt>Port</dt>
        <dd><code>{s.fixed.bind}</code></dd>
        <dt>Settings file</dt>
        <dd>
          {#if s.settings_file}<code>{s.settings_file}</code>{:else}<span class="bad">None, so settings can't be saved. Set CRATARR_DATA_DIR.</span>{/if}
        </dd>
      </dl>
    </section>
    {/if}
  {/if}

  {#if dirty}
    <div class="savebar" role="region" aria-label="Unsaved changes">
      <span>Unsaved changes</span>
      <button class="primary" disabled={saving || !s?.settings_file} aria-busy={saving} onclick={save}>Save</button>
      <button disabled={saving} onclick={() => s && fill(s)}>Discard</button>
    </div>
  {/if}
  </div>
</div>

<style>
  /* The panel: its bar on top, the part under it scrolling (room left for the mini player). */
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .bar .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 20px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .end {
    display: flex;
    align-items: center;
  }

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px clamp(12px, 2vw, 24px) calc(96px + var(--mini-h, 0px));
  }

  /* The part's own heading is the bar's: kept for its name (aria-labelledby), not shown. */
  section > h2:first-child {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }

  section {
    margin-bottom: 20px;
    padding: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
  }

  h2 {
    margin: 0 0 6px;
    font-size: 1.05rem;
  }

  h3 {
    margin: 0;
    margin-right: auto;
    font-size: 0.98rem;
  }

  h3 small,
  .l small {
    margin-left: 6px;
    color: var(--muted);
    font-weight: 400;
    font-size: 0.8rem;
  }

  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85rem;
  }

  .example {
    overflow-wrap: anywhere;
  }

  .lede {
    margin: 0 0 14px;
    color: var(--muted);
    font-size: 0.88rem;
  }

  .lede strong {
    color: var(--text);
  }

  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 8px 16px;
    margin: 0;
    font-size: 0.92rem;
  }

  dt {
    color: var(--muted);
  }

  dl.account {
    margin-bottom: 14px;
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  @media (max-width: 520px) {
    dl {
      grid-template-columns: minmax(0, 1fr);
      gap: 2px;
    }

    dd {
      margin-bottom: 8px;
    }
  }

  code {
    font-size: 0.85em;
    overflow-wrap: anywhere;
  }

  .dot {
    display: inline-block;
    flex: none;
    width: 8px;
    height: 8px;
    margin-right: 8px;
    border-radius: 50%;
    background: var(--danger);
  }

  .dot.ok {
    background: var(--accent);
  }

  .bad {
    color: var(--danger);
  }

  .server {
    display: grid;
    gap: 14px;
    margin-bottom: 14px;
    padding: 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
  }

  .libs-of + .libs-of {
    margin-top: 12px;
  }

  .libs {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .libs li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    flex-wrap: wrap;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .libs li:last-child {
    border-bottom: 0;
  }

  /* Phones: every row the same, the name above and the choice full width below. */
  @media (max-width: 520px) {
    .libs li {
      flex-direction: column;
      align-items: stretch;
    }

    .libs .seg button {
      flex: 1;
    }
  }

  .where {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .where .warn code {
    color: inherit;
  }

  .sub {
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .sub h3 {
    margin-bottom: 6px;
  }

  details.sub summary {
    cursor: pointer;
    font-weight: 600;
    font-size: 0.98rem;
  }

  details.sub[open] summary {
    margin-bottom: 8px;
  }

  textarea {
    width: 100%;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    resize: vertical;
  }

  .server.off .fields {
    opacity: 0.6;
  }

  .server header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .ask {
    font-size: 0.9rem;
  }

  .state {
    display: flex;
    align-items: center;
    margin: 0;
    font-size: 0.88rem;
    overflow-wrap: anywhere;
  }

  .fields {
    display: grid;
    gap: 14px;
  }

  label {
    display: grid;
    gap: 6px;
  }

  .l {
    font-size: 0.9rem;
    font-weight: 600;
  }

  .hint {
    color: var(--muted);
    font-size: 0.8rem;
  }

  .hint.warn {
    color: var(--warn);
  }

  .hint.bad {
    color: var(--danger);
  }

  .checks {
    display: grid;
  }

  .pref :global(.switch .label) {
    font-size: 0.9rem;
    font-weight: 600;
  }

  .checks :global(.switch .label) {
    font-size: 0.92rem;
  }

  input[type='url'],
  input[type='number'],
  input[type='text'],
  input[type='password'],
  select {
    width: 100%;
    min-height: 44px;
    padding: 0 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .row select {
    width: auto;
  }

  button {
    min-height: 44px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .server button {
    background: var(--surface);
  }

  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button.danger {
    border-color: var(--danger);
    color: var(--danger);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .pref {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    min-height: 52px;
    border-bottom: 1px solid var(--border);
  }

  .pref:last-child {
    border-bottom: 0;
  }

  .seg {
    display: flex;
    align-items: center;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }

  .seg button {
    border: 0;
    border-radius: 0;
  }

  .seg button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  /* Appears only with unsaved changes, at the bottom of the screen. */
  .savebar {
    position: sticky;
    bottom: calc(8px + env(safe-area-inset-bottom));
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--surface);
    border: 1px solid var(--accent);
    border-radius: 12px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  .savebar span {
    margin-right: auto;
    font-weight: 600;
  }

  @media (min-width: 1024px) {
    .savebar {
      bottom: 16px;
    }
  }

  .days {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .import {
    display: grid;
    gap: 14px;
  }

  .import h3 {
    margin-bottom: 0;
  }

  .import .state,
  .import .hint {
    margin: 0;
  }

  @media (min-width: 521px) {
    .fields.pair {
      grid-template-columns: 1fr 1fr;
    }
  }

  .days input {
    width: 6em;
  }
</style>
