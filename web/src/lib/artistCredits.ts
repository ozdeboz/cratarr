// An artist's credits and key (0.10, decision 0083; the owner, 2026-10-30: an artist excluded everywhere is out of every
// crate, radio, Autoplay and Discover, any credit counting): a track's artist line split into its artists, as the
// server splits it (feat., ft., featuring, &, ",", x, vs, with), whole names only; and the key the server keeps an
// artist by, its name loosely (case and accents aside). Pure, so the tests take it as it is.

const SPLIT = /\s*(?:,|&|;)\s*|\s+(?:feat\.?|ft\.?|featuring|with|vs\.?|versus|x|×)\s+/i;

/** The artists a credit names, each once, in the order named: "Air feat. Beck & Jarvis Cocker" → Air, Beck, Jarvis Cocker. */
export function creditsOf(artist: string | null | undefined): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const part of (artist ?? '').split(SPLIT)) {
    const name = part.replace(/^\(|\)$/g, '').trim();
    const key = artistKey(name);
    if (!key || seen.has(key)) continue;
    seen.add(key);
    out.push(name);
  }
  return out;
}

/** The artist's key as the server keeps it: the name loosely, accents and case aside, spaces as one. */
export const artistKey = (name: string) =>
  name.normalize('NFKD').replace(/\p{M}/gu, '').toLowerCase().replace(/\s+/g, ' ').trim();
