# Plans

Big ideas that are not decided and not being built. Each entry gives what it is, why it waits, and the catches known so
far. A plan that is taken up becomes a decision in [decisions](decisions) first, and its entry here points to it.

Most of these wait for the same reason. Cratarr is a **thin client**: the server holds the library, the queues (0033)
and the pages, and the app shows the server's pages, with a native player under them (0031). The app keeps tracks for
offline play (0038), but it only knows what the server tells it. Each plan below asks the phone, or a car, to know
something the server doesn't.

## External library

Another music server's library played in Cratarr's app directly: an OpenSubsonic server, a Jellyfin, through the same
kind of adapters Cratarr's own sources use (0005).

- Waits for: the thin-client model. Today another server's music reaches the app only through Cratarr's server, as one
  of its sources.
- Catches:
  - The app would need its own adapters, its own sign-in for each server, and a library view that isn't a server page.
  - Queues, Play On and the history are held by Cratarr's server (0033). A track that only the phone can reach can't be
    handed over to another player, and its plays are not counted where the rest are.
  - Crates, playlists and analysis (0024, 0002, 0035) work on what the server catalogues. An external track would have
    no tempo, loudness or waveform unless the phone analysed it.

## Local library

The phone's own music files, played in Cratarr's app beside the server's.

- Waits for: the thin-client model, as above.
- Catches:
  - Reading the phone's files needs Android's media permission and an index of its own on the phone.
  - The same split as the external library: queues, Play On, history and crates live on the server, the files on the
    phone. Mixing both in one queue means a queue the server can't fully play elsewhere.
  - Not the same as kept tracks (0038): those are the server's tracks copied to the phone, and stay the server's.

## CarPlay

Cratarr's player in the car, on an iPhone.

- Waits for: an app on iOS. Cratarr has an Android app only (0031).
- Catches:
  - CarPlay shows an audio app through Apple's own list and now-playing templates, not a web page. The thin client's
    pages can't be its face in the car: the browsing would be native.
  - An audio app on CarPlay needs Apple's CarPlay entitlement for it, asked for and granted per app.
  - Android Auto, the counterpart on Android, was taken out of the app with the rest of Jellyfin's extras (0031).
    It is designed again and waits for a car or head unit that runs it (0055: the owner's tabs, voice and start; the
    car's JVC unit does not), and would come before iOS: Android Auto browses through a media library service
    (Media3), the kind of service the app's player already runs in.
