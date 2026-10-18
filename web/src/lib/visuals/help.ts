// What each of the visuals' settings does: shown in a popover when its label is held down (held.svelte.ts), never
// otherwise (the panel itself stays labels and values). Each says what it changes and what its slider's two ends look
// like, never a middle as "usual" (a track's preset sets the look, not a default), and what it costs where that
// matters. Keys: a setting's own key (settings.svelte.ts), `aspect:` and `signal:` with a route column's or row's key
// (routes.ts), and a few of the panel's other parts.

export const HELP: Record<string, string> = {
  // How much the device draws.
  points:
    'How many points the flame is drawn from: the setting that costs the most graphics power. At 16k the picture is coarse and grainy but light on the device; at 2M it is finest and fullest, and heaviest. A device that cannot keep up uses fewer, never more than this.',
  steps:
    'How many times each point moves and is drawn per frame. At 1 the flame fills in slowly and costs least; at 16 it fills in fastest and smoothest, costing about as much as sixteen times the points.',
  resolution:
    'How sharp the picture is against the screen’s own pixels. At 25% it is soft and light on the device; at 100% it is as sharp as the screen. Either way it is scaled to fill the screen.',
  fps: 'The most frames a second. At 10 the movement is choppy but light on the device; at 60 it is smoothest. If the device cannot keep up, the visuals lower Steps, then Resolution, then Points by themselves (never above what you set) and raise them again when there is room.',

  // How the show looks.
  intensity:
    'How much the music’s loudness sets the size of the show. At 0 loudness plays no part: the show is always at full strength. At 100% it follows the loudness fully: a quiet passage gives a small, calm show, a loud one a big show.',
  flash:
    'How big the quick jumps are: the lights, glow, size and colour that jump with beats and hits. At 0 there are no jumps, only the slow changes, such as a passage getting louder. At 100% every jump is as big as its routes set it; at 200%, twice that.',
  speed: 'How fast the flame turns, following the track’s tempo. At 0 it stands still; at 200% it turns twice as fast as at 100%.',
  brightness: 'The brightest the picture can ever get, after every flash and glow. At 10% it stays very dim, for a dark room; at 100% it can reach the screen’s full brightness.',
  purity:
    'How purely each band’s part keeps its own colour. At 0 the parts’ colours blend fully where they overlap; at 100% every band shows only its own colour.',
  spread:
    'How far apart the six parts of the flame are drawn. At 25% they are pulled together into one shape; at 250% they are drawn far apart into separate clusters.',
  smoothing:
    'How much the sparse, grainy parts of the picture are softened, while the dense parts of the flame stay sharp. At 0 you see the raw grain of single points; at 100% the sparse parts become a soft haze, up to twelve pixels wide.',
  floor:
    'How faint a part can be and still show. At 0 everything shows, down to single stray points; at 100% everything fainter than the flame’s average fades to black, leaving only its brightest parts.',
  depth:
    'How much faint detail is lifted into view. At 0 only the bright structure shows, with strong contrast; at 100% the faint mist and grain around it are lifted almost as bright as the flame.',
  colours:
    'Where the colours come from. Rainbow: the look’s own colours. Cover: six colours from the playing album’s cover, the darkest for the bass and the lightest for the highs. Fire: deep red for the bass to yellow for the highs, the densest parts glowing white-hot.',
  symmetry: 'How many times the flame repeats around its middle, like the arms of a star. At 1 it does not repeat; at 8 it has eight arms.',
  mirror: 'Off: all the arms turn the same way. On: each arm is mirrored, like a kaleidoscope.',

  // The look a track or the dice sets (the preset); each can be moved, and then stays as you set it.
  look: 'The settings that make the flame. Each track sets them to its own look as it starts, and the dice to a random one; a setting you move keeps your value, pinned, until you tap its pin.',
  form: 'Which kind of flame this is, from 0 to 999: each number gives the six parts their own kinds of bends, and numbers next to each other are not alike. Detail, Twist and Warp then shape whichever form it is.',
  detail:
    'How each part folds into itself. At 0 the parts are dense, compact cores; at 100% they are large, open and wispy, with more fine detail.',
  twist: 'How much each part turns as it repeats. At 0 the arms reach straight outwards; at 100% they wind into tight spirals.',
  warp: 'How much each part bends. At 0 its lines stay straight and its edges sharp; at 100% it curls into swirls, bubbles and loops.',
  family:
    'How the Rainbow colours are chosen. Spectrum: spread round the whole colour wheel. Arc: along part of it, warm or cool. Two tones: two colours across the wheel, the bands taking them in turn. Three: three colours, each for two neighbouring bands.',
  hue: 'Turns all the colours round the colour wheel together, keeping how they relate. 0° and 360° are the same colours: a full turn.',
  lightness: 'How light the colours are. At 0 they are deep and rich; at 100% light and pastel.',
  vividness: 'How vivid the colours are. At 0 they are soft and muted; at 100% as vivid as they go.',
  final: 'A lens over the whole picture, such as a sphere, a bubble or a swirl, each warping the flame as a whole. Off shows the flame without one.',
  pin: 'You moved this setting, so it keeps your value when the next track starts. Tap the pin to let each track set it again.',
  dice: 'Another look: a new, random setting for every part of the look you have not pinned.',

  // Each band's times.
  lighting:
    'How long this band’s light keeps glowing after each of its hits. At 20 ms it is an instant flash; at 1 s a glow that fades over a second.',
  evolution:
    'How quickly this band’s part follows changes in the music. At 20 ms it reacts at once and can look nervous; at 2 s it moves smoothly, taking two seconds to catch up.',
  continuity:
    'How this band’s part moves. At 0 only in steps, on its own hits; at 100% only steadily; in between, some of each.',

  // The routes.
  routes:
    'How much each signal from the music moves each part of the picture, from −100% to +100%; tap a square to set it. The top rows are signals of the whole track; the rows below are each band’s own, for the band picked above them (All: every band at once).',
  reset: 'Puts every setting back to its default, the routes too; every pin is let go and the playing track’s own look comes back.',
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
