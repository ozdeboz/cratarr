<script lang="ts">
  // A choice's mark, at its row's end (the owner, 2026-11-02: one row, its end says what kind of choice it is): a round
  // mark for one of many, a rounded square for several of many (select mode's too); filled in the accent with a tick
  // once chosen. A switch is lib/Switch, a value that opens is the row's value and ›.
  let { kind = 'one', on }: { kind?: 'one' | 'several'; on: boolean } = $props();
</script>

<span class="mark" class:box={kind === 'several'} class:on aria-hidden="true"></span>

<style>
  .mark {
    display: grid;
    flex: none;
    place-items: center;
    box-sizing: border-box;
    width: 22px;
    height: 22px;
    border: 2px solid var(--muted);
    border-radius: 50%;
  }

  .box {
    border-radius: 6px;
  }

  .on {
    border-color: var(--art-accent, var(--accent));
    background: var(--art-accent, var(--accent));
  }

  .on::after {
    content: '';
    width: 9px;
    height: 5px;
    border: 2.5px solid var(--accent-text);
    border-top: 0;
    border-right: 0;
    transform: translateY(-1px) rotate(-45deg);
  }
</style>
