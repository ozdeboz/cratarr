<script lang="ts">
  // An album not in the library being opened (looked for on Bandcamp; the owner, 2026-10-25): the busy ring on its cover,
  // in the middle, as Play's and the status' rounds are (the accent, the ring in its text colour), not the faint ring a
  // busy button has. What holds it is positioned. `small`: a row's cover.
  let { small = false }: { small?: boolean } = $props();
</script>

<span class="cover-busy" class:small role="status" aria-label="Opening"><span class="ring"></span></span>

<style>
  .cover-busy {
    position: absolute;
    top: 50%;
    left: 50%;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.35);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }

  .cover-busy.small {
    width: 22px;
    height: 22px;
  }

  /* The app's busy ring (app.css busy-spin), drawn on the accent. */
  .ring {
    width: 16px;
    height: 16px;
    border: 2px solid color-mix(in srgb, var(--accent-text) 30%, transparent);
    border-top-color: var(--accent-text);
    border-radius: 50%;
    animation: busy-spin 0.7s linear infinite;
    box-sizing: border-box;
  }

  .small .ring {
    width: 12px;
    height: 12px;
  }

  @media (prefers-reduced-motion: reduce) {
    .ring {
      animation-duration: 2s;
    }
  }

  :global(:root[data-motion='off']) .ring {
    animation-duration: 2s;
  }
</style>
