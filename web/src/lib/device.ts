/** A phone's screen: its short side under 600 px (the owner, 2026-10-26: by screen size alone). */
export const phoneScreen = typeof window !== 'undefined' && Math.min(screen.width, screen.height) < 600;
/** A phone: touch first and a phone's screen (a tablet keeps lossless). */
export const isPhone = phoneScreen && typeof window !== 'undefined' && matchMedia('(pointer: coarse)').matches;
