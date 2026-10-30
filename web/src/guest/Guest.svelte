<script lang="ts">
  // The guest page (0.10; the owner, 2026-10-30): on the home network, without an account, a guest gives a name,
  // sees what plays, searches the library and asks for a track, next or later; the host accepts or declines it in their
  // queue (lib/QueueSheet). Their own asked show here, waiting, then what became of them. Off, or away from home, the
  // server answers nothing of it: the page says it is not open.
  import { onMount } from 'svelte';
  import Icon from '../lib/Icon.svelte';
  import { ask, GuestError, hello, mine, now, search, type GuestAsked, type GuestNow, type GuestTrack } from './api';

  const NAME = 'cratarr.guestName';
  const kept = () => {
    try {
      return localStorage.getItem(NAME) ?? '';
    } catch {
      return '';
    }
  };
  let name = $state(kept());
  let named = $state(false);
  let closed = $state(false);
  let error = $state<string | null>(null);
  let playing = $state<GuestNow | null>(null);
  let query = $state('');
  let found = $state<GuestTrack[]>([]);
  let asked = $state<GuestAsked[]>([]);
  let busy = $state<string | null>(null);

  const failed = (e: unknown) => {
    if (e instanceof GuestError && e.status === 404) closed = true;
    else error = e instanceof Error ? e.message : 'Something went wrong.';
  };

  async function begin() {
    const n = name.trim();
    if (!n) return;
    error = null;
    try {
      await hello(n);
      try {
        localStorage.setItem(NAME, n);
      } catch {
        /* asked again next time */
      }
      named = true;
      void refresh();
    } catch (e) {
      failed(e);
    }
  }

  async function refresh() {
    try {
      [playing, asked] = await Promise.all([now(), mine()]);
    } catch (e) {
      failed(e);
    }
  }

  // Found as the words settle.
  $effect(() => {
    const q = query.trim();
    if (!named || q.length < 2) return void (found = []);
    const t = setTimeout(() => search(q).then((r) => (found = r.tracks), failed), 300);
    return () => clearTimeout(t);
  });

  async function askFor(t: GuestTrack, how: 'next' | 'last') {
    busy = t.id;
    error = null;
    try {
      const { cover: _cover, ...track } = t;
      await ask(track, how);
      query = '';
      void refresh();
    } catch (e) {
      failed(e);
    } finally {
      busy = null;
    }
  }

  onMount(() => {
    // A name given before: straight in.
    if (name.trim()) void begin();
    const t = setInterval(() => {
      if (named && !closed && document.visibilityState === 'visible') void refresh();
    }, 10_000);
    return () => clearInterval(t);
  });

  const stateText = (a: GuestAsked) => (a.state === 'accepted' ? 'In the queue' : a.state === 'declined' ? 'Not this time' : 'Waiting');
</script>

<main>
  <h1>Cratarr</h1>
  {#if closed}
    <p class="line">This page is not open.</p>
  {:else if !named}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void begin();
      }}
    >
      <label>Your name<input bind:value={name} autocomplete="nickname" maxlength="40" /></label>
      <button class="go" disabled={!name.trim()}>Ask for a track</button>
    </form>
  {:else}
    {#if playing?.track}
      {@const t = playing.track}
      <section class="now" aria-label="Playing now">
        {#if t.cover}<img src={t.cover} alt="" />{/if}
        <div><small>{playing.playing === false ? 'Paused' : 'Playing'}</small><strong>{t.name}</strong><span>{t.artist ?? ''}</span></div>
      </section>
    {/if}

    <label class="find"><Icon name="search" size={20} /><input type="search" placeholder="Title, artist or album" aria-label="Search the music" bind:value={query} /></label>

    {#if found.length}
      <ol class="rows" aria-label="Found">
        {#each found as t (t.id)}
          <li>
            {#if t.cover}<img src={t.cover} alt="" />{:else}<span class="nocover"></span>{/if}
            <div><strong>{t.name}</strong><span>{[t.artist, t.album].filter(Boolean).join(' · ')}</span></div>
            <button aria-label="Ask to play {t.name} next" title="Next" disabled={busy !== null} onclick={() => askFor(t, 'next')}><Icon name="playNext" size={22} /></button>
            <button aria-label="Ask to add {t.name} to the queue" title="Later" disabled={busy !== null} onclick={() => askFor(t, 'last')}><Icon name="listAdd" size={22} /></button>
          </li>
        {/each}
      </ol>
    {/if}

    {#if asked.length}
      <h2>Asked for</h2>
      <ol class="rows" aria-label="Asked for">
        {#each asked as a (a.id)}
          <li class:done={a.state && a.state !== 'pending'}>
            {#if a.track.cover}<img src={a.track.cover} alt="" />{:else}<span class="nocover"></span>{/if}
            <div><strong>{a.track.name}</strong><span>{a.track.artist ?? ''}</span><small>{stateText(a)}</small></div>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
  {#if error}<p class="line bad">{error}</p>{/if}
</main>

<style>
  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font-family: system-ui, sans-serif;
  }

  main {
    max-width: 560px;
    margin: 0 auto;
    padding: 24px 16px 48px;
  }

  h1 {
    margin: 0 0 20px;
    font-size: 22px;
  }

  h2 {
    margin: 28px 0 8px;
    color: var(--muted);
    font-size: 14px;
    font-weight: 600;
  }

  form {
    display: grid;
    gap: 16px;
  }

  label {
    display: grid;
    gap: 6px;
    color: var(--muted);
    font-size: 14px;
  }

  input {
    min-height: 48px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 16px;
  }

  .go {
    min-height: 48px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
  }

  .go:disabled {
    opacity: 0.5;
  }

  .now {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 20px;
    padding: 12px;
    border-radius: 16px;
    background: var(--surface);
  }

  .now img {
    width: 72px;
    height: 72px;
    border-radius: 8px;
    object-fit: cover;
  }

  .now div,
  .rows div {
    display: grid;
    min-width: 0;
  }

  .now small,
  .rows small {
    color: var(--muted);
    font-size: 12px;
  }

  .now span,
  .rows span {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .find {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
  }

  .find input {
    flex: 1;
    border: 0;
    background: none;
  }

  .rows {
    display: grid;
    gap: 2px;
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 60px;
  }

  .rows li div {
    flex: 1;
  }

  .rows li.done {
    opacity: 0.6;
  }

  .rows img,
  .nocover {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 6px;
    object-fit: cover;
    background: var(--surface);
  }

  .rows button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--accent);
  }

  .rows button:disabled {
    opacity: 0.4;
  }

  .line {
    color: var(--muted);
  }

  .bad {
    color: var(--danger-ink, var(--danger));
  }
</style>
