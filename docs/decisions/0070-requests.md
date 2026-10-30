# 0070: Requests: Want for everyone, in a queue Cratarr keeps

- Status: accepted (the owner, 2026-10-24: for someone not an administrator Want "shouldn't automatically monitor in
  Lidarr. It should use a queue the way Seerr works"; "the request queue would come in 0.9"; the queue on a page of its
  own, "Requests, left of Downloads"; every account may request; no limits for now)
- Date: 2026-10-24
- Release: 0.9
- Builds on: 0013 (Incoming), 0051 (accounts and administrators), 0067 (one import, its matcher), 0069 (Discover)

## Context

Discover's Want (0069) sends an album to Lidarr: the artist added where Lidarr has not got it (its other albums
unmonitored), the album monitored and searched for, so it arrives in Incoming. Since 0.8.9 Want is an administrator's
only: anyone else's is refused ("Want is an administrator's: ask one"), and the page shows it unavailable. Everyone else
can see what they would like, but not ask for it.

Seerr (the 2025 merger of Overseerr and Jellyseerr, MIT, github.com/seerr-team/seerr) solves this for films and series
in front of Radarr and Sonarr: a request is asked for, an administrator approves or declines it, and only an approved
one reaches Radarr. Its model is taken here, its code is not (TypeScript, for films and series); it is credited.

Lidarr is, for Cratarr, an acquisition engine: a want list by MusicBrainz plus a search-and-import pipeline that drops
albums into Incoming. Music also comes from elsewhere than Soulseek, so Lidarr stays; but the want list is Cratarr's
(the owner liked the intermediate step): Lidarr is one way of carrying a request out.

## Decision

**Cratarr keeps the requests, each with its own status, and hands an approved one to a fetcher.**

- **A request:** an album (MusicBrainz release group where known, title, artist, year, its cover), who asked, when,
  and its status: *pending*, *approved*, *declined* (with the reason, if one was given) or *cancelled* (by the one who
  asked, while pending). Kept in Cratarr's data folder, the account's by name (0051).
- **Who may do what**, per account (the administrators' Accounts, 0051):
  - *Request*: on for every account out of the box; an administrator can take it away.
  - *Auto-approve*: off out of the box; an administrator's own Wants are always approved at once (Want as in 0069).
  - *Manage requests*: the administrators; can be given to an account.
- **Approved, it is fetched:** by a *fetcher*, behind one interface (`want`, `status`, `cancel`), so another (slskd
  directly, say) could take Lidarr's place without the requests, their queue or their statuses changing. Lidarr is the
  first: 0069's Want, with the quality and metadata profiles and root folder chosen on Lidarr's page (0.8.9), searched
  again only when the album is out and not there yet (Seerr's rule).
- **Two statuses side by side:** the request's (pending, approved, declined) and the album's: *requested*, *being
  fetched* (Lidarr's queue, with how far), *in Incoming*, *in the library* (0067's matcher), *partly there*. The one who
  asked sees both on the album in Discover and on the Requests page.
- **A page of its own, Requests, left of Downloads:**
  - everyone: their own requests, newest first, with both statuses; a pending one can be cancelled;
  - those who manage requests: everyone's pending requests too, approved or declined one at a time or many at once (the
    select mode every list has), a reason optional;
  - and **"Lidarr is looking for these"** (the owner: "surface the monitor of Lidarr in Cratarr"): what Lidarr has
    monitored and not found yet (its Wanted, `/api/v1/wanted/missing`) and what it is fetching now (`/api/v1/queue`),
    each with how far it is, so the requests and Lidarr's own watch read as one.
- **Health:** "3 requests waiting", for those who manage requests, opening the Requests page.
- **Discover:** Want for everyone who may request: "Requested" on the card once asked, then the album's status;
  declined shows the reason. An administrator's Want goes straight through, as in 0069. An album asked for stays on
  Discover, its status on its card, until the library has it (0069's Want took it off the list at once).
- **Taken back:** a pending request by the one who asked (or by those who manage requests); an approved one only by
  those who manage requests, the fetcher then told to stop looking for it (`cancel`; what is there already stays).
- **The rights are the account's permissions** (0051's `Permissions`, the Users panel): *Request albums*, *Requests
  approved at once*, *Answer requests*, beside the others.
- **No limits** in 0.9 (Seerr's per-week limits can come later).

## Consequences

- Want reaches Lidarr only through an approved request; an administrator's approval (or their own Want, or an account
  with Auto-approve) is the one way in.
- The requests outlive Lidarr: another fetcher can carry them out later, and a Lidarr that is gone leaves the requests
  and their history as they were.
- The page reads Lidarr's Wanted and queue while it is open (Lidarr is asked, not watched), so a Lidarr that is slow or
  down shows as such and nothing else waits on it.
- A new store in the data folder (requests, with their statuses and history), and three permissions on each account.
