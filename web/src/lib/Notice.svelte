<script lang="ts">
  // A page's own state in place, such as "the page could not load" (docs/ui-conventions.md → Feedback). What an
  // action did goes in a toast instead (lib/toast.svelte.ts). A failed read can be tried again from it (the owner,
  // 2026-11-02: errors are never shown as nothing).
  // `away`: what it says instead with Cratarr out of reach, one line and no Try again (lib/noticeAway: "Discover needs Cratarr.", '' for
  // a part of a page whose line is said elsewhere); none, it is the Notice there too.
  import { noticeShows } from './noticeAway';
  import { reach } from './reach.svelte';

  let { text, tone = 'error', onretry, away }: { text: string; tone?: 'warn' | 'error'; onretry?: () => void; away?: string } = $props();
  const shows = $derived(noticeShows(reach.away, away));
</script>

{#if shows === 'line'}
  <p class="away">{away}</p>
{:else if shows === 'notice'}
  <p class="notice {tone}" role={tone === 'error' ? 'alert' : 'status'}>
    <span>{text}</span>{#if onretry}<button class="retry" onclick={onretry}>Try again</button>{/if}
  </p>
{/if}

<style>
  .notice {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0 0 12px;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-size: 0.9rem;
  }

  /* Out of reach, what cannot be shown here, in one muted line (as the menus' "The rest needs Cratarr."). */
  .away {
    margin: 0 0 12px;
    color: var(--muted);
    font-size: 13px;
  }

  .notice span {
    flex: 1;
    min-width: 0;
  }

  .notice.warn {
    border-color: var(--warn);
    color: var(--warn-ink);
  }

  .notice.error {
    border-color: var(--danger);
    color: var(--danger-ink);
  }

  /* As DownloadsPanel's Retry. */
  .retry {
    flex: none;
    padding: 8px 14px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--danger) 16%, transparent);
    color: var(--danger-ink);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
</style>
