// An artist's page asked to open at one of its sections (Home's Artists to explore, the owner 2026-10-23): a tap on
// the description opens it on About, unfolded; a tap on the fanned albums on Albums. The artist's name and the section,
// until their page has done so.
export const pageAsked = $state<{ name: string | null; section: 'about' | 'albums' }>({ name: null, section: 'about' });
