<script lang="ts" module>
  /** One notice style everywhere (docs/ui-conventions.md): done, warning or error, optionally undoable. */
  export type NoticeData = { text: string; tone?: 'done' | 'warn' | 'error'; undo?: () => void };
</script>

<script lang="ts">
  let { notice = $bindable(null) }: { notice: NoticeData | null } = $props();

  // Done and warning notices go away by themselves; an error stays until the next action replaces it.
  $effect(() => {
    if (!notice || notice.tone === 'error') return;
    const t = setTimeout(() => (notice = null), 6000);
    return () => clearTimeout(t);
  });
</script>

{#if notice}
  <div class="notice {notice.tone ?? 'done'}" role={notice.tone === 'error' ? 'alert' : 'status'}>
    <span>{notice.text}</span>
    {#if notice.undo}
      <button
        class="undo"
        onclick={() => {
          const u = notice?.undo;
          notice = null;
          u?.();
        }}>Undo</button
      >
    {/if}
  </div>
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

  .notice.warn {
    border-color: var(--warn);
    color: var(--warn);
  }

  .notice.error {
    border-color: var(--danger);
    color: var(--danger);
  }

  .undo {
    margin-left: auto;
    padding: 0;
    background: none;
    border: 0;
    color: var(--accent);
    font: inherit;
    font-weight: 600;
    text-decoration: underline;
    cursor: pointer;
  }
</style>
