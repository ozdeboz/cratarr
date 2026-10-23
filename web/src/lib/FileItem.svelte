<script lang="ts">
  // A file of the library as a row (docs/ui-conventions.md → Rows), on the Health page and in the Repair tab: its name,
  // and under it where it is (its album folder, and what is wrong with it), or what it will point at instead ("→ the
  // other file"). Large; a track's own name takes the file's place where it is known. The folder is its tooltip.
  import type { Snippet } from 'svelte';
  import Row from './Row.svelte';

  let {
    path,
    name,
    to,
    detail,
    playlist,
    action,
  }: {
    /** Music-relative. */
    path: string;
    /** The track's name, shown in place of the file's. */
    name?: string;
    /** The file a repair points it at instead. */
    to?: string;
    /** What is wrong, or who it is by, after the folder. */
    detail?: string;
    /** The playlist it is in, first on the second line (when several are shown together). */
    playlist?: string;
    action?: Snippet;
  } = $props();

  const fileOf = (p: string) => p.slice(p.lastIndexOf('/') + 1);
  const albumOf = (p: string) => p.slice(0, Math.max(0, p.lastIndexOf('/')));
  const where = $derived([playlist, ...(to ? [] : [albumOf(path), detail])].filter(Boolean).join(' · '));
</script>

<Row size="large" title={name ?? fileOf(path)} tooltip={albumOf(path)} {action} wrap={!!action}>
  {#snippet sub()}{where}{#if to}{where ? ' · ' : ''}<span class="arrow">→</span> {fileOf(to)}{/if}{/snippet}
</Row>

<style>
  .arrow {
    color: var(--accent-ink);
  }
</style>
