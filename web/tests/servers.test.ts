// Modes per server (decision 0098): a Subsonic and a Jellyfin server as the page reads them (lib/servers), against
// recorded answers: what each call asks, how its answer is read, and how a refusal is said.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import axios, { type AxiosRequestConfig } from 'axios';

const { subsonicServer, signInSubsonic } = await import('../src/lib/servers/subsonic.ts');
const { jellyfinServer, signInJellyfin } = await import('../src/lib/servers/jellyfin.ts');
const { ServerError } = await import('../src/lib/servers/server.ts');

const OK = { status: 'ok', version: '1.16.1', openSubsonic: true, type: 'navidrome', serverVersion: '0.58.0' };

/** A Subsonic server answering each endpoint from `answers`, keeping every call's address. */
function subsonic(answers: Record<string, object>) {
  const calls: URL[] = [];
  const fetcher = (async (input: string) => {
    const url = new URL(input);
    calls.push(url);
    const name = url.pathname.split('/').pop()!.replace('.view', '');
    const body = answers[name] ?? { status: 'failed', error: { code: 70, message: 'Not found' } };
    return new Response(JSON.stringify({ 'subsonic-response': { ...OK, ...body } }));
  }) as typeof fetch;
  return { calls, fetcher };
}

const IN = { id: 's1', kind: 'subsonic' as const, name: 'N', url: 'https://music.example', user: 'listener', secret: 'pw' };

test('a Subsonic sign-in pings, and names the server by its kind and host', async () => {
  const s = subsonic({ ping: {} });
  const signIn = await signInSubsonic('s1', 'https://music.example', 'listener', 'pw', false, s.fetcher);
  assert.equal(signIn.name, 'Navidrome on music.example');
  assert.equal(s.calls[0].searchParams.get('u'), 'listener');
  assert.equal(s.calls[0].searchParams.get('p'), null, 'never the password itself: a salted token');
  assert.ok(s.calls[0].searchParams.get('t') && s.calls[0].searchParams.get('s'));
});

test('a Subsonic refusal is a ServerError, signed out where the sign-in was refused', async () => {
  const s = subsonic({ ping: { status: 'failed', error: { code: 40, message: 'Wrong username or password' } } });
  await assert.rejects(signInSubsonic('s1', 'https://music.example', 'listener', 'pw', false, s.fetcher), (e: InstanceType<typeof ServerError>) => e instanceof ServerError && e.signedOut && e.message === 'Wrong username or password');
});

test("a Subsonic album's tracks are read in the page's shape", async () => {
  const s = subsonic({
    getAlbum: {
      album: {
        id: 'al1', name: 'Moon Safari', artist: 'Air', artistId: 'ar1', year: 1998, songCount: 2, duration: 600, created: '2026-01-02T00:00:00Z', coverArt: 'al-1',
        song: [
          { id: 't1', title: 'La femme d’argent', artist: 'Air', artistId: 'ar1', album: 'Moon Safari', albumId: 'al1', track: 1, discNumber: 1, duration: 427, isDir: false, starred: '2026-02-01T00:00:00Z' },
          { id: 't2', title: 'Sexy Boy', artist: 'Air', album: 'Moon Safari', albumId: 'al1', track: 2, isDir: false },
        ],
      },
    },
  });
  const { album, tracks } = await subsonicServer(IN, s.fetcher).album('al1');
  assert.equal(album.title, 'Moon Safari');
  assert.equal(album.added, Date.parse('2026-01-02T00:00:00Z') / 1000);
  assert.deepEqual(tracks.map((t) => [t.id, t.track, t.starred]), [['t1', 1, true], ['t2', 2, false]]);
  assert.equal(tracks[0].duration_secs, 427);
});

test('a Subsonic playlist is saved by writing it again whole, its songs in the new order, those left out gone', async () => {
  const s = subsonic({ getPlaylist: { playlist: { id: 'p1', name: 'P', songCount: 3, duration: 0, created: '', changed: '', entry: [{ id: 'a', title: 'A', isDir: false }, { id: 'b', title: 'B', isDir: false }, { id: 'c', title: 'C', isDir: false }] } }, createPlaylist: { playlist: { id: 'p1' } } });
  const server = subsonicServer(IN, s.fetcher);
  const { tracks } = await server.playlist('p1');
  assert.deepEqual(tracks.map((t) => t.entry), ['0', '1', '2']);
  await server.keepPlaylist('p1', ['2', '0']);
  const write = s.calls.find((u) => u.pathname.endsWith('createPlaylist.view'))!;
  assert.equal(write.searchParams.get('playlistId'), 'p1');
  assert.deepEqual(write.searchParams.getAll('songId'), ['c', 'a']);
  await assert.rejects(server.keepPlaylist('p1', ['0', '7']), /changed on the server/);
});

test('a Subsonic server without similar songs has no mix, and is not asked again', async () => {
  const s = subsonic({});
  const server = subsonicServer(IN, s.fetcher);
  assert.equal(await server.mix('t1', 20), null);
  assert.equal(await server.mix('t1', 20), null);
  assert.equal(s.calls.filter((u) => u.pathname.endsWith('getSimilarSongs2.view')).length, 1);
});

test("a Subsonic stream's address names the track and carries a token, not the password", async () => {
  const url = new URL(await subsonicServer(IN, subsonic({}).fetcher).streamUrl('t1'));
  assert.equal(url.pathname, '/rest/stream.view');
  assert.equal(url.searchParams.get('id'), 't1');
  assert.equal(url.searchParams.get('p'), null);
});

/** A Jellyfin answering each request from `answer`, keeping every request. */
function jellyfin(answer: (c: AxiosRequestConfig) => { status?: number; data?: unknown }) {
  const calls: AxiosRequestConfig[] = [];
  const instance = axios.create({
    adapter: async (config) => {
      calls.push(config);
      const { status = 200, data = {} } = answer(config);
      const response = { data, status, statusText: String(status), headers: {}, config };
      if (status >= 400) throw Object.assign(new Error(`status ${status}`), { response, config, isAxiosError: true });
      return response;
    },
  });
  return { calls, instance };
}
const CLIENT = { version: '0.15.0', device: 'Tablet', deviceId: 'dev-1' };
const JIN = { id: 'j1', kind: 'jellyfin' as const, name: 'Home', url: 'https://jf.example', user: 'listener', userId: 'u1', secret: 'tok' };
const path = (c: AxiosRequestConfig) => new URL(c.url!).pathname;

test('a Jellyfin sign-in keeps the access token and the user, not the password, and names the server', async () => {
  const jf = jellyfin((c) =>
    path(c) === '/Users/AuthenticateByName' ? { data: { AccessToken: 'tok', User: { Id: 'u1', Name: 'listener' } } } : path(c) === '/System/Info/Public' ? { data: { ServerName: 'Home' } } : { status: 404 },
  );
  const signIn = await signInJellyfin('j1', 'https://jf.example', 'listener', 'pw', CLIENT, jf.instance);
  assert.deepEqual({ ...signIn }, { ...JIN });
  assert.ok(!JSON.stringify(signIn).includes('"pw"'));
  assert.match(String(jf.calls[0].headers?.Authorization), /Client="Cratarr".*Device="Tablet".*DeviceId="dev-1"/);
});

test('a Jellyfin refusing the token signs the mode out', async () => {
  const jf = jellyfin(() => ({ status: 401 }));
  await assert.rejects(jellyfinServer(JIN, CLIENT, jf.instance).playlists(), (e: InstanceType<typeof ServerError>) => e.signedOut);
});

test("a Jellyfin's recently played albums come from its tracks, each album once, in their order", async () => {
  const jf = jellyfin((c) => {
    if (new URL(c.url!).searchParams.get('includeItemTypes') === 'Audio') return { data: { Items: [{ Id: 't1', AlbumId: 'b' }, { Id: 't2', AlbumId: 'a' }, { Id: 't3', AlbumId: 'b' }] } };
    return { data: { Items: [{ Id: 'a', Name: 'A' }, { Id: 'b', Name: 'B' }] } };
  });
  const albums = await jellyfinServer(JIN, CLIENT, jf.instance).list('played', 10);
  assert.deepEqual(albums.map((a) => a.id), ['b', 'a']);
});

test('a Jellyfin playlist is saved by taking out those left out, then moving only the entries out of place', async () => {
  const jf = jellyfin((c) => (path(c) === '/Playlists/p1/Items' && c.method === 'get' ? { data: { Items: ['x', 'y', 'z', 'w'].map((e) => ({ Id: `t${e}`, PlaylistItemId: e })) } } : { status: 204 }));
  await jellyfinServer(JIN, CLIENT, jf.instance).keepPlaylist('p1', ['y', 'x', 'z']);
  const out = jf.calls.find((c) => c.method === 'delete')!;
  assert.equal(new URL(out.url!).searchParams.get('entryIds'), 'w');
  const moves = jf.calls.filter((c) => path(c).includes('/Move/')).map(path);
  assert.deepEqual(moves, ['/Playlists/p1/Items/y/Move/0']);
});

test("a Jellyfin stream is the file as it is, with the token", async () => {
  const url = new URL(await jellyfinServer(JIN, CLIENT, jellyfin(() => ({})).instance).streamUrl('t1'));
  assert.equal(url.pathname, '/Audio/t1/stream');
  assert.equal(url.searchParams.get('static'), 'true');
  assert.equal(url.searchParams.get('api_key'), 'tok');
});
