<script lang="ts">
  // An album's About (as Symfonium's, decided 2026-10-16), at the end of its panel: its genres (a chip each, opening the
  // genre), year, when it was last played and added (date and time, as Symfonium writes them), and where it comes
  // from: the server and its library. What is not known is left out; Last played says Never when it never was.
  import { viewGenre } from './crates.svelte';
  import Icon from './Icon.svelte';

  let {
    genres,
    year,
    added,
    played,
    plays,
    provider,
  }: {
    genres: string[];
    year: number | null;
    added?: number;
    played?: number;
    plays?: number;
    /** The server's name, and the library's (with its dot) when known. */
    provider: { server: string; library: string | null; colour: string | null };
  } = $props();

  const two = (n: number) => String(n).padStart(2, '0');
  /** 2026-10-16 14:05:09, this device's time. */
  const when = (secs: number) => {
    const d = new Date(secs * 1000);
    return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
  };
</script>

<section aria-labelledby="album-about">
  <h2 id="album-about">About</h2>
  {#if genres.length}
    <h3>Genres</h3>
    <p class="chips">
      {#each genres as g (g)}<button class="chip" onclick={() => viewGenre(g)}>{g}</button>{/each}
    </p>
  {/if}
  {#if year}
    <h3>Year</h3>
    <p class="chips"><span class="chip">{year}</span></p>
  {/if}
  {#if played || plays === 0}
    <h3>Last played</h3>
    <p>{played ? when(played) : 'Never'}</p>
  {/if}
  {#if added}
    <h3>Date added</h3>
    <p>{when(added)}</p>
  {/if}
  <h3>Media provider</h3>
  <p class="chips">
    <span class="chip">
      <Icon name="server" size={16} />
      {provider.server}{#if provider.library}<span class="sep">·</span>{#if provider.colour}<i class="dot" style:background={provider.colour}></i>{/if}{provider.library}{/if}
    </span>
  </p>
</section>

<style>
  section {
    margin-top: 24px;
    padding: 0 16px;
  }

  h2 {
    margin: 8px 0 4px;
    font-size: 18px;
    font-weight: 600;
  }

  h3 {
    margin: 16px 0 6px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
  }

  p {
    margin: 0;
    font-size: 15px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /* Material's outlined chip, as Symfonium's. */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    min-height: 32px;
    padding: 0 12px;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 14px;
  }

  button.chip {
    cursor: pointer;
  }

  .sep {
    color: var(--muted);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
</style>
