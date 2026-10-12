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
  import { prefs, type Theme } from './lib/prefs.svelte';
  import Notice, { type NoticeData } from './lib/Notice.svelte';

  // A server (a source, in the code) as the page edits it. Secrets typed here are kept apart: the server never sends saved ones.
  type Draft = SourceConfig & { secrets: Record<string, string>; view: SourceView | null; busy: boolean; note: string | null };

  let s = $state<Settings | null>(null);
  let drafts = $state<Draft[]>([]);
  let loadError = $state<string | null>(null);
  let saving = $state(false);
  let notice = $state<NoticeData | null>(null);
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

  function addServer(kind: string) {
    const k = s?.kinds.find((x) => x.id === kind);
    if (!k) return;
    let id = k.id;
    for (let n = 2; drafts.some((d) => d.id === id); n++) id = `${k.id}-${n}`;
    drafts.push({ id, kind: k.id, enabled: true, libraries: { music: [], incoming: null }, values: {}, secrets: {}, view: null, busy: false, note: null });
  }

  async function save() {
    notice = null;
    saving = true;
    try {
      fill(await saveSettings(drafts.map(payload)));
      notice = { text: 'Saved and in use.' };
    } catch (e) {
      notice = { text: e instanceof ApiError ? e.message : 'Unexpected error', tone: 'error' };
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
  {#if loadError}<Notice notice={{ text: loadError, tone: 'error' }} />{/if}
  <Notice bind:notice />

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
    </section>

    <section aria-labelledby="lib-h">
      <h2 id="lib-h">Libraries</h2>
      <p class="lede">
        <strong>Music</strong>: tracks that can go in playlists. <strong>Incoming</strong>: new music waiting to be
        listened to; never added to playlists.
      </p>

      {#each drafts.filter((d) => d.enabled) as d (d)}
        {@const libs = d.view?.placement.libraries ?? []}
        <!-- With one server its frame adds nothing: the section already is the frame. -->
        <article class="server" class:plain={!many}>
          {#if many}<h3>{kindOf(d)?.label ?? d.kind}<small>{d.id}</small></h3>{/if}
          {#if !d.view}
            <p class="hint">Check the connection to see this server's libraries.</p>
          {:else}
            <fieldset>
              <legend>Music</legend>
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
              {#if !d.view.placement.root}
                <p class="hint bad">None of this server's libraries is inside the playlist folder, so nothing can be added to playlists.</p>
              {/if}
            </fieldset>

            <fieldset>
              <legend>Incoming <small>optional</small></legend>
              <select value={d.libraries.incoming ?? ''} onchange={(e) => (d.libraries.incoming = e.currentTarget.value || null)} aria-label="Incoming library">
                <option value="">None</option>
                {#each libs.filter((l) => !d.libraries.music.includes(l.id)) as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
              </select>
            </fieldset>
          {/if}
        </article>
      {:else}
        <p class="hint">Add a server under Connections first.</p>
      {/each}
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
        <dt>Playlists</dt>
        <dd>{s.playlists} in <code>{s.fixed.playlist_dir}</code></dd>
        {#each s.sources.filter((v) => v.placement.root) as v (v.id)}
          <dt>{v.label}{s.sources.length > 1 ? ` (${v.id})` : ''}</dt>
          <dd>sees that folder as <code>{v.placement.root}</code></dd>
        {/each}
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

  .muted {
    color: var(--muted);
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

  .server.plain {
    padding: 0;
    background: none;
    border: 0;
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
</style>
