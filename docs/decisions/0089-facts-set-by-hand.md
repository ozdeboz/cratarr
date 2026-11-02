# 0089: A record's facts set by hand

- Status: accepted (the owner, 2026-10-31: "a coherent and unified way to modify type, alongside other attributes of
  records"; "Yes, in 0.13"); for 0.13
- Date: 2026-10-31
- Builds on: 0081 (release types, its second amendment: the owner's rule), 0082 (classical marks), 0030 (album tags:
  household-wide, by Music-form id)

## Context

A record's type, year, genres and kind come from its files, MusicBrainz, Discogs and Cratarr's rules. However good the
rules, some records come out wrong (a soundtrack counted classical by one track, a single that is really an EP), and
each fix so far had its own control: classical had a mark, the rest had none.

## Decision

- **One place for what is set by hand.** A record's facts set by hand are kept together (user data
  `record_overrides`), each winning over its files, MusicBrainz, Discogs and the rules until it is reset to what
  Cratarr finds. They are: `type` (album, ep, single, incomplete, other), `secondary` (MusicBrainz's secondary types),
  `year` (its original year, the one shown), `genres` (its tracks' too, so the axes and crates follow) and `classical`.
- **The classical marks are its `classical`.** They move in as the user data opens; the mark endpoint keeps working
  on top of them.
- **Household-wide, by Music-form id**, as album tags and the marks: a setting follows a record kept from Incoming.
  Administrators only.
- **One call for one record or a selection.** `PUT /api/albums/overrides {albums, set}`: a field given is set, `null`
  resets it, one left out is left alone. The library is built again with them before the answer.
- **Shown as set.** An album's `release.set` names its facts set by hand; a type set says `type_from: "set"`. The
  page's Edit sheet (admin, from the album menu and a selection) shows each with a Reset.
- **A file's own words stay in its files.** Title, artists and track numbers are fixed in the tags (Health's fixes,
  Renumber), so every other player sees them too; they cannot be set here.

## Consequences

- A rule's mistake is fixed on the record, not by bending the rule; the rule stays the owner's.
- One store and one call to read for the coming MCP server's "why" (0.13).
