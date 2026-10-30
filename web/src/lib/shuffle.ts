// One shuffle for every way of playing (the architecture audit, 2026-10-29: two copies of Fisher–Yates, and two
// `sort(() => Math.random() - 0.5)`, which is not a fair shuffle: it leaves tracks near where they were).

/** A copy in a random order, every order as likely (Fisher–Yates). */
export function shuffled<T>(items: readonly T[]): T[] {
  const a = [...items];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}
