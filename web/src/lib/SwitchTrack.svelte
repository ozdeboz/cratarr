<script lang="ts">
  // The switch's track alone (Material 3's, as lib/Switch draws it), for a control that is something else around it: a
  // menu entry that turns a thing on or off (lib/MenuEntry's `switched`). The control says it is a switch.
  let { on }: { on: boolean } = $props();
</script>

<i class:on aria-hidden="true"></i>

<style>
  /* A 52 × 32 track, its thumb growing and moving over when on, the track then filled with the accent. */
  i {
    position: relative;
    flex: none;
    box-sizing: border-box;
    width: 52px;
    height: 32px;
    margin-left: auto;
    border: 2px solid var(--muted);
    border-radius: 16px;
    transition:
      background-color 150ms,
      border-color 150ms;
  }

  i::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 6px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--muted);
    transform: translateY(-50%);
    transition:
      left 150ms,
      width 150ms,
      height 150ms,
      background-color 150ms;
  }

  .on {
    border-color: var(--art-accent, var(--accent));
    background: var(--art-accent, var(--accent));
  }

  .on::after {
    left: 22px;
    width: 24px;
    height: 24px;
    background: var(--accent-text);
  }
</style>
