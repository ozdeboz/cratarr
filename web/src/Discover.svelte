<script lang="ts">
  // Discover (decision 0069; the owner, 2026-10-24: "suggestions outside of" the library, "a discover page next to
  // playlists"): albums the library does not have, from the signed-in account's ListenBrainz and Last.fm, each with why
  // it is suggested, a cover, where to listen elsewhere, Want (Lidarr fetches it into Incoming, where it is heard and
  // kept or deleted) and Not interested (it never comes back). Looked up again once a day as a task, its progress in
  // one line (ui-conventions, Progress); Refresh in the page's pill asks again now. Without a service connected, the
  // page says what it needs.
  import { getDiscover, notDiscover, refreshDiscover, wantDiscover, type DiscoverAlbum, type DiscoverView } from './lib/api';
  import Icon from './lib/Icon.svelte';
  import Meter from './lib/Meter.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { usePageAction } from './lib/pageView.svelte';
  import { progressOf, tasks } from './lib/tasks.svelte';
  import { me } from './lib/me.svelte';
  import { toast } from './lib/toast.svelte';

  // Want: an administrator's, with Lidarr connected and on (the owner, 2026-10-24: others ask, from 0.9).

  const KIND = 'discover';
  let view = $state<DiscoverView | null>(null);
  let error = $state<string | null>(null);
  const load = () =>
    getDiscover().then(
      (v) => ((view = v), (error = null)),
      (e) => (error = e instanceof Error ? e.message : String(e)),
    );
  load();

  // The look under way (the server's task), followed while the page shows; when it ends, the list is read again.
  $effect(() => tasks.follow());
  const task = $derived(tasks.of(KIND));
  const looking = $derived(tasks.running(KIND));
  const canWant = $derived(me.admin && !!view?.lidarr);
  const wantWhy = $derived(!me.admin ? 'Ask an administrator' : 'Needs Lidarr');
  let seenEnd = $state<number | null>(null);
  $effect(() => {
    const t = task;
    if (t && t.finished !== null && t.id !== seenEnd) {
      seenEnd = t.id;
      if (t.error) toast.show({ text: t.error, tone: 'error' });
      load();
    }
  });
  const progress = $derived(looking ? progressOf(task) : null);

  async function refresh() {
    try {
      tasks.begun((await refreshDiscover()).task);
    } catch (e) {
      toast.fail(e, 'Could not look again');
    }
  }
  const connected = $derived(!!view && (view.services.listenbrainz || view.services.lastfm));
  usePageAction(() => (connected ? { icon: 'refresh', label: 'Look again', run: refresh, progress: looking ? (progress?.value ?? 0) : null } : null));

  // Answered here: gone from the list at once (the server keeps the answer).
  let gone = $state<string[]>([]);
  let asking = $state<string | null>(null);
  const albums = $derived((view?.found?.albums ?? []).filter((a) => !gone.includes(a.key)));
  async function want(a: DiscoverAlbum) {
    asking = a.key;
    try {
      const { said } = await wantDiscover(a.key);
      gone = [...gone, a.key];
      toast.show({ text: `${a.album}: ${said}` });
    } catch (e) {
      toast.fail(e, 'Lidarr could not fetch it');
    } finally {
      asking = null;
    }
  }
  async function no(a: DiscoverAlbum) {
    gone = [...gone, a.key];
    try {
      await notDiscover(a.key);
    } catch (e) {
      gone = gone.filter((k) => k !== a.key);
      toast.fail(e, 'Could not leave it out');
    }
  }
</script>

{#snippet how()}
  {#if progress}
    {#if progress.value !== null}<Meter label={progress.line} value={progress.value} />{:else}<p class="note">{progress.line}</p>{/if}
  {/if}
{/snippet}

<main>
  {#if error && !view}
    <Notice text={error} />
  {:else if !view}
    <PageLoading />
  {:else if !connected}
    <div class="empty">
      <p>Discover suggests albums you don't have yet, from what you listen to. It needs ListenBrainz or Last.fm.</p>
      <a class="button primary" href="#/settings?view=settings%3Aconnections">Connect a listening service</a>
    </div>
  {:else}
    {#if looking && !albums.length}
      <!-- The first look: the page's loader where it always is, how far it is under it (ui-conventions, Progress). -->
      <PageLoading>{@render how()}</PageLoading>
    {:else}
      {#if progress}<div class="looking">{@render how()}</div>{/if}
      {#each view.found?.notes ?? [] as note (note)}<p class="note">{note}</p>{/each}
    {/if}
    {#if !albums.length}
      {#if !looking}<p class="note">{view.found ? 'Nothing new to suggest right now.' : 'Looking for albums you might like…'}</p>{/if}
    {:else}
      {#if !me.admin}
        <p class="note">Want is an administrator's: ask one.</p>
      {:else if !view.lidarr}
        <p class="note">Want needs Lidarr: <a href="#/settings?view=settings%3Aconnections%2Flidarr">connect it</a>.</p>
      {/if}
      <ul class="grid">
        {#each albums as a (a.key)}
          <li class="card">
            <div class="cover">{#if a.cover}<img src={a.cover} alt="" loading="lazy" />{/if}</div>
            <div class="text">
              <b>{a.album}</b>
              <span class="by">{a.artist}{a.year ? ` · ${a.year}` : ''}{#if a.new_artist}<span class="new">New to you</span>{/if}</span>
              {#each a.why as w (w)}<small class="why">{w}</small>{/each}
              <span class="links">
                {#each a.links as l (l.name)}<a href={l.url} target="_blank" rel="noopener noreferrer">{l.name}</a>{/each}
              </span>
              <span class="acts">
                <!-- Unavailable, not hidden, without Lidarr or for someone not an administrator (the owner, 2026-10-24: they
                     will ask, in a queue the administrators answer, in 0.9); the note above says why. -->
                <button class="primary" disabled={!canWant || asking === a.key} aria-busy={asking === a.key} title={canWant ? null : wantWhy} onclick={() => want(a)}
                  >Want</button
                >
                <button class="no" aria-label="Not interested" title="Not interested" onclick={() => no(a)}><Icon name="thumbsDown" size={20} /></button>
              </span>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</main>

<style>
  /* The page answers to its own width (beside the player or not): a container, as Crates. */
  main {
    position: relative;
    padding: 14px clamp(12px, 2vw, 32px) 48px;
    container-type: inline-size;
  }

  .looking {
    margin: 0 0 12px;
  }

  .note {
    margin: 0 0 12px;
    color: var(--muted);
    font-size: 14px;
  }

  .empty {
    display: grid;
    justify-items: start;
    gap: 12px;
    max-width: 520px;
    margin: 24px 0;
  }

  .empty p {
    margin: 0;
  }

  /* One card across a phone, more as the page widens (as Crates). */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 340px), 1fr));
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .card {
    display: flex;
    gap: 14px;
    padding: 12px;
    border-radius: 14px;
    background: var(--surface);
  }

  .cover {
    flex: none;
    width: 112px;
    height: 112px;
    border-radius: 8px;
    overflow: hidden;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .cover img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  b {
    overflow-wrap: anywhere;
    font-size: 15px;
  }

  .by {
    color: var(--muted);
    font-size: 14px;
  }

  .new {
    margin-left: 8px;
    padding: 1px 7px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent-ink);
    font-size: 12px;
  }

  .why {
    color: var(--muted);
    font-size: 13px;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 13px;
  }

  .links a {
    color: var(--accent-ink);
  }

  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: auto;
    padding-top: 6px;
  }

  button,
  .button {
    min-height: 34px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .button {
    display: inline-flex;
    align-items: center;
    text-decoration: none;
  }

  button.primary,
  .button.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* Not interested: an icon beside Want, as round as the buttons beside it are tall. */
  .no {
    display: grid;
    place-items: center;
    padding: 0 10px;
  }
</style>
