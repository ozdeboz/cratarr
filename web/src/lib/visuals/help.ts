// What each of the visuals' settings does: shown in a popover when its label is held down (held.svelte.ts), never
// otherwise (the panel itself stays labels and values). Each says what it changes, what low and high look like, and
// what it costs where that matters. Keys: a setting's own key (settings.svelte.ts), `aspect:` and `signal:` with a
// route column's or row's key (routes.ts), and a few of the panel's other parts.

export const HELP: Record<string, string> = {
  // How much the device draws.
  points:
    'How many points the flame is drawn from. More gives a finer, fuller and less grainy picture; this is the setting that costs the most graphics power. A device that cannot keep up uses fewer, never more than this.',
  steps:
    'How many times each point moves and is drawn per frame. More fills the flame in faster and smoother, and costs about as much as more points.',
  resolution:
    'How sharp the picture is against the screen’s own pixels. Lower is softer and lighter on the device; the picture is scaled up to fill the screen either way.',
  fps: 'The most frames a second. Lower is calmer on the device. If it cannot keep up, the visuals lower Steps, then Resolution, then Points by themselves (never above what you set) and raise them again when there is room.',

  // How the show looks.
  intensity:
    'How much the music’s loudness sets the size of the show. At 0 the show is always at full strength; at 100% a quiet passage gives a small, calm show and a loud one a big show.',
  flash:
    'How strongly the quick changes show: the lights, glow, size and colour that jump with beats and hits. At 0 only the slow changes remain, such as a passage getting louder; 100% is as the routes set it; 200% doubles every jump.',
  speed: 'How fast the flame turns, following the track’s tempo. At 0 it stands still; at 200% it turns twice as fast.',
  brightness: 'The brightest the picture can ever get, after every flash and glow. Lower it for a dark room.',
  purity:
    'How purely each band’s part keeps its own colour. At 100% every band shows only its own colour; lower lets the colours of the parts blend where they overlap.',
  spread:
    'How far apart the six parts of the flame are drawn. Low pulls them together into one shape; high draws them apart into separate clusters.',
  smoothing:
    'Softens the sparse, grainy parts of the picture into a haze, while the dense, bright parts of the flame stay sharp. At 0 you see the raw grain of single points; 50% is the usual look.',
  floor:
    'Fades the faintest parts of the picture to black, so stray specks and thin mist disappear and only the flame stays. At 0 everything shows.',
  depth:
    'How much faint detail is lifted into view. Lower gives a contrasty picture of only the bright structure; higher shows more of the faint mist and grain around it. 50% is the usual look.',
  colours:
    'Where the colours come from. Rainbow: the look’s own colours. Cover: six colours from the playing album’s cover, the darkest for the bass and the lightest for the highs. Fire: deep red for the bass to yellow for the highs, the densest parts glowing white-hot.',
  symmetry: 'How many times the flame repeats around its middle, like the arms of a star. 1 is no repetition.',
  mirror: 'Mirrors each repeat, like a kaleidoscope. Off keeps all the arms turning the same way.',

  // The look a track or the dice sets (the preset); each can be moved, and then stays as you set it.
  form: 'Which kind of flame this is: each number gives the six parts their own kinds of bends. Detail, Twist and Warp then shape whichever form it is.',
  detail:
    'How tightly each part folds into itself. Low gives dense, compact cores; high gives large, open, wispy forms with more fine detail.',
  twist: 'How much each part turns as it repeats. Low gives straight arms reaching outwards; high winds them into spirals.',
  warp: 'How much each part bends. Low keeps straight lines and sharp edges; high curls them into swirls, bubbles and loops.',
  family:
    'How the Rainbow colours are chosen: spread round the whole colour wheel, along a warm or a cool arc, in two tones, or in three.',
  hue: 'Turns all the colours round the colour wheel together, keeping how they relate to each other.',
  lightness: 'How light the colours are, from deep to pale.',
  vividness: 'How vivid the colours are, from muted to full.',
  final: 'A warp over the whole picture, like looking through a lens, a bubble or a swirl. Off shows the flame as it is.',
  pin: 'You moved this setting, so it keeps your value when the next track starts. Tap the pin to let each track set it again.',
  dice: 'Another look: a new, random setting for every part of the look you have not pinned.',

  // Each band's times.
  lighting:
    'How long this band’s light keeps glowing after each of its hits. Short gives sharp flashes; long gives a slow glow that fades away.',
  evolution:
    'How quickly this band’s part follows changes in the music. Short reacts at once and can look nervous; long moves smoothly and lags a little.',
  continuity:
    'How this band’s part moves. At 0 it moves in steps, on its own hits; at 100% it moves steadily; in between, some of each.',

  // The routes.
  routes:
    'How much each signal from the music moves each part of the picture, from −100% to +100%; tap a square to set it. The top rows are signals of the whole track; the rows below are each band’s own, for the band picked above them (All: every band at once).',
  reset: 'Puts every setting back to its default, the routes too.',
  'routes:reset': 'Puts every route back to its default, the way the visuals responded before the routes existed.',

  // Route columns: what a signal moves.
  'aspect:shape': 'Makes the part grow and change its form as the signal rises.',
  'aspect:rotation':
    'Turns the part as the signal rises; the whole track’s signals turn the whole picture. Below 0 it turns the other way.',
  'aspect:lights': 'Lights the part up in its own colour as the signal rises. Below 0 the part dims instead.',
  'aspect:hue': 'Moves the part’s colour round the colour wheel as the signal rises; below 0, the other way round.',
  'aspect:spread': 'Draws the part out from the middle as the signal rises; below 0, in towards it.',
  'aspect:brightness': 'Makes the whole picture glow brighter as the signal rises; below 0, dimmer.',
  'aspect:size': 'Draws the whole picture larger as the signal rises; below 0, smaller.',
  'aspect:saturation': 'Makes the colours richer as the signal rises; below 0, paler.',
  'aspect:trails': 'Leaves longer trails behind the movement as the signal rises; below 0, shorter ones.',
  'aspect:smoothing': 'Softens the grain into haze as the signal rises; below 0, sharpens it.',
  'aspect:floor': 'Fades the faint parts out as the signal rises; below 0, brings them back.',
  'aspect:depth': 'Lifts more faint detail into view as the signal rises; below 0, less.',
  'aspect:reseed': 'Rolls a new look, as the dice do, each time the signal rises past half way.',
  'aspect:symmetry': 'Picks a new symmetry each time the signal rises past half way.',

  // Route rows: the music's signals. The whole track's:
  'signal:loudness': 'How loud the music sounds right now, as you hear it.',
  'signal:beat': 'A pulse on every beat, strongest on the strong beats.',
  'signal:downbeat': 'A pulse on the first beat of every bar.',
  'signal:tempo': 'How fast the track is: half at 60 beats a minute, full at 120, and further past full for faster tracks.',
  'signal:section': 'A pulse where a new part of the track starts, such as a verse, a chorus or a drop.',
  'signal:harmony': 'Rises when the chords change, and stays low while they hold.',
  'signal:centroid': 'How bright the sound is: low for dark, muffled sounds, high for bright, sharp ones.',
  'signal:key': 'Where the music’s key sits on the circle of fifths. It changes slowly, so it suits slow drifts such as Hue.',
  'signal:swell': 'Rises while the music grows louder over several seconds, and falls back when it stops growing.',
  'signal:buildUp': 'Rises through a build-up, as the music climbs towards a drop.',
  // Each band's:
  'signal:level': 'How loud this band is right now.',
  'signal:flux': 'How much this band’s sound is changing: high on new notes and hits, low on held sounds.',
  'signal:hits': 'A pulse on each hit in this band, glowing out over the band’s Lighting time.',
  'signal:percussive': 'How much of this band is drums and other short, hit sounds.',
  'signal:harmonic': 'How much of this band is held notes and chords.',
  'signal:percussiveHits': 'A pulse on each drum hit in this band, leaving held notes out; it glows out over the band’s Lighting time.',
  'signal:crest': 'How punchy this band is: its sharp peaks against its average loudness.',
  'signal:width': 'How wide this band sounds across the stereo field: low in the middle, high when spread left and right.',
  'signal:bandSwell': 'Rises while this band grows louder over several seconds.',
};
