<script lang="ts">
  // Progress on a row (an album converting, a playlist's copies being made): what it is at, and a thin bar. One look
  // for every kind of work, wherever it shows.
  let { label, value }: { label: string; /** 0 to 1. */ value: number } = $props();
</script>

<span class="meter">
  {label}
  <span class="track" aria-hidden="true"><span style:width="{Math.min(1, Math.max(0, value)) * 100}%"></span></span>
</span>

<style>
  .meter {
    display: block;
    margin-top: 3px;
    color: var(--accent-ink);
    font-size: 0.8rem;
  }

  .track {
    display: block;
    max-width: 240px;
    height: 3px;
    margin-top: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    overflow: hidden;
  }

  .track span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.4s;
  }
</style>
