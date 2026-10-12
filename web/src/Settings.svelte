<script lang="ts">
  import { ApiError, getSettings, probe, saveSettings, type Probe, type Settings, type Source } from './lib/api';
  import { prefs, resetPrefs, ZOOM_STEPS, type Theme } from './lib/prefs.svelte';

  let s = $state<Settings | null>(null);
  let loadError = $state<string | null>(null);

  // The connection form. `form` is what the page shows; `s.values` is what the server runs with.
  let form = $state({ jellyfin_url: '', api_key: '', user_id: '', player_client: '', prefixes: '', jellyfin_playlist_dir: '' });
  let users = $state<Probe['users']>([]);
  let probed = $state<Probe | null>(null);
  let busy = $state(false);
  let formError = $state<string | null>(null);
  let notice = $state<string | null>(null);

  function fill(v: Settings) {
    s = v;
    form = {
      jellyfin_url: v.values.jellyfin_url,
      api_key: '',
      user_id: v.values.user_id,
      player_client: v.values.player_client,
      prefixes: v.values.music_prefixes.join('\n'),
      jellyfin_playlist_dir: v.values.jellyfin_playlist_dir,
    };
  }

  const lines = (t: string) =>
    t
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean);

  async function load() {
    try {
      fill(await getSettings());
      loadError = null;
      // Fill the user list from the saved connection; failure just leaves the plain id field.
      test(true);
    } catch (e) {
      loadError = e instanceof ApiError ? e.message : 'Unexpected error';
    }
  }
  load();

  const urlChanged = $derived(!!s && form.jellyfin_url.trim().replace(/\/+$/, '') !== s.values.jellyfin_url);
  const dirty = $derived(
    !!s &&
      (urlChanged ||
        form.api_key.trim() !== '' ||
        form.user_id.trim() !== s.values.user_id ||
        form.player_client.trim() !== s.values.player_client ||
        lines(form.prefixes).join('\n') !== s.values.music_prefixes.join('\n') ||
        form.jellyfin_playlist_dir.trim() !== s.values.jellyfin_playlist_dir),
  );

  async function test(quiet = false) {
    if (!quiet) {
      formError = null;
      notice = null;
    }
    busy = true;
    try {
      probed = await probe(form.jellyfin_url, form.api_key || undefined);
      users = probed.users;
      if (!quiet) notice = `Reached ${probed.server_name ?? 'Jellyfin'} ${probed.version}, ${users.length} ${users.length === 1 ? 'user' : 'users'}.`;
    } catch (e) {
      probed = null;
      if (!quiet) formError = e instanceof ApiError ? e.message : 'Unexpected error';
    } finally {
      busy = false;
    }
  }

  async function save() {
    formError = null;
    notice = null;
    busy = true;
    try {
      fill(
        await saveSettings({
          jellyfin_url: form.jellyfin_url,
          api_key: form.api_key || undefined,
          user_id: form.user_id,
          player_client: form.player_client,
          music_prefixes: lines(form.prefixes),
          jellyfin_playlist_dir: form.jellyfin_playlist_dir,
        }),
      );
      notice = 'Saved and in use.';
    } catch (e) {
      formError = e instanceof ApiError ? e.message : 'Unexpected error';
    } finally {
      busy = false;
    }
  }

  const SOURCE: Record<Source, string> = { file: 'Saved here', env: 'From the container', default: 'Default' };
  const THEMES: { id: Theme; label: string }[] = [
    { id: 'auto', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
  const since = (epoch: number) =>
    new Date(epoch * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  // The saved user is always offered, even when the list could not be loaded or no longer has it.
  const userOptions = $derived(
    users.some((u) => u.Id === form.user_id) || !form.user_id ? users : [{ Id: form.user_id, Name: form.user_id }, ...users],
  );
</script>

<main>
  <h1>Settings</h1>

  {#if loadError}<div class="banner error" role="alert">{loadError}</div>{/if}

  {#if s}
    <section aria-labelledby="status-h">
      <h2 id="status-h">Status</h2>
      <dl>
        <dt>Jellyfin</dt>
        <dd>
          {#if s.jellyfin.ok}
            <span class="dot ok" aria-hidden="true"></span>Connected to {s.jellyfin.server_name ?? 'Jellyfin'}
            {s.jellyfin.version} as {s.jellyfin.user_name}
          {:else}
            <span class="dot down" aria-hidden="true"></span><span class="bad">{s.jellyfin.error ?? 'Not connected'}</span>
          {/if}
        </dd>
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

    <section aria-labelledby="conn-h">
      <h2 id="conn-h">Jellyfin connection</h2>
      <p class="lede">
        Shared by everyone who uses Cratarr. Values saved here win over the container's; saving one back to the
        container's value removes the override. The playlist folder and port belong to the container and are set there.
      </p>

      <form
        onsubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        <label>
          <span class="l">Server address <small>{SOURCE[s.sources.jellyfin_url]}</small></span>
          <input type="url" required bind:value={form.jellyfin_url} autocomplete="off" spellcheck="false" />
        </label>

        <label>
          <span class="l">API key <small>{SOURCE[s.sources.api_key]}</small></span>
          <input
            type="password"
            bind:value={form.api_key}
            autocomplete="new-password"
            placeholder={s.values.api_key_set ? 'Saved; leave empty to keep it' : 'Not set'}
            required={urlChanged}
          />
          {#if urlChanged}<span class="hint">A new address needs the key entered again, so the saved key is never sent to another server.</span>{/if}
        </label>

        <div class="row">
          <button type="button" disabled={busy || !form.jellyfin_url} onclick={() => test()}>Test connection</button>
          {#if probed}<span class="hint">{probed.server_name ?? 'Jellyfin'} {probed.version}</span>{/if}
        </div>

        <label>
          <span class="l">User whose library is used <small>{SOURCE[s.sources.user_id]}</small></span>
          {#if userOptions.length}
            <select bind:value={form.user_id} required>
              {#each userOptions as u (u.Id)}<option value={u.Id}>{u.Name}</option>{/each}
            </select>
          {:else}
            <input type="text" required bind:value={form.user_id} spellcheck="false" placeholder="User id" />
          {/if}
        </label>

        <label>
          <span class="l">Preferred player <small>{SOURCE[s.sources.player_client]}</small></span>
          <input type="text" bind:value={form.player_client} placeholder="Finamp" spellcheck="false" />
          <span class="hint">When several players are playing music, this one's track counts.</span>
        </label>

        <label>
          <span class="l">Library folders, as Jellyfin sees them <small>{SOURCE[s.sources.music_prefixes]}</small></span>
          <textarea rows="3" required bind:value={form.prefixes} spellcheck="false"></textarea>
          <span class="hint">One per line. Tracks outside these (an incoming folder, say) cannot be added to playlists.</span>
        </label>

        <label>
          <span class="l">Playlist folder, as Jellyfin sees it <small>{SOURCE[s.sources.jellyfin_playlist_dir]}</small></span>
          <input type="text" required bind:value={form.jellyfin_playlist_dir} spellcheck="false" />
        </label>

        {#if formError}<div class="banner error" role="alert">{formError}</div>{/if}
        {#if notice}<div class="banner" role="status">{notice}</div>{/if}

        <div class="row">
          <button class="primary" type="submit" disabled={busy || !dirty || !s.settings_file}>Save</button>
          <button type="button" disabled={busy || !dirty} onclick={() => s && fill(s)}>Discard changes</button>
        </div>
      </form>
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
  }

  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 8px;
    border-radius: 50%;
    vertical-align: middle;
  }

  .dot.ok {
    background: var(--accent);
  }

  .dot.down {
    background: var(--danger);
  }

  .bad {
    color: var(--danger);
  }

  form {
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

  .l small {
    margin-left: 6px;
    color: var(--muted);
    font-weight: 400;
  }

  .hint {
    color: var(--muted);
    font-size: 0.8rem;
  }

  input[type='url'],
  input[type='text'],
  input[type='password'],
  select,
  textarea {
    width: 100%;
    min-height: 44px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  textarea {
    padding: 10px 12px;
    resize: vertical;
    font-family: ui-monospace, 'Cascadia Mono', Menlo, monospace;
    font-size: 0.88rem;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  button {
    min-height: 44px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    cursor: pointer;
  }

  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .banner {
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
