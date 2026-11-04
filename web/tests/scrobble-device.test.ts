// Scrobbling from the device (decision 0091): ListenBrainz's and Last.fm's calls as the device makes them (their
// bodies, Last.fm's signature), and what the queue sends to each (lib/scrobbleDevice).
import { test } from 'node:test';
import assert from 'node:assert/strict';

const d = await import('../src/lib/scrobbleDevice.ts');

const play = (id: string, at: number, extra: Record<string, unknown> = {}) => ({ id, at, artist: 'M. Rivers', title: 'Harbour Lights', album: 'Late Set', duration_secs: 241.6, ...extra });
const answering = (status: number, body: unknown) => {
  const calls: { url: string; init: RequestInit }[] = [];
  const f = (async (url: string, init: RequestInit = {}) => (calls.push({ url, init }), new Response(JSON.stringify(body), { status }))) as unknown as typeof fetch;
  return { calls, f };
};

test("Last.fm's signature: the parameters sorted, without format, the secret last, MD5'd", () => {
  // md5("api_key" "k" "method" "auth.getSession" "token" "t" + "s"), computed apart (Python's hashlib).
  assert.equal(d.lastFmSignature({ token: 't', method: 'auth.getSession', api_key: 'k', format: 'json' }, 's'), '4b1fe62eb056c03672443be528ff2633');
});

test("a Last.fm scrobble names each play by its place, signed, with the session", async () => {
  const s = answering(200, { scrobbles: {} });
  await d.lastFmScrobble('k', 's', 'sk1', [play('a', 100), play('b', 200, { album: null })], s.f);
  const body = new URLSearchParams(String(s.calls[0].init.body));
  assert.equal(body.get('method'), 'track.scrobble');
  assert.equal(body.get('sk'), 'sk1');
  assert.equal(body.get('timestamp[1]'), '200');
  assert.equal(body.get('duration[0]'), '242');
  assert.equal(body.get('album[1]'), null);
  const signed = Object.fromEntries([...body].filter(([k]) => k !== 'api_sig' && k !== 'format'));
  assert.equal(body.get('api_sig'), d.lastFmSignature(signed, 's'));
});

test('a refused session signs out; another refusal does not', async () => {
  await assert.rejects(d.lastFmScrobble('k', 's', 'sk', [play('a', 1)], answering(200, { error: 9, message: 'Invalid session key' }).f), (e: InstanceType<typeof d.ScrobbleError>) => e.signedOut);
  await assert.rejects(d.lastFmScrobble('k', 's', 'sk', [play('a', 1)], answering(200, { error: 29, message: 'Rate limit' }).f), (e: InstanceType<typeof d.ScrobbleError>) => !e.signedOut);
});

test('ListenBrainz: one play single, several an import, now playing without a time', async () => {
  const s = answering(200, { status: 'ok' });
  await d.listenBrainzScrobble('tok', [play('a', 100)], s.f);
  await d.listenBrainzScrobble('tok', [play('a', 100), play('b', 200)], s.f);
  await d.listenBrainzPlaying('tok', play('c', 300), s.f);
  const bodies = s.calls.map((c) => JSON.parse(String(c.init.body)));
  assert.deepEqual(bodies.map((b) => b.listen_type), ['single', 'import', 'playing_now']);
  assert.equal(bodies[0].payload[0].listened_at, 100);
  assert.equal(bodies[0].payload[0].track_metadata.additional_info.duration_ms, 241600);
  assert.equal(bodies[2].payload[0].listened_at, undefined);
  assert.equal((s.calls[0].init.headers as Record<string, string>).Authorization, 'Token tok');
});

test("the queue: each service sent what it has not had; Last.fm nothing older than 14 days", () => {
  const now = 100 * 86_400;
  const q = [play('old', now - 20 * 86_400), play('a', now - 60), play('b', now - 30)];
  assert.deepEqual(d.nextBatch(q, 'lastfm', new Set(), now).map((p) => p.id), ['a', 'b']);
  assert.deepEqual(d.nextBatch(q, 'listenbrainz', new Set(['a']), now).map((p) => p.id), ['old', 'b']);
  const sent = { listenbrainz: new Set(['old', 'a']), lastfm: new Set(['a']) };
  assert.deepEqual(d.remaining(q, ['listenbrainz', 'lastfm'], sent, now).map((p) => p.id), ['b']);
});
