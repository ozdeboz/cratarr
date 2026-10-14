<script lang="ts">
  // Renders lib/toast.svelte.ts, once, in the app shell: floating above the mini-player (or the bottom edge), at the bottom of the
  // content beside the sidebar from 1024 px. It never moves the page. Done and warning toasts go after 6 s (10 s with
  // its Undo or Redo); errors stay until tapped away, replaced, or the page changes. Touching or hovering it holds the timer; a
  // swipe sideways or down dismisses it.
  import { toast } from './toast.svelte';

  const t = $derived(toast.current);
  let held = $state(false);

  $effect(() => {
    if (!t || t.tone === 'error' || held) return;
    const id = t.id;
    const timer = setTimeout(() => toast.current?.id === id && toast.dismiss(), t.act ? 10_000 : 6000);
    return () => clearTimeout(timer);
  });

  // The page changing takes it away; the panel opening or closing (the address's `?…`) does not: a deleted playlist's
  // panel closes, and its Undo must stay.
  $effect(() => {
    const page = () => location.hash.split('?')[0];
    let was = page();
    const away = () => {
      if (page() !== was) toast.dismiss();
      was = page();
    };
    addEventListener('hashchange', away);
    return () => removeEventListener('hashchange', away);
  });

  let from: { x: number; y: number } | null = null;
  function down(e: PointerEvent) {
    held = true;
    from = { x: e.clientX, y: e.clientY };
  }
  function up(e: PointerEvent) {
    if (e.pointerType !== 'mouse') held = false;
    if (!from) return;
    const [dx, dy] = [e.clientX - from.x, e.clientY - from.y];
    from = null;
    if (Math.abs(dx) > 60 || dy > 40) toast.dismiss();
  }
</script>

<div class="region" aria-live="polite">
  {#if t}
    {#key t.id}
      <div
        class="toast {t.tone ?? 'done'}"
        role={t.tone === 'error' ? 'alert' : 'status'}
        onpointerenter={() => (held = true)}
        onpointerleave={() => {
          held = false;
          from = null;
        }}
        onpointerdown={down}
        onpointerup={up}
        onpointercancel={() => {
          held = false;
          from = null;
        }}
      >
        <span class="text">{t.text}</span>
        {#if t.act}
          <button
            class="act"
            onclick={() => {
              const run = t.act?.run;
              toast.dismiss();
              run?.();
            }}>{t.act.label}</button
          >
        {/if}
        <button class="close" aria-label="Dismiss" onclick={() => toast.dismiss()}>×</button>
      </div>
    {/key}
  {/if}
</div>

<style>
  .region {
    position: fixed;
    inset: auto 0 calc(var(--bottom-h, 0px) + env(safe-area-inset-bottom) + 8px);
    z-index: 25;
    display: flex;
    justify-content: center;
    padding: 0 12px;
    pointer-events: none;
  }

  @media (min-width: 1024px) {
    .region {
      inset: auto 0 calc(var(--bottom-h, 0px) + 24px) 240px;
    }
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 4px;
    width: min(100%, 560px);
    min-height: 48px;
    padding: 6px 6px 6px 16px;
    box-sizing: border-box;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
    font-size: 0.9rem;
    pointer-events: auto;
    touch-action: none;
    animation: rise 160ms ease-out;
  }

  .toast.warn {
    border-color: var(--warn);
    color: var(--warn);
  }

  .toast.error {
    border-color: var(--danger);
    color: var(--danger);
  }

  .text {
    flex: 1;
    min-width: 0;
    padding: 6px 0;
  }

  .act,
  .close {
    flex: none;
    min-width: 40px;
    min-height: 36px;
    padding: 0 10px;
    background: none;
    border: 0;
    border-radius: 8px;
    font: inherit;
    cursor: pointer;
  }

  .act {
    color: var(--accent);
    font-weight: 600;
  }

  .close {
    color: var(--muted);
    font-size: 1.3rem;
    line-height: 1;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
</style>
