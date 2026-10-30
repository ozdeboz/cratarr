/** A length in hours and minutes: "12 h 30 min", "45 min" (Insights' listening time, an album's length). */
export function hoursMinutes(secs: number): string {
  const m = Math.round(secs / 60);
  const h = Math.floor(m / 60);
  return h ? `${h} h${m % 60 ? ` ${m % 60} min` : ''}` : `${m} min`;
}
