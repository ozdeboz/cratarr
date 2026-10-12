# 0006: Players are their own concept, chosen per browser

- Status: accepted
- Date: 2026-10-12

## Context

Decision 0005 put the player to follow in each source's settings (the Jellyfin plugin's "Follow player" field). The
owner asked for a coherent player architecture that is not tangled with sources: the choice was server-wide, so
the tablet and the phone could not each follow their own player, and a second picker (sources) sat beside it.

Jellyfin facts this rests on (10.11, observed 2026-10-12):
- Only connected sessions say whether they can play audio (`PlayableMediaTypes`). The device list's capabilities
  are empty for every device, and the activity log reaches back about a day and names only the device, inside an
  English sentence. So "players that played music lately" cannot be known; "players connected now" can.
- Finamp registers two sessions per device: one reports what plays (many supported commands, no websocket), the
  other ("Play On") is the only one that acts on a play command. Both say `SupportsRemoteControl=false` and Jellyfin
  accepts the command for both. Playing on a player therefore goes to every session of that device.

## Decision

- A player is a core concept: `{source, key, label, can_play}`. The key is stable across sessions: the source's id
  plus app and device (`jellyfin:Finamp::Lenovo Tab`). Sessions of the same app on the same device are one player.
- Sources only list players and act on one: `players()`, `now_playing(player)`, `play_album(album, player)`. A
  source stores no choice. The per-source "Follow player" field is removed; a saved value is ignored.
- The picker lists players connected now that can play audio, across all sources, plus "Automatic". It replaces
  the source picker: choosing a player implies its source.
- The choice belongs to the browser (kept with the other display preferences) and is sent with each request as
  `?player=`. There is no server-wide default.
- Automatic: follow whatever started playing most recently on any player; "play" goes to the player playing now.
- A chosen player that is not connected stays chosen and is shown as not connected; nothing is followed until it
  returns. No silent switching.

## Consequences

- Each device follows its own player; two people can use Cratarr at once.
- A player only appears while its app is open. Its key stays valid, so the choice survives the app closing.
- The source picker, its `?source=` parameter and `StateView.sources` go away; `?player=` replaces them.
