<script lang="ts">
  // A track's cover in front of its large row (lib/Row.svelte): 60 px, corners 8 (Finamp's track tile), the stand-in
  // without one (lib/Cover), and Finamp's mini visualiser over a dark veil while it plays (still while paused).
  import Cover from './Cover.svelte';

  let { src, playing = false, paused = false }: { src: string | null; playing?: boolean; paused?: boolean } = $props();
</script>

<span class="cover">
  <Cover {src} />
  {#if playing}<span class="eq" class:still={paused} aria-hidden="true"><i></i><i></i><i></i></span>{/if}
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
  }

  .eq i {
    width: 4px;
    height: 15px;
    border-radius: 2px;
    background: white;
    transform-origin: bottom;
    animation: eq 900ms ease-in-out infinite alternate;
  }

  .eq i:nth-child(2) {
    animation-delay: -300ms;
  }

  .eq i:nth-child(3) {
    animation-delay: -600ms;
  }

  .eq.still i {
    animation: none;
    transform: scaleY(0.4);
  }

  @keyframes eq {
    from {
      transform: scaleY(0.25);
    }
    to {
      transform: scaleY(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .eq i {
      animation: none;
    }
  }

</style>
