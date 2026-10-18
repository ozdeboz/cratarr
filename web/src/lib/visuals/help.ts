// What each of the visuals' settings does: shown in a popover when its label is held down (held.svelte.ts), never
// otherwise (the panel itself stays labels and values). Each stands on its own for someone who has never seen the
// visuals explained: it names what it acts on in plain words (the picture, the flame's six parts, the bands of the
// sound, a row or a column of the routes) rather than words that only meant something while it was being built, says
// what its slider's two ends look like (never a middle as "usual": a track's preset sets the look, not a default),
// and what it costs where that matters. Keys: a setting's own key (settings.svelte.ts), `aspect:` and `signal:` with
// a route column's or row's key (routes.ts), `band:` with a band's place (or `all`), and the panel's other parts.

export const HELP: Record<string, string> = {
  // How much the device draws.
  points:
    'The picture is drawn by points that keep jumping around the flame’s shape, leaving light where they land. More points give a finer, fuller, less grainy picture and the most work for the graphics chip: at 16k it is coarse and grainy but light on the device; at 2M finest and heaviest. A device that cannot keep up uses fewer, never more than this.',
  steps:
    'How many times each point jumps and lands per frame. At 1 the picture fills in slowly and costs least; at 16 it fills in fastest and smoothest, costing about as much as sixteen times the points.',
  resolution:
    'How sharp the picture is against the screen’s own pixels. At 25% it is soft and light on the device; at 100% it is as sharp as the screen. Either way it is scaled to fill the screen.',
  fps: 'The most frames a second. At 10 the movement is choppy but light on the device; at 60 it is smoothest. If the device cannot keep up, the visuals lower Steps, then Resolution, then Points by themselves (never above what you set) and raise them again when there is room.',

  // How the picture responds and looks.
  intensity:
    'How much the music’s loudness controls how strongly the picture responds to it. At 0 loudness plays no part: the picture always responds in full. At 100% a quiet passage gives a smaller, calmer, paler picture and a loud one a bigger, livelier one.',
  flash:
    'How much the picture jumps on each beat and hit: its sudden flashes of light and glow, and its sudden changes of size and colour. At 0 nothing jumps and the picture only changes gradually, as when a passage gets louder. At 100% each jump is as large as the routes below set it; at 200%, twice that.',
  speed: 'How fast the flame turns. It follows the track’s tempo, so faster music turns it faster. At 0 it stands still; at 200% it turns twice as fast as at 100%.',
  brightness: 'The brightest the picture can ever get, however it flashes or glows. At 10% it stays very dim, for a dark room; at 100% it can reach the screen’s full brightness.',
  purity:
    'The flame has six parts, one for each band of the sound from the deepest bass to the highest treble, each with its own colour. At 0 the parts’ colours mix where they overlap; at 100% each part keeps only its own colour, so you can tell the bands apart.',
  spread:
    'How far apart the flame’s six parts (one for each band of the sound) are drawn. At 25% they are pulled together into one shape; at 250% they are drawn far apart into separate clusters.',
  smoothing:
    'Where few points have landed, the picture looks grainy. Smoothing softens those sparse areas into a haze, while the dense areas stay sharp. At 0 you see the raw grain of single points; at 100% the sparse areas become a soft haze up to twelve pixels wide.',
  floor:
    'How faint an area of the picture can be and still show. At 0 everything shows, down to single stray points; at 100% everything fainter than the flame’s average fades to black, leaving only its brightest areas.',
  depth:
    'How much faint detail is lifted into view. At 0 only the bright structure shows, with strong contrast; at 100% the faint mist and grain around it are lifted almost as bright as the flame.',
  fluid:
    'How much the picture flows like smoke. At 0 the flame’s trails simply fade where they were drawn, and nothing of the fluid runs. Up to 100% they drift and curl more and more in a flow the music stirs, gently by design: at 100% a strong flow, still a flame, not all smoke. Each band’s part is kicked on its hits, beats push out from the centre in a pattern that turns with the tempo, downbeats shove harder and a new section sets a whirl going. Above 0 it costs a little more of the graphics chip.',
  colours:
    'Where the colours of the flame’s six parts come from. Rainbow: from the Look settings Family, Hue, Lightness and Vividness. Cover: six colours from the playing album’s cover, the darkest for the bass and the lightest for the treble. Fire: deep red for the bass to yellow for the treble, the densest areas glowing white-hot.',
  symmetry: 'How many times the flame repeats around its centre, like the arms of a star. At 1 it does not repeat; at 8 it has eight arms.',
  mirror: 'Off: the arms are turned copies of each other, all curling the same way. On: each arm also gets a mirror image, like in a kaleidoscope.',

  // The look a track or the dice sets (the preset); each can be moved, and then stays as you set it.
  look: 'The settings that decide what the flame looks like: its shape and its colours. When a track starts it sets them to that track’s own look, the same each time it plays; the dice set them at random. A setting you move stays where you put it (it is pinned) until you tap its pin.',
  form: 'The flame’s basic shape, from 0 to 999. Each number gives the six parts their own kinds of curve, and neighbouring numbers are not alike. Detail, Twist and Warp then shape whichever form it is.',
  detail:
    'How the flame’s parts fold into themselves. At 0 each part is a dense, compact core; at 100% it is large, open and wispy, with more fine detail.',
  twist: 'How much the flame’s parts turn as they repeat into themselves. At 0 they reach straight outwards; at 100% they wind into tight spirals.',
  warp: 'How much the flame’s parts bend. At 0 their lines stay straight and their edges sharp; at 100% they curl into swirls, bubbles and loops.',
  family:
    'How the six parts’ colours are chosen, with Rainbow colours. Spectrum: spread round the whole colour wheel. Arc: along part of it, warm or cool. Two tones: two colours from opposite sides of the wheel, the bands taking them in turn. Three: three colours, each for two neighbouring bands.',
  hue: 'Turns all the colours round the colour wheel together, keeping how they relate (with Rainbow colours). 0° and 360° are the same colours: a full turn.',
  lightness: 'How light the colours are, with Rainbow colours. At 0 they are deep and rich; at 100% light and pastel.',
  vividness: 'How vivid the colours are, with Rainbow colours. At 0 they are soft and muted; at 100% as vivid as they go.',
  final: 'A lens over the whole picture, such as a sphere, a bubble or a swirl, each bending the flame as a whole. Off shows the flame without one.',
  pin: 'You moved this setting, so it keeps your value when the next track starts, and Another look and the dice leave it alone. Tap the pin to let each track set it again.',
  snapshots:
    'Snapshots: the visuals saved with every setting and a small picture. Tap one to bring it back: every setting returns as it was saved, and its Look settings are pinned, so the next track keeps that look. Hold one to rename or delete it. They are kept on the server, so every device sees them.',
  'snapshots:standard':
    'The visuals as Cratarr made them before snapshots existed, kept so they can always be brought back: every setting to its default, every pin let go, and each track its own look again. It cannot be renamed or deleted.',
  'snapshots:save':
    'Saves the visuals as they are now: every setting, from the Look and the routes to Points and Brightness, with a small picture, named after the playing track and the time (tap the name to change it). In a sync group, bringing it back sets the shared settings on every device, and Points, Steps, Resolution, FPS and Brightness only on the device you bring it back on.',
  another: 'Another look: every Look setting you have not pinned moves to a new random value.',
  randomise:
    'Randomise: everything at random. A new look (every Look setting you have not pinned) and new values for how the visuals move with the music: the routes, Flash, Speed, Intensity, Fluid, Smoothing, Floor, Depth and each band’s times. Each roll is first tried against the next half minute of the playing track: one that would get livelier than Wild allows, or flash over a quarter of the screen more than three times a second, is passed over for another (after eight tries, the one closest to fitting is taken). Colours, Wild, Points, Steps, Resolution, FPS and Brightness stay as they are. In a sync group, every device gets it.',
  'thumbs-up':
    'Thumbs up: you like the visuals as they are now. If they are being held back just then (the amber dot), the limit doing it is eased a step, so they get more room. Otherwise your thumb is only kept, with how lively the last 20 seconds were, to tune the limits later. The limit on flashes never changes.',
  'thumbs-down':
    'Thumbs down: you don’t like the visuals as they are now. If one kind of liveliness stood out over the last 20 seconds (turning, changes of shape or colour, pumping in size, new looks), its limit is tightened a step at once, to just under what it was then. If nothing stood out, your thumb is only kept, to tune the limits later: it was probably the look, not the liveliness. What the thumbs teach drifts halfway back in a month. The limit on flashes never changes.',
  forget: 'Forget: lets go of everything the thumbs taught. The limits go back to where they started, on all your devices. Your thumbs themselves stay kept.',
  rate:
    'Rate: your answers set the limits Wild works within. The visuals show two ways of moving with the music, A for 20 seconds and then B, and you choose the one you liked better, or Same, or Neither. Each answer is kept on the server with how lively A and B were, and the next pair starts. About twenty pairs, a quarter of an hour, are enough to begin with. The cross stops it, and the next time it carries on from where you stopped on this device. What you see while rating never changes your own settings.',
  undo: 'Undo: puts the visuals back as they were before the last roll of the dice: the look, the pins and how they move with the music, on every device in the sync group.',
  wild:
    'How lively the visuals may get. They are kept within limits on how fast they turn, how quickly their shape and colours change, how much they pump in size and how often a new look comes. At 0 they stay within half of those limits, calm; at 100% they may go to twice them, wild. Loud music gets a little more room than quiet. Flashes over a quarter of the screen stay at most three a second either way.',
  held: 'Held back: the visuals are doing more than Wild allows just now (or flashing over a large area too often), so this is turned down for a moment. It eases back by itself.',

  // The bands, as headings of their own settings and as the routes' choice of band.
  'band:0': 'Sub: the deepest bass, below 60 Hz. Kick drums and sub-bass, felt more than heard.',
  'band:1': 'Bass: 60 to 150 Hz. Bass lines and the body of kick drums.',
  'band:2': 'Low mid: 150 to 400 Hz. The warmth of voices, guitars and pianos.',
  'band:3': 'Mid: 400 Hz to 2 kHz. Most of voices and melodies.',
  'band:4': 'Presence: 2 to 5 kHz. The edge and clarity of voices and snares.',
  'band:5': 'Air: above 5 kHz. Cymbals, hi-hats and breath.',
  'band:all': 'Every band at once: a square set here sets that row for all six bands.',

  // Each band's times.
  lighting:
    'How long this band’s part of the flame keeps glowing after each hit in this band, such as a drum or a plucked note. At 20 ms it flashes for an instant; at 1 s it glows and fades over a second.',
  evolution:
    'How quickly this band’s part of the flame changes shape when what moves it changes (at first, this band getting louder or quieter). At 20 ms it changes at once and can look nervous; at 2 s it changes smoothly, taking two seconds to catch up.',
  continuity:
    'How this band’s part of the flame turns and changes. At 0 only in steps, one on each hit in this band; at 100% only smoothly; in between, some of each.',

  // The routes.
  routes:
    'Routes connect the music to the picture. Each row is something measured in the music, each column something in the picture, and each square how much that row moves that column, from −100% (the opposite way) to +100%; tap a square to set it. The top rows measure the whole track and act on every part of the flame. The rows below measure one band each, picked above them, and act on that band’s part; in the columns for the whole picture, the bands count together.',
  reset: 'Puts every setting back to its default, the routes too; every pin is let go and the playing track’s own look comes back.',
  'routes:reset':
    'Puts every square back to its default: loudness sets the picture’s size, colour and turning; beats make it flash and swell; each band’s loudness shapes, turns and tints its part, and its hits light it up.',

  // Route columns: what a row moves.
  'aspect:shape': 'As the row rises, its part of the flame grows and changes form (with the whole-track rows, every part).',
  'aspect:rotation':
    'As the row rises, its part of the flame turns (with the whole-track rows, the whole picture). Below 0 it turns the other way.',
  'aspect:lights': 'As the row rises, its part of the flame lights up in its own colour. Below 0 the part dims instead.',
  'aspect:hue': 'As the row rises, its part’s colour moves round the colour wheel; below 0, the other way round.',
  'aspect:spread': 'As the row rises, its part of the flame is drawn out from the centre; below 0, in towards it.',
  'aspect:brightness': 'As the row rises, the whole picture glows brighter; below 0, dimmer.',
  'aspect:size': 'As the row rises, the whole picture is drawn larger; below 0, smaller.',
  'aspect:saturation': 'As the row rises, the colours get richer; below 0, paler.',
  'aspect:trails': 'As the row rises, the movement leaves longer trails; below 0, shorter ones.',
  'aspect:smoothing': 'As the row rises, the grain softens into haze, as the Smoothing setting does; below 0, it sharpens.',
  'aspect:floor': 'As the row rises, the faint areas fade out, as the Floor setting does; below 0, they come back.',
  'aspect:fluid': 'As the row rises, the picture flows more like smoke, as the Fluid setting does; below 0, less.',
  'aspect:depth': 'As the row rises, more faint detail is lifted into view, as the Depth setting does; below 0, less.',
  'aspect:drift':
    'As the row rises, the flame’s look walks: its Detail, Twist, Warp, colours, Spread and Purity move a little further in a direction that changes every bar, never far from where the track set them. Below 0 the look walks back. Pinned settings stay where they are.',
  'aspect:symmetry':
    'Each time the row rises past half way, the Symmetry and Mirror settings move to new random values, unless you have pinned them.',

  // Route rows: what is measured in the music. The whole track's:
  'signal:loudness': 'How loud the music sounds right now, as you hear it.',
  'signal:beat': 'A pulse on every beat, strongest on the strong beats.',
  'signal:downbeat': 'A pulse on the first beat of every bar.',
  'signal:tempo': 'How fast the track is: half at 60 beats a minute, full at 120, and further past full for faster tracks.',
  'signal:section': 'A pulse where a new part of the track starts, such as a verse, a chorus or a drop.',
  'signal:harmony': 'Rises when the chords change, and stays low while they hold.',
  'signal:centroid': 'How bright the sound is: low for dark, muffled sounds, high for bright, sharp ones.',
  'signal:key': 'Which key the music is in, as a place on a circle where related keys sit side by side. It changes slowly, so it suits slow drifts such as Hue.',
  'signal:swell': 'Rises while the music grows louder over several seconds, and falls back when it stops growing.',
  'signal:buildUp': 'Rises through a build-up, as the music climbs towards a drop.',
  'signal:track':
    'A pulse as each new track starts. Into Drift it decides what a new track does to the look: at −100% the look becomes the new track’s own, at −50% it goes halfway there, at 0 it stays as it is, and above 0 it moves away from the track’s own.',
  // Each band's:
  'signal:level': 'How loud this band is right now.',
  'signal:flux': 'How much this band’s sound is changing: high on new notes and hits, low on held sounds.',
  'signal:hits': 'A pulse on each hit in this band, such as a drum or a plucked note, fading over the band’s Lighting setting.',
  'signal:percussive': 'How much of this band is drums and other short, struck sounds.',
  'signal:harmonic': 'How much of this band is held notes and chords.',
  'signal:percussiveHits': 'A pulse on each drum hit in this band, leaving held notes out, fading over the band’s Lighting setting.',
  'signal:crest': 'How punchy this band is: how far its sharp peaks stand out above its average loudness.',
  'signal:width': 'How wide this band sounds between the left and right speakers: low when it comes from the middle, high when it is spread out.',
  'signal:bandSwell': 'Rises while this band grows louder over several seconds.',
};
