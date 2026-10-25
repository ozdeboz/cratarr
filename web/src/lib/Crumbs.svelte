<script lang="ts">
  // A panel's title as a breadcrumb (0.9.3; the owner, 2026-10-25: Requests, Downloads, Insights, Health and Settings
  // get "pure breadcrumbs", "2 at most. So the current and one level above."): the level above, a tap back there, then
  // where one is. The page's own name above does what the panel's ✕ does; without a way back (`go`), it is only said.
  // No level above (`up` left out): only where one is, as beside the page whose name its list already shows (the
  // owner, 2026-10-25: Settings › Display beside Settings' list said Settings twice).
  let { up = null, here }: { up?: { label: string; go?: () => void } | null; here: string } = $props();
</script>

<h2 class="crumbs">
  {#if up}{#if up.go}<button class="up" onclick={up.go}>{up.label}</button>{:else}<span class="up">{up.label}</span>{/if}<span class="sep" aria-hidden="true">›</span>{/if}<span class="here" aria-current="page">{here}</span>
</h2>

<style>
  /* As the panels' titles were: one line, the level above muted; each part shortened with … rather than the line wrap. */
  .crumbs {
    flex: 1;
    display: flex;
    align-items: baseline;
    min-width: 0;
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    white-space: nowrap;
  }

  .up,
  .here {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .up {
    flex: 0 1 auto;
    max-width: 45%;
    color: var(--muted);
  }

  button.up {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    cursor: pointer;
  }

  button.up:hover {
    color: var(--text);
  }

  .sep {
    flex: none;
    margin: 0 8px;
    color: var(--muted);
  }

  .here {
    flex: 0 1 auto;
  }
</style>
