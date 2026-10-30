<script lang="ts">
  // A page's own state in place, such as "the page could not load" (docs/ui-conventions.md → Feedback). What an
  // action did goes in a toast instead (lib/toast.svelte.ts).
  let { text, tone = 'error' }: { text: string; tone?: 'warn' | 'error' } = $props();
</script>

<p class="notice {tone}" role={tone === 'error' ? 'alert' : 'status'}>{text}</p>

<style>
  .notice {
    margin: 0 0 12px;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-size: 0.9rem;
  }

  .notice.warn {
    border-color: var(--warn);
    color: var(--warn-ink);
  }

  .notice.error {
    border-color: var(--danger);
    color: var(--danger-ink);
  }
</style>
