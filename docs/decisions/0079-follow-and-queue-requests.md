# 0079: Listening along: following another listener, asking for a track, and a guest page

- Status: accepted (the owner, 2026-10-30: "Decide now, build in 0.9.19")
- Date: 2026-10-30
- Builds on: 0031 (Play On), 0033 (Cratarr holds the queues), 0051 (several users), 0064 (one listening history),
  0068 (one way in for listening), 0.9.8 (shared devices, used from home only)
- Not: 0037 (sync play: one room's players playing one sound, each steering it), 0070 (album requests, which own
  `/api/requests`)

## Context

Several people use one Cratarr. One wants to hear what another plays, on their own phone, wherever they are; at a
party, people want to put a song on the host's queue without taking over the host's player; a visitor without an
account wants the same from the living room. Sync play (0037) does not fit: it is tight, same-room playback where every
member steers the queue. Album requests (0070) ask the library for an album, not a player for a track.

## Decision

The owner's decisions (2026-10-30):

1. **Follow across users.** Each user opts in with **Others may follow me** (off by default). A follower listens along
   on their own device, in step, from anywhere. The home-network fence still applies only to shared devices: another
   user's shared player is followed from home only, as it is used.
2. **Queue requests.** A follower asks for a track, play next or add to queue. Each request shows to the leader with
   Accept and Decline. The leader can mark people whose requests go straight in (auto-accept). Only the leader edits the
   queue.
3. **History.** Plays heard while following go into the follower's listening history and scrobbles, but are left out of
   their taste profile, Discover and the radio's and Autoplay's picks.
4. **Guest page.** A page `/guest`, on the home network only, with no account. A guest gives a name, sees what plays,
   searches the library and asks for a track. Requests go to the host chosen in Settings (a user or a shared device),
   who accepts them like any other. Rate-limited, off by default.
5. **Told.** A leader is told of each new request on their app, requests close together as one notice; an asker is told
   when theirs is accepted or declined. A declined request leaves the leader's list at once.

### How it is built (design choices)

- **One's own settings** (`/api/me/settings`) carry `followable` (absent: off) and `auto_accept` (user names). Turning
  `followable` off ends every following of that user at once.
- **The relation is Play On's.** A player follows `{user, player}`; it is kept with the queues (`queues.json`, as
  `shared` is) with the times it followed. The player view carries `following` on the follower, `followers` (user
  names) on the leader, and `queue_requests` (how many wait) on the asking account's own players.
- **Who may be followed:** another account's connected player, its user followable, following no one itself; a shared
  one from home only. No chains: a player others follow cannot follow, and a follower cannot be followed. A player in a
  sync group, or playing the device's own music (0052), cannot follow.
- **Keeping in line.** Each change of the leader (its track, play or pause, a seek of more than two seconds, its speed)
  sends each follower a `play` of the leader's one track at the leader's place now (its last report counted on at its
  speed), paused as the leader is. A follower more than two seconds off is sent a `seek`; its reports in the three
  seconds after a play or a seek are not judged. The follower's queue is one queue, "Following Ann", whoever it
  follows: its own queue is kept as it stood the first time (queue history, 0033 amended), to go back to, and its
  Repeat, Radio and Autoplay are off while it follows (they come back with that queue).
- **The follower's controls.** Next, Previous, Wind, the direction knob and every queue edit (add, move, remove,
  upcoming, shuffle, clear, restore, jump, switching queues, handover, joining a group) answer 409 "Following Ann: ask
  for a track instead." Pause stays its own (also when paused on the device); Resume catches up with the leader. **A
  play of something else, or a stop, ends the following** and then does what it says: choosing other music is leaving.
  A seek passes and is pulled back.
- **Following ends** when the leader's player, or the follower, has been gone for two minutes, when the leader turns
  Others may follow me off, or on DELETE.
- **Who may ask:** anyone who could follow that player now (the same rule as above), whether following it or not: one
  at the party asks without listening along. Followers may ask for Bandcamp previews (0071); guests only for library
  tracks, named from the library's own facts.
- **Answering:** the player's owner (a shared device's owner), or an administrator. Accept adds the track through Play
  On's add, run as the leader, next or at the end as asked. Requests are kept in memory for two hours, waiting or
  answered; a declined one leaves the leader's list at once and stays in the asker's own list, declined, until then.
- **Telling:** a Play On command `notice {text}` to the account's connected apps, which show it as a notification while
  the app is not in front. The leader: "Ann asks for So What" ("Guest Sam asks for …"), requests within five seconds of
  each other as one, "3 new requests", sent once they settle; not for auto-accepted ones. The asker: "Ann added your
  request: So What", "Ann declined: So What". The pages learn of every change from the players stream, which now
  carries the account's `queue_requests` version beside the players; guests ask again.
- **History:** a play is marked `followed` when its player was following at the time it began (late plays too: the
  times are kept). It is logged and sent to the listening services as any play, but not counted on the song, its album
  and its artists: those counts are what the taste, Discover's own artists and the radio's recent plays read. The
  readers of the log for taste (the taste profile, Sounds like your week) leave followed plays out; the history views
  keep them.
- **Guest page:** the build's second entry (`web/guest.html`), embedded as the shell is. The setting is
  `guest: {on, host: {user} | {player}}` in Settings → Server, the administrators'. Every guest path answers 404 away
  from home or while it is off. A guest is a cookie (`cratarr-guest`, 12 hours, in memory). Covers come as signed links
  (a day). Limits, per guest and per address: 30 searches a minute; 1 request per 30 seconds, 10 an hour, 3 waiting at
  most; 20 names an hour per address. A user host's requests go to the player they listen on then.
- **The page's code** (`/assets/`, fingerprinted) is open without a session: the guest page loads it, and it is the
  open-source app's own code.

## Consequences

- Following is a second, loose way of playing together beside sync play: anywhere, a second or two apart at most,
  one stream each.
- A follower's own music waits in its queue history while it follows.
- Requests and guests are not kept over a restart; the relation is.
- Listening services (ListenBrainz, Last.fm) hear followed plays, so Discover's suggestions from them may reflect them;
  Cratarr's own picks do not.
