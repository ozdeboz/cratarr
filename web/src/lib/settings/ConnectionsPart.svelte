<script lang="ts">
  // Connections: the servers Cratarr works with, and Lidarr; saved with the Save bar.
  import { ed } from './editor.svelte';
  import MoreInfo from '../MoreInfo.svelte';
  import Switch from '../Switch.svelte';
  import { str } from './editor.svelte';

  // Shown only once the settings are read (lib/SettingsPanel).
  const s = $derived(ed.s!);

  const many = $derived(ed.drafts.length > 1);
</script>

  <section aria-labelledby="conn-h">
    <h2 id="conn-h">Connections</h2>
    <p class="lede">
      The servers Cratarr works with <MoreInfo label="Connections" text="Shared by everyone who uses Cratarr, not this device's own." />
      {#if s.origin === 'env'}Until you save here, these come from the container's settings.{/if}
    </p>

    {#each ed.drafts as d (d)}
      {@const k = ed.kindOf(d)}
      {@const line = ed.statusLine(d.view)}
      <article class="server" class:off={!d.enabled}>
        <header>
          <h3>{k?.label ?? d.kind}{#if many}<small>{d.id}</small>{/if}</h3>
          <Switch label={`Use ${k?.label ?? d.kind}`} hideLabel checked={d.enabled} onchange={(on) => (d.enabled = on)} />
          {#if ed.removing === d.id}
            <span class="ask">Remove this server?</span>
            <button class="danger" onclick={() => ((ed.drafts = ed.drafts.filter((x) => x !== d)), (ed.removing = null))}>Remove</button>
            <button onclick={() => (ed.removing = null)}>Cancel</button>
          {:else}
            <button onclick={() => (ed.removing = d.id)}>Remove</button>
          {/if}
        </header>

        {#if line}<p class="state" class:bad={!line.ok}><span class="dot" class:ok={line.ok} aria-hidden="true"></span>{line.text}</p>{/if}

        <div class="fields">
          {#each k?.fields ?? [] as f (f.key)}
            <label>
              <span class="l">{f.label}{#if !f.required}<small>optional</small>{/if}</span>
              {#if f.kind === 'secret'}
                {@const needed = ed.secretNeeded(d, f)}
                {@const isSaved = ed.saved(d)?.secrets_set.includes(f.key)}
                <input
                  type="password"
                  autocomplete="new-password"
                  bind:value={d.secrets[f.key]}
                  required={needed}
                  placeholder={isSaved && !needed ? 'Saved; leave empty to keep it' : ''}
                />
                {#if needed && isSaved}
                  <span class="hint warn">You changed the {ed.labelOf(d, f.bound_to)}, so enter the {f.label.toLowerCase()} again.</span>
                {/if}
              {:else if f.kind === 'choice' && ed.choicesFor(d, f).length}
                <select value={str(d.values[f.key])} onchange={(e) => (d.values[f.key] = e.currentTarget.value)} required={f.required}>
                  {#if !ed.choicesFor(d, f).some((c) => c.value === '')}<option value="" disabled={f.required}>Choose…</option>{/if}
                  {#each ed.choicesFor(d, f) as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
                </select>
              {:else if f.kind === 'multi_choice'}
                {@const chosen = Array.isArray(d.values[f.key]) ? (d.values[f.key] as string[]) : []}
                <span class="checks">
                  {#each ed.choicesFor(d, f) as c (c.value)}
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
          <button type="button" disabled={d.busy} aria-busy={d.busy} onclick={() => ed.check(d)}>Check connection</button>
        </div>
      </article>
    {:else}
      <p class="hint">No server yet.</p>
    {/each}

    {#if s.kinds.length === 1}
      <button class="add" onclick={() => ed.addServer(s!.kinds[0].id)}>Add {s.kinds[0].label} server</button>
    {:else if s.kinds.length > 1}
      <div class="row">
        <select bind:value={ed.addKind} aria-label="Kind of server">
          {#each s.kinds as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
        </select>
        <button class="add" onclick={() => ed.addServer(ed.addKind)}>Add server</button>
      </div>
    {/if}

    {#if ed.showLidarr}
      <article class="server lidarr">
        <header>
          <h3>
            Lidarr<MoreInfo label="Lidarr" text="With Lidarr's address and API key, Delete in Incoming also stops Lidarr fetching that album again." /><small
              >optional</small
            >
          </h3>
          {#if s.lidarr && ed.lid.url}
            {#if ed.lid.removing}
              <span class="ask">Remove Lidarr?</span>
              <button class="danger" onclick={() => ((ed.lid.url = ''), (ed.lid.key = ''), (ed.lid.view = null), (ed.lid.removing = false))}>Remove</button>
              <button onclick={() => (ed.lid.removing = false)}>Cancel</button>
            {:else}
              <button onclick={() => (ed.lid.removing = true)}>Remove</button>
            {/if}
          {/if}
        </header>
        {#if ed.lid.view && ed.lid.url}
          <p class="state" class:bad={!ed.lid.view.version}>
            <span class="dot" class:ok={!!ed.lid.view.version} aria-hidden="true"></span>{ed.lid.view.version ? `Connected to Lidarr ${ed.lid.view.version}` : (ed.lid.view.error ?? 'Not connected')}
          </p>
        {/if}
        <div class="fields">
          <label>
            <span class="l">Server address</span>
            <input type="url" bind:value={ed.lid.url} spellcheck="false" autocomplete="off" placeholder="http://lidarr:8686" />
          </label>
          <label>
            <span class="l">API key</span>
            <input
              type="password"
              autocomplete="new-password"
              bind:value={ed.lid.key}
              placeholder={s.lidarr?.key_set && !ed.lidarrKeyNeeded ? 'Saved; leave empty to keep it' : ''}
            />
            {#if ed.lidarrKeyNeeded}
              <span class="hint warn">You changed the server address, so enter the API key again.</span>
            {:else}
              <span class="hint">In Lidarr under Settings, General</span>
            {/if}
          </label>
        </div>
        <div class="row">
          <button type="button" disabled={ed.lid.busy || !ed.lid.url.trim()} aria-busy={ed.lid.busy} onclick={ed.checkLidarr}>Check connection</button>
        </div>
      </article>
    {/if}
  </section>
