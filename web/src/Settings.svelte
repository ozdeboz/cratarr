<script lang="ts">
  import {
    ApiError,
    getSettings,
    previewSource,
    saveSettings,
    type Choice,
    type Field,
    type Kind,
    type Settings,
    type SourceConfig,
    type SourceView,
  } from './lib/api';
  import { prefs, resetPrefs, ZOOM_STEPS, type Theme } from './lib/prefs.svelte';

  // A source as the page edits it. Secrets typed here are kept apart: the server never sends saved ones.
  type Draft = SourceConfig & { secrets: Record<string, string>; view: SourceView | null; busy: boolean; note: string | null };

  let s = $state<Settings | null>(null);
  let drafts = $state<Draft[]>([]);
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let formError = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let removing = $state<string | null>(null);
  let addKind = $state('');

  const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x));

  function fill(next: Settings) {
    s = next;
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

  const dirty = $derived(
    !!s &&
      (drafts.some((d) => Object.values(d.secrets).some((v) => v.trim())) ||
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

  function addSource() {
    const k = s?.kinds.find((x) => x.id === addKind);
    if (!k) return;
    let id = k.id;
    for (let n = 2; drafts.some((d) => d.id === id); n++) id = `${k.id}-${n}`;
    drafts.push({ id, kind: k.id, enabled: true, libraries: { music: [], incoming: null }, values: {}, secrets: {}, view: null, busy: false, note: null });
  }

  async function save() {
    formError = null;
    notice = null;
    saving = true;
    try {
      fill(await saveSettings(drafts.map(payload)));
      notice = 'Saved and in use.';
    } catch (e) {
      formError = e instanceof ApiError ? e.message : 'Unexpected error';
    } finally {
      saving = false;
    }
  }

  // Music libraries: automatic (every library inside the playlist folder) or a chosen set.
  function setAutomatic(d: Draft, auto: boolean) {
    const inside = (d.view?.placement.libraries ?? []).filter((l) => l.inside && l.id !== d.libraries.incoming);
    d.libraries.music = auto ? [] : inside.map((l) => l.id);
  }

  function toggleMusic(d: Draft, id: string, on: boolean) {
    d.libraries.music = on ? [...d.libraries.music, id] : d.libraries.music.filter((x) => x !== id);
  }

  function statusLine(v: SourceView | null) {
    if (!v) return null;
    const st = v.status;
    if (st.connected) return { ok: true, text: ['Connected to', st.server_name ?? v.label, st.version, st.detail].filter(Boolean).join(' ') };
    return { ok: false, text: st.error ?? v.error ?? 'Not connected' };
  }

  const THEMES: { id: Theme; label: string }[] = [
    { id: 'auto', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
  const since = (epoch: number) => new Date(epoch * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
</script>

<main>
  <h1>Settings</h1>

  {#if loadError}<div class="banner error" role="alert">{loadError}</div>{/if}

  {#if s}
    <section aria-labelledby="status-h">
      <h2 id="status-h">Status</h2>
      <dl>
        {#each s.sources as v (v.id)}
          {@const line = statusLine(v)}
          <dt>{v.label}{s.sources.length > 1 ? ` (${v.id})` : ''}</dt>
          <dd>
            {#if !v.enabled}<span class="muted">Switched off</span>
            {:else if line}<span class="dot" class:ok={line.ok} aria-hidden="true"></span><span class:bad={!line.ok}>{line.text}</span>{/if}
          </dd>
        {:else}
          <dt>Sources</dt>
          <dd class="bad">None yet: add one below</dd>
        {/each}
        <dt>Playlists</dt>
        <dd>{s.playlists} in <code>{s.fixed.playlist_dir}</code></dd>
        <dt>Listening on</dt>
        <dd><code>{s.fixed.bind}</code></dd>
        <dt>Settings file</dt>
        <dd>
          {#if s.settings_file}<code>{s.settings_file}</code>{:else}<span class="bad">None: set CRATARR_DATA_DIR to save settings here</span>{/if}
        </dd>
        <dt>Cratarr</dt>
        <dd>{s.version}, running since {since(s.started)}</dd>
      </dl>
    </section>

    <section aria-labelledby="sources-h">
      <h2 id="sources-h">Sources</h2>
      <p class="lede">
        The servers Cratarr works alongside. Shared by everyone who uses Cratarr.
        {#if s.origin === 'env'}These come from the container's settings until you save here.{/if}
        Saved secrets are never shown; leave them empty to keep them.
      </p>

      {#each drafts as d (d)}
        {@const k = kindOf(d)}
        {@const line = statusLine(d.view)}
        {@const libs = d.view?.placement.libraries ?? []}
        <article class="source" class:off={!d.enabled}>
          <header>
            <h3>{k?.label ?? d.kind} <small>{d.id}</small></h3>
            <label class="switch"><input type="checkbox" bind:checked={d.enabled} /> On</label>
            {#if removing === d.id}
              <button class="danger" onclick={() => ((drafts = drafts.filter((x) => x !== d)), (removing = null))}>Remove</button>
              <button onclick={() => (removing = null)}>Keep</button>
            {:else}
              <button onclick={() => (removing = d.id)}>Remove…</button>
            {/if}
          </header>

          {#if line}<p class="state" class:bad={!line.ok}><span class="dot" class:ok={line.ok} aria-hidden="true"></span>{line.text}</p>{/if}

          <div class="fields">
            {#each k?.fields ?? [] as f (f.key)}
              <label>
                <span class="l">{f.label}{#if !f.required}<small>optional</small>{/if}</span>
                {#if f.kind === 'secret'}
                  {@const needed = secretNeeded(d, f)}
                  <input
                    type="password"
                    autocomplete="new-password"
                    bind:value={d.secrets[f.key]}
                    required={needed}
                    placeholder={saved(d)?.secrets_set.includes(f.key) && !needed ? 'Saved; leave empty to keep it' : ''}
                  />
                  {#if needed && saved(d)?.secrets_set.includes(f.key)}
                    <span class="hint warn">Changed {f.bound_to.join(', ')}: enter it again, so the saved one is never sent somewhere new.</span>
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
                    placeholder={f.kind === 'choice' ? 'Connect first to choose' : ''}
                  />
                {/if}
                {#if f.help}<span class="hint">{f.help}</span>{/if}
              </label>
            {/each}
          </div>

          <div class="row">
            <button type="button" disabled={d.busy} onclick={() => check(d)}>{d.view ? 'Check again' : 'Connect'}</button>
            {#if d.note}<span class="hint bad">{d.note}</span>{/if}
          </div>

          {#if d.view}
            <fieldset>
              <legend>Music</legend>
              <p class="hint">Tracks in these libraries can go in playlists.</p>
              <label class="check"><input type="radio" checked={d.libraries.music.length === 0} onchange={() => setAutomatic(d, true)} />Every library inside the playlist folder</label>
              <label class="check"><input type="radio" checked={d.libraries.music.length > 0} onchange={() => setAutomatic(d, false)} />Only these:</label>
              {#if d.libraries.music.length > 0}
                <div class="checks indent">
                  {#each libs.filter((l) => l.id !== d.libraries.incoming) as l (l.id)}
                    <label class="check" class:muted={!l.inside}>
                      <input type="checkbox" disabled={!l.inside} checked={d.libraries.music.includes(l.id)} onchange={(e) => toggleMusic(d, l.id, e.currentTarget.checked)} />
                      {l.name}{#if !l.inside}<small>outside the playlist folder</small>{/if}
                    </label>
                  {/each}
                </div>
              {/if}
              {#if d.view.placement.root}
                <p class="hint">This source sees the playlist folder as <code>{d.view.placement.root}</code>.</p>
              {:else}
                <p class="hint bad">None of this source's music libraries is inside the playlist folder ({s.fixed.playlist_dir}).</p>
              {/if}
            </fieldset>

            <fieldset>
              <legend>Incoming <small>optional</small></legend>
              <p class="hint">Where new music waits for review (Lidarr's sandbox, say). Never added to playlists; choosing one turns on incoming-music features.</p>
              <select value={d.libraries.incoming ?? ''} onchange={(e) => (d.libraries.incoming = e.currentTarget.value || null)}>
                <option value="">None</option>
                {#each libs.filter((l) => !d.libraries.music.includes(l.id)) as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
              </select>
            </fieldset>
          {/if}
        </article>
      {/each}

      {#if s.kinds.length}
        <div class="row add">
          <select bind:value={addKind} aria-label="Kind of source">
            {#each s.kinds as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
          </select>
          <button onclick={addSource}>Add source</button>
        </div>
      {/if}

      {#if formError}<div class="banner error" role="alert">{formError}</div>{/if}
      {#if notice}<div class="banner" role="status">{notice}</div>{/if}

      <div class="row">
        <button class="primary" disabled={saving || !dirty || !s.settings_file} onclick={save}>Save</button>
        <button disabled={saving || !dirty} onclick={() => s && fill(s)}>Discard changes</button>
      </div>
    </section>
  {:else if !loadError}
    <p class="lede">Loading…</p>
  {/if}

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
      <span class="l" id="sort-l">Playlist order</span>
      <div class="seg" role="group" aria-labelledby="sort-l">
        <button aria-pressed={!prefs.membersFirst} onclick={() => (prefs.membersFirst = false)}>A–Z</button>
        <button aria-pressed={prefs.membersFirst} onclick={() => (prefs.membersFirst = true)}>In first</button>
      </div>
    </div>

    <div class="pref">
      <span class="l" id="zoom-l">Playlist size</span>
      <div class="seg" role="group" aria-labelledby="zoom-l">
        <button aria-label="Smaller" disabled={prefs.zoom === 0} onclick={() => prefs.zoom--}>−</button>
        <span class="val">{prefs.zoom + 1} / {ZOOM_STEPS}</span>
        <button aria-label="Larger" disabled={prefs.zoom === ZOOM_STEPS - 1} onclick={() => prefs.zoom++}>+</button>
      </div>
    </div>

    <label class="pref check">
      <span class="l">Progress bar under the playing track</span>
      <input type="checkbox" bind:checked={prefs.showProgress} />
    </label>

    <div class="row">
      <button onclick={resetPrefs}>Reset display</button>
    </div>
  </section>
</main>

<style>
  main {
    max-width: 760px;
    padding: 8px clamp(12px, 2vw, 32px) 48px;
  }

  h1 {
    margin: 16px 0 4px;
    font-size: 1.25rem;
  }

  section {
    margin-top: 20px;
    padding: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
  }

  h2 {
    margin: 0 0 8px;
    font-size: 1rem;
  }

  h3 {
    margin: 0;
    margin-right: auto;
    font-size: 0.98rem;
  }

  h3 small,
  .l small,
  legend small,
  .check small {
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
    vertical-align: middle;
  }

  .dot.ok {
    background: var(--accent);
  }

  .bad {
    color: var(--danger);
  }

  .muted {
    color: var(--muted);
  }

  .source {
    display: grid;
    gap: 14px;
    margin-bottom: 14px;
    padding: 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
  }

  .source.off .fields,
  .source.off fieldset {
    opacity: 0.6;
  }

  .source header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
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

  fieldset {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  legend {
    padding: 0 6px;
    font-size: 0.9rem;
    font-weight: 600;
  }

  fieldset p {
    margin: 0;
  }

  .checks {
    display: grid;
    gap: 6px;
  }

  .indent {
    padding-left: 28px;
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

  .row.add {
    margin-bottom: 14px;
  }

  .row.add select {
    width: auto;
  }

  button {
    min-height: 44px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    cursor: pointer;
  }

  .source button {
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

  .banner {
    margin: 12px 0;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    font-size: 0.9rem;
  }

  .banner.error {
    border-color: var(--danger);
    color: var(--danger);
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

  .pref.check {
    display: flex;
    cursor: pointer;
  }

  .pref.check input {
    width: 22px;
    height: 22px;
    accent-color: var(--accent);
  }

  .pref + .row {
    margin-top: 14px;
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

  .val {
    min-width: 4.5em;
    text-align: center;
    font-variant-numeric: tabular-nums;
    font-size: 0.9rem;
  }
</style>
