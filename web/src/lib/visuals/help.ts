// What each of the visuals' settings does, in a sentence or two: shown in a popover when its label is held down (or
// focused and asked for), never otherwise (the panel itself stays labels and values only).
// Keys: a setting's own key (settings.svelte.ts), `aspect:` and `signal:` with a route column's or row's key
// (routes.ts), and a few of the panel's other parts.

export const HELP: Record<string, string> = {
  // How much the device draws.
  points: 'How many points draw the flame. More gives a finer, fuller picture and costs the most graphics power.',
  steps: 'How many times each point moves and is drawn per frame. More fills the picture in faster, at a cost.',
  resolution: 'How sharp the picture is drawn against the screen. Lower is softer and cheaper.',
  fps: 'The most frames a second. When the device cannot keep up, Points, Steps and Resolution step down under their settings to hold it.',

  // How the show looks.
  intensity: 'How much the music’s loudness sets the show. At 0 the show is always full; at 100% a quiet passage is a small show.',
  flash: 'How much the quick changes show: lights, glow, size and colour on beats and hits. At 0 only the slow changes remain.',
  speed: 'How fast the flame turns, at the track’s tempo.',
  brightness: 'The brightest the picture ever gets, after every flash.',
  purity: 'How purely each band’s part keeps its own colour. Lower lets the colours of the bands blend.',
  spread: 'Draws the six parts apart into separate clusters, or together into one shape.',
  smoothing: 'Blurs the sparse, grainy parts into a soft haze. The dense parts of the flame stay sharp.',
  floor: 'Fades the faintest parts to black, so stray specks go and the flame stays.',
  depth: 'How much faint detail is lifted. Lower shows only the bright structure; higher shows more mist.',
  colours:
    'Rainbow: the flame’s own colours. Cover: six colours from the album cover, darkest for the bass. Fire: deep red to yellow, the densest parts glowing white.',
  symmetry: 'How many times the flame repeats around its middle.',
  mirror: 'Mirrors each repeat, like a kaleidoscope.',

  // The look a track or the dice sets (the preset).
  detail: 'How tightly each part folds into itself. Low gives dense, compact cores; high gives large, wispy forms.',
  twist: 'How much each part turns as it repeats. Low gives straight arms; high gives spirals.',
  warp: 'How much each part bends. Low keeps straight lines; high curls it into swirls and bubbles.',
  form: 'Which bends each part uses. Every number is another kind of flame; Detail, Twist and Warp shape whichever it is.',
  family: 'How the colours are chosen under Rainbow: around the whole wheel, along a warm or cool arc, two tones, or three.',
  hue: 'Turns all the colours round the colour wheel.',
  lightness: 'How light the colours are.',
  final: 'A warp over the whole picture, like a lens or a swirl.',
  pin: 'You set this one: it keeps your value when the next track starts. Tap to let the track set it again.',
  dice: 'Another look: a new preset in every setting you have not pinned.',

  // Each band's times.
  lighting: 'How long this band’s light rings after a hit. Short is a sharp flash; long a slow glow.',
  evolution: 'How quickly this band’s part follows the music. Short is quick and nervous; long smooth.',
  continuity: 'How this band’s part moves: at 0 in steps on its own hits, at 100% steadily.',

  // The routes.
  routes:
    'How much each signal of the music moves each part of the picture, from −100% to +100%. The top rows are the whole track; the rows below, each band’s.',
  reset: 'Every setting back to its default.',
  'routes:reset': 'Every route back to its default.',

  // Route columns: what a signal moves.
  'aspect:shape': 'The part grows and changes form.',
  'aspect:rotation': 'The part turns, and the whole picture with the whole track’s signals.',
  'aspect:lights': 'The part lights up in its own colour; below 0 it dims.',
  'aspect:hue': 'The part’s colour moves round the wheel.',
  'aspect:spread': 'The part draws out from the middle, or in below 0.',
  'aspect:brightness': 'The whole picture glows brighter, or dimmer below 0.',
  'aspect:size': 'The whole picture is drawn larger, or smaller below 0.',
  'aspect:saturation': 'The colours get richer, or paler below 0.',
  'aspect:trails': 'Longer trails behind the movement, or shorter below 0.',
  'aspect:smoothing': 'The grain softens into haze, or sharpens below 0.',
  'aspect:floor': 'The faint parts fade out, or come back below 0.',
  'aspect:depth': 'More faint detail is lifted, or less below 0.',
  'aspect:reseed': 'A new look when the signal rises past half.',
  'aspect:symmetry': 'A new symmetry when the signal rises past half.',

  // Route rows: the music's signals. The whole track's:
  'signal:loudness': 'How loud the music sounds now.',
  'signal:beat': 'A pulse on every beat.',
  'signal:downbeat': 'A pulse on the first beat of every bar.',
  'signal:tempo': 'How fast the track is: full at 120 beats a minute.',
  'signal:section': 'A pulse where a new section starts, such as a verse, a chorus or a drop.',
  'signal:harmony': 'How much the chords are changing.',
  'signal:centroid': 'How bright the sound is: dull and dark low, bright and sharp high.',
  'signal:key': 'Where the music’s key sits on the circle of fifths. It moves slowly.',
  'signal:swell': 'Rises while the music is getting louder over several seconds.',
  'signal:buildUp': 'Rises through a build-up towards a drop.',
  // Each band's:
  'signal:level': 'How loud this band is.',
  'signal:flux': 'How much this band’s sound is changing.',
  'signal:hits': 'A pulse on each hit in this band, rung out over its Lighting time.',
  'signal:percussive': 'How much of this band is drums and other hits.',
  'signal:harmonic': 'How much of this band is held notes and chords.',
  'signal:percussiveHits': 'A pulse on each drum hit in this band.',
  'signal:crest': 'How punchy this band is: sharp peaks over its average.',
  'signal:width': 'How wide this band is across the stereo field.',
  'signal:bandSwell': 'Rises while this band is getting louder over several seconds.',
};
