<script lang="ts">
  import {
    ApiError,
    getCatalogue,
    getSettings,
    previewLidarr,
    previewSource,
    saveSettings,
    type Choice,
    type Field,
    type Kind,
    type LibraryUse,
    type LidarrView,
    type Settings,
    type SourceConfig,
    type SourceView,
    type TagRules,
    type CatalogueStatus,
  } from './lib/api';
  import { prefs, type Theme } from './lib/prefs.svelte';
  import Notice from './lib/Notice.svelte';
  import { toast } from './lib/toast.svelte';

  // A server (a source, in the code) as the page edits it. Secrets typed here are kept apart: the server never sends saved ones.
  type Draft = SourceConfig & { secrets: Record<string, string>; view: SourceView | null; busy: boolean; note: string | null };

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

  // Lidarr as the page edits it; the saved key is never sent here.
  let lid = $state({ url: '', key: '', view: null as LidarrView | null, busy: false, note: null as string | null, removing: false });

  function fill(next: Settings) {
    s = next;
    ruleText = {
      artistSeps: next.tag_rules.artist_separators.join(' '),
      genreSeps: next.tag_rules.genre_separators.join(' '),
      artistKeep: next.tag_rules.artist_keep.join('\n'),
      genreKeep: next.tag_rules.genre_keep.join('\n'),
    };
    cat = next.catalogue;
    lid = { url: next.lidarr?.url ?? '', key: '', view: next.lidarr, busy: false, note: null, removing: false };
    trashDays = next.trash_days;
    drafts = next.sources.map((v) => ({
      id: v.id,
      kind: v.kind,
      enabled: v.enabled,
      libraries: clone(v.libraries),
      values: clone(v.values),
      secrets: {},
      view: v,
      busy: false,
      note: null,
    }));
    addKind = next.kinds[0]?.id ?? '';
  }

  getSettings()
    .then(fill)
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
    lid.note = null;
    try {
      lid.view = await previewLidarr({ url: lid.url, api_key: lid.key });
      lid.note = lid.view?.error ?? null;
    } catch (e) {
      lid.note = e instanceof ApiError ? e.message : 'Unexpected error';
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
        JSON.stringify(drafts.map(payload)) !==
          JSON.stringify(s.sources.map((v) => payload({ ...v, secrets: {}, view: v, busy: false, note: null })))),
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
    d.note = null;
    try {
      d.view = await previewSource(payload(d));
      d.note = d.view.status.connected ? null : (d.view.status.error ?? d.view.error);
    } catch (e) {
      d.note = e instanceof ApiError ? e.message : 'Unexpected error';
    } finally {
      d.busy = false;
    }
  }

  function addServer(kind: string) {
    const k = s?.kinds.find((x) => x.id === kind);
    if (!k) return;
    let id = k.id;
    for (let n = 2; drafts.some((d) => d.id === id); n++) id = `${k.id}-${n}`;
    drafts.push({ id, kind: k.id, enabled: true, libraries: { music: [], incoming: [] }, values: {}, secrets: {}, view: null, busy: false, note: null });
  }

  async function save() {
    saving = true;
    try {
      const lidarr = lidarrChanged ? { url: lid.url.trim(), api_key: lid.key.trim() } : undefined;
      fill(await saveSettings(drafts.map(payload), { tag_rules: rules, lidarr, trash_days: trashDays }));
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

  // The catalogue's progress, while this page is open.
  $effect(() => {
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

<main>
  {#if loadError}<Notice text={loadError} />{/if}

  {#if s}
    {@const many = drafts.length > 1}
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
            <label class="switch"><input type="checkbox" bind:checked={d.enabled} /> On</label>
            {#if removing === d.id}
              <span class="ask">Remove this server?</span>
              <button class="danger" onclick={() => ((drafts = drafts.filter((x) => x !== d)), (removing = null))}>Remove</button>
              <button onclick={() => (removing = null)}>Cancel</button>
            {:else}
              <button onclick={() => (removing = d.id)}>Remove</button>
            {/if}
          </header>

          {#if line}<p class="state" class:bad={!line.ok}><span class="dot" class:ok={line.ok} aria-hidden="true"></span>{line.text}</p>{/if}
          {#if d.view?.playlist_sync}
            {@const ps = d.view.playlist_sync}
            <p class="state" class:bad={!!ps.error}>
              <span class="dot" class:ok={!ps.error} aria-hidden="true"></span>{ps.error
                ? `Playlists not in step: ${ps.error}`
                : `${ps.playlists} playlists in step, ${new Date(ps.at * 1000).toLocaleTimeString(undefined, { timeStyle: 'short' })}`}
            </p>
          {/if}

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
                      <label class="check">
                        <input
                          type="checkbox"
                          checked={chosen.includes(c.value)}
                          onchange={(e) =>
                            (d.values[f.key] = e.currentTarget.checked ? [...chosen, c.value] : chosen.filter((x) => x !== c.value))}
                        />{c.label}
                      </label>
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
            <button type="button" disabled={d.busy} onclick={() => check(d)}>Check connection</button>
            {#if d.note}<span class="hint bad">{d.note}</span>{/if}
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
            <button type="button" disabled={lid.busy || !lid.url.trim()} onclick={checkLidarr}>Check connection</button>
            {#if lid.note}<span class="hint bad">{lid.note}</span>{/if}
          </div>
        </article>
      {/if}
    </section>

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

      {#if drafts.some((d) => d.enabled && d.libraries.incoming.length)}
        <div class="sub fields">
          <label>
            <span class="l">Keep deleted Incoming albums for</span>
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
  {:else if !loadError}
    <p class="lede">Loading…</p>
  {/if}

  <!-- Shown with the rest, so the sections never jump around while the page loads. -->
  {#if s || loadError}
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

      <label class="pref check">
        <span class="l">Progress bar under the playing track</span>
        <input type="checkbox" bind:checked={prefs.showProgress} />
      </label>
    </section>
  {/if}

  {#if s}
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

  {#if dirty}
    <div class="savebar" role="region" aria-label="Unsaved changes">
      <span>Unsaved changes</span>
      <button class="primary" disabled={saving || !s?.settings_file} onclick={save}>Save</button>
      <button disabled={saving} onclick={() => s && fill(s)}>Discard</button>
    </div>
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 96px;
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
    gap: 6px;
  }

  label.check,
  label.switch {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 0.92rem;
    cursor: pointer;
  }

  label.check input,
  label.switch input {
    width: 20px;
    height: 20px;
    margin: 0;
    accent-color: var(--accent);
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

  .pref.check {
    display: flex;
    cursor: pointer;
  }

  .pref.check input {
    width: 22px;
    height: 22px;
    accent-color: var(--accent);
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

  /* Appears only with unsaved changes; above the phone's bottom bar. */
  .savebar {
    position: sticky;
    bottom: calc(80px + env(safe-area-inset-bottom));
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

  .days input {
    width: 6em;
  }
</style>
