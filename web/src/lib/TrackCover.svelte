<script lang="ts">
  // A track's cover in front of its large row (lib/Row.svelte): 60 px, corners 8 (Finamp's track tile), the stand-in
  // without one (lib/Cover), and while it plays the one playing sign (lib/PlayingDots) over a dark veil, still while
  // paused (the owner, 2026-10-27: one playing sign on covers; Finamp's bars before).
  import Cover from './Cover.svelte';
  import PlayingDots from './PlayingDots.svelte';

  let { src, playing = false, paused = false }: { src: string | null; playing?: boolean; paused?: boolean } = $props();
</script>

<span class="cover">
  <Cover {src} />
  {#if playing}<span class="eq"><PlayingDots still={paused} /></span>{/if}
</span>

<style>
  .cover {
    position: relative;
    flex: none;
    width: 60px;
    height: 60px;
    border-radius: 8px;
  }

  .eq {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    border-radius: 8px;
    background: rgb(0 0 0 / 0.4);
    color: white;
  }
</style>
