<script lang="ts">
  // Jump chips over the graph (inside PatchGraph's SvelteFlow): "All" fits the whole patch; each of the picture lane's
  // parts (a layer, the loop, a device opened in place, a module of the picture's own) zooms to it. Svelte Flow's own
  // fitView does the moving.
  import { useSvelteFlow, type FitViewOptions } from '@xyflow/svelte';

  let { parts, padding, onjump }: { parts: { id: string; label: string }[]; padding: FitViewOptions['padding']; onjump?: () => void } = $props();
  const flow = useSvelteFlow();
  // A fade at an edge with more chips past it (the row scrolls sideways).
  let row = $state<HTMLDivElement | null>(null);
  let more = $state({ left: false, right: false });
  function edges() {
    if (!row) return;
    more = { left: row.scrollLeft > 2, right: row.scrollLeft + row.clientWidth < row.scrollWidth - 2 };
  }
  $effect(() => {
    void parts.length;
    edges();
    const r = row;
    if (!r || typeof ResizeObserver !== 'function') return;
    const o = new ResizeObserver(edges);
    o.observe(r);
    return () => o.disconnect();
  });

  function jump(id: string | null) {
    onjump?.();
    void flow.fitView({ padding, duration: 250, ...(id ? { nodes: [{ id }], maxZoom: 1.4 } : {}) });
  }
</script>

<div class="chips nodrag nopan" class:fl={more.left} class:fr={more.right} role="toolbar" aria-label="Go to" bind:this={row} onscroll={edges}>
  <button onclick={() => jump(null)}>All</button>
  {#each parts as p (p.id)}<button onclick={() => jump(p.id)}>{p.label}</button>{/each}
</div>

<style>
  .chips {
    display: flex;
    gap: 6px;
    max-width: 100%;
    overflow-x: auto;
    scrollbar-width: none;
    pointer-events: auto;
  }
  .chips.fr {
    mask-image: linear-gradient(to right, #000 calc(100% - 32px), transparent);
  }
  .chips.fl {
    mask-image: linear-gradient(to left, #000 calc(100% - 32px), transparent);
  }
  .chips.fl.fr {
    mask-image: linear-gradient(to right, transparent, #000 32px, #000 calc(100% - 32px), transparent);
  }
  button {
    flex: none;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 16px;
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    white-space: nowrap;
    cursor: pointer;
  }
  button:hover {
    border-color: color-mix(in srgb, var(--text) 30%, var(--border));
  }
</style>
