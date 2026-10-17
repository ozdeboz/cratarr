<script lang="ts">
  // Diagnostics (decision 0045): what happened, as Cratarr keeps it, laid out as Finamp lays out its logs (the owner,
  // 2026-10-17: "we learn from the way Finamp presents settings and deeper settings"): one switch on top for more
  // detail, a day long, with the areas one by one under it only while it is on; each player's link as Cratarr knows it
  // and as the player last told; then the log, newest first, by player, area and level. The file for a bug report is
  // the bar's Download. The log as it is, no prose.
  import { getLog, getLogLevels, getPlayers, setLogLevels, type LogLevels, type LogRow, type PlayOnPlayer } from '../api';
  import Switch from '../Switch.svelte';

  const AREA_NAMES: Record<string, string> = {
    playon: 'Play On',
    syncplay: 'Sync play',
    signin: 'Sign-in',
    library: 'Library',
    subsonic: 'Subsonic',
    imports: 'Imports',
    server: 'Server',
  };
  const areaName = (a: string) => AREA_NAMES[a] ?? a;
  const PAGE = 200;

  let players = $state<PlayOnPlayer[]>([]);
  let rows = $state<LogRow[]>([]);
  let levels = $state<LogLevels | null>(null);
  let place = $state('');
  let area = $state('');
  let level = $state('');
  let more = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** More detail lasts this long, then goes back by itself. */
  const HOURS = 24;

  const failed = (e: unknown) => (error = e instanceof Error ? e.message : String(e));

  // Only the latest load's answer is shown (an older one may come after it).
  let asked = 0;
  async function load(next = false) {
    const mine = ++asked;
    busy = true;
    error = null;
    try {
      const before = next ? rows.at(-1)?.id : undefined;
      const r = await getLog({ place, area, level, before, limit: PAGE });
      if (mine !== asked) return;
      rows = next ? [...rows, ...r.rows] : r.rows;
      more = r.rows.length === PAGE;
    } catch (e) {
      if (mine === asked) failed(e);
    } finally {
      if (mine === asked) busy = false;
    }
  }

  // Again whenever what is shown changes.
  $effect(() => {
    void [place, area, level];
    load();
  });
  getPlayers().then(
    (r) => (players = r.players),
    () => (players = []),
  );
  getLogLevels().then(
    (l) => (levels = l),
    () => (levels = null),
  );

  async function setLevels(now: Record<string, string>) {
    error = null;
    try {
      levels = await setLogLevels(now, HOURS);
    } catch (e) {
      failed(e);
    }
  }

  /** One area more (or less) than the rest. */
  function raise(a: string, to: string) {
    if (!levels) return;
    const now = { ...levels.now };
    if (to === 'info') delete now[a];
    else now[a] = to;
    setLevels(now);
  }

  const detailed = $derived(!!levels && Object.keys(levels.now).length > 0);
  /** More detail: every area, or back to normal. */
  const detail = (on: boolean) => levels && setLevels(on ? Object.fromEntries(levels.areas.map((a) => [a, 'debug'])) : {});

  const today = new Date().toDateString();
  /** The time, with the day when it is not today. */
  function time(ms: number, seconds = true) {
    const d = new Date(ms);
    const t = d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', ...(seconds ? { second: '2-digit' } : {}) });
    return d.toDateString() === today ? t : `${d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })} ${t}`;
  }

  /** A row's details as `key value`, times shown as times. */
  function details(f: Record<string, unknown>) {
    return Object.entries(f).map(([k, v]) => {
      const n = typeof v === 'number' && (k === 'until' || k.endsWith('_at') || k === 'connected_since') && v > 1e12 ? time(v) : v;
      return `${k} ${typeof n === 'object' ? JSON.stringify(n) : n}`;
    });
  }

  const places = $derived(['server', ...players.map((p) => p.name)]);
</script>

{#if levels}
  <section aria-labelledby="detail-h">
    <h2 id="detail-h">Diagnostics</h2>
    <div class="pref">
      <Switch label="More detail" hint={levels.until ? `Until ${time(levels.until, false)}` : null} hintWhenOn checked={detailed} onchange={detail} />
    </div>
    {#if detailed}
      <ul class="list">
        {#each levels.areas as a (a)}
          <li>
            <span class="what">{areaName(a)}</span>
            <select value={levels.now[a] ?? 'info'} aria-label={`${areaName(a)}: how much`} onchange={(e) => raise(a, e.currentTarget.value)}>
              <option value="info">Normal</option>
              <option value="debug">More</option>
              <option value="trace">Everything</option>
            </select>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<section aria-labelledby="players-h">
  <h2 id="players-h">Players</h2>
  {#if players.length}
    <ul class="list">
      {#each players as p (p.name)}
        {@const l = p.link}
        <li>
          <span class="what">
            {p.name}
            {#if l?.own && (l.own.failed || l.own.wait_until)}
              <small>
                {#if l.own.failed}{l.own.failed} failed{#if l.own.last_error}: {l.own.last_error}{/if}{/if}
                {#if l.own.wait_until} · waiting until {time(l.own.wait_until, false)}{#if l.own.wait_why} ({l.own.wait_why}){/if}{/if}
              </small>
            {/if}
          </span>
          <span class="when">
            {#if p.online}Online{:else if l?.offline_since}Offline since {time(l.offline_since * 1000, false)}{:else}Offline{/if}{#if !p.online && l?.ended}<small>{l.ended}</small>{/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<section aria-labelledby="log-h">
  <h2 id="log-h">Log</h2>
  <div class="filters">
    <select bind:value={place} aria-label="Where">
      <option value="">Everywhere</option>
      {#each places as p (p)}<option value={p}>{p === 'server' ? 'Server' : p}</option>{/each}
    </select>
    <select bind:value={area} aria-label="Area">
      <option value="">All areas</option>
      {#each levels?.areas ?? Object.keys(AREA_NAMES) as a (a)}<option value={a}>{areaName(a)}</option>{/each}
    </select>
    <select bind:value={level} aria-label="Level">
      <option value="">All</option>
      <option value="warn">Warnings</option>
      <option value="error">Errors</option>
    </select>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <ol class="log" aria-busy={busy}>
    {#each rows as r (r.id)}
      <li class={r.level}>
        <span class="head">
          <time>{time(r.at)}</time>
          <span class="place">{r.place}</span>
          <span class="area">{areaName(r.area)}</span>
          {#if r.origin === 'device'}<span class="area">its log</span>{/if}
        </span>
        <span class="msg">{r.what}{#if r.count > 1}<span class="count">×{r.count}, last {time(r.last)}</span>{/if}</span>
        {#if Object.keys(r.fields).length}<span class="fields">{details(r.fields).join(' · ')}</span>{/if}
      </li>
    {/each}
  </ol>
  {#if more}<button disabled={busy} onclick={() => load(true)}>More</button>{/if}
</section>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }

  .what {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  /* Its own width, not the panel's whole row (SettingsPanel's selects); one element more to outweigh that rule. */
  .list li select {
    flex: none;
    width: auto;
  }

  .what small,
  .when small {
    display: block;
    color: var(--muted);
  }

  .when {
    color: var(--muted);
    text-align: right;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }

  .filters select {
    flex: 1 1 120px;
    min-width: 0;
  }

  .log {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .log li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 0 6px 10px;
    border-top: 1px solid var(--border);
    border-left: 3px solid transparent;
  }

  .log li.warn {
    border-left-color: var(--warn, #c90);
  }

  .log li.error {
    border-left-color: var(--danger, #c33);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    color: var(--muted);
    font-size: 13px;
  }

  .place {
    color: var(--text);
  }

  .count {
    margin-left: 8px;
    color: var(--muted);
    font-size: 13px;
  }

  .fields {
    color: var(--muted);
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .error {
    color: var(--danger, #c33);
  }
</style>
