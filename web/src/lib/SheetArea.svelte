<script lang="ts" module>
  import { getContext } from 'svelte';

  const AREA = Symbol('sheet area');

  /** The part of the window a sheet belongs to: the element of the page or of the panel it was opened from; `null`
   *  for a sheet of neither (the player, say), which stays in the middle of the window. */
  export function sheetArea(): HTMLElement | null {
    return getContext<(() => HTMLElement | undefined) | undefined>(AREA)?.() ?? null;
  }
</script>

<script lang="ts">
  // A side of the window that sheets belong to (the page, the panel): every sheet opened from inside it covers this
  // side only, on a window wide enough to show both (lib/MenuSheet.svelte). Sheets are drawn at the end of the page
  // (lib/portal.ts), so the side is told through Svelte's context, from the components that open them.
  import { setContext, type Snippet } from 'svelte';

  let { area, children }: { area: () => HTMLElement | undefined; children: Snippet } = $props();
  setContext(AREA, () => area());
</script>

{@render children()}
