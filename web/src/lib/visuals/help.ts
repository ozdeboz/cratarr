// What each of the visuals' settings does: shown in a popover when its label is held down (held.svelte.ts), never
// otherwise (the panel itself stays labels and values). Each stands on its own for someone who has never seen the
// visuals explained: it names what it acts on in plain words (the picture, the flame's six parts, the bands of the
// sound, a row or a column of the routes) rather than words that only meant something while it was being built, says
// what its slider's two ends look like (never a middle as "usual": a track's preset sets the look, not a default),
// and what it costs where that matters. Keys: a setting's own key (settings.svelte.ts), `aspect:` and `signal:` with
// a route column's or row's key (routes.ts), `band:` with a band's place (or `all`), and the panel's other parts.

export const HELP: Record<string, string> = {
  // How much the device draws.
  quality:
    'How much the device may draw at most: how many points make the picture, how many times each jumps per frame, and how sharp it is against the screen. At 60% as the visuals have always drawn; at 100% four times the points, twice the jumps and all of the screen’s sharpness, the most work for the graphics chip; at 0 coarse and grainy, but light on the device. A device that cannot keep up draws less by itself, never more than this.',
  fps: 'The most frames a second. At 10 the movement is choppy but light on the device; at 60 it is smoothest. If the device cannot keep up, the visuals draw less by themselves (fewer jumps per frame, then less sharpness, then fewer points; never more than Quality allows) and draw more again when there is room.',

  // How the picture responds and looks.
  intensity:
    'How much the music’s loudness controls how strongly the picture responds to it. At 0 loudness plays no part: the picture always responds in full. At 100% a quiet passage gives a smaller, calmer, paler picture and a loud one a bigger, livelier one: every route answers less in a quiet passage, the fluid’s stirring aside.',
  flash:
    'How much the picture jumps on each beat and hit: its sudden flashes of light and glow, and its sudden changes of size and colour. At 0 nothing jumps and the picture only changes gradually, as when a passage gets louder. At 100% each jump is as large as the routes below set it; at 200%, twice that.',
  speed: 'How fast the flame turns, and the fluid’s flow with it. It follows the track’s tempo, beat by beat, so faster music turns it faster; that is built in, not a route. At 0 it stands still; at 200% it turns twice as fast as at 100%.',
  brightness: 'The brightest the picture can ever get, however it flashes or glows. At 10% it stays very dim, for a dark room; at 100% it can reach the screen’s full brightness.',
  purity:
    'The flame has six parts, one for each band of the sound from the deepest bass to the highest treble, each with its own colour. At 0 the parts’ colours mix where they overlap; at 100% each part keeps only its own colour, so you can tell the bands apart.',
  spread:
    'How far apart the flame’s six parts (one for each band of the sound) are drawn. At 25% they are pulled together into one shape; at 250% they are drawn far apart into separate clusters.',
  shape:
    'Shape: how much of what the routes into the Shape column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 the music leaves the parts’ form as it is.',
  rotation:
    'Rotation: how much of what the routes into the Rotation column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 the music turns nothing.',
  lights:
    'Lights: how much of what the routes into the Lights column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 no part lights up or dims with the music.',
  glow:
    'Glow: how much of what the routes into the Glow column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 the glow stays even.',
  size:
    'Size: how much of what the routes into the Size column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 the picture stays at its resting size.',
  saturation:
    'Saturation: how much of what the routes into the Saturation column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 the colours stay as rich as at rest.',
  trails:
    'Trails: how much of what the routes into the Trails column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 the trails keep their usual length.',
  stir:
    'Stir: how much of what the routes into the Stir column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 nothing pushes the fluid.',
  flow:
    'Flow: how much of what the routes into the Flow column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 the music gives the flow no life.',
  swirl:
    'Swirl: how much of what the routes into the Swirl column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 nothing sets it whirling.',
  presence:
    'Presence: how much of what the routes into the Presence column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 every part keeps its usual share of the picture, whatever its sound.',
  waves:
    'Waves: how much of what the routes into the Waves column give reaches the picture. At 100% all of it; lower, every swing as much smaller, keeping its shape; at 0 no waves run.',
  smoothing:
    'Where few points have landed, the picture looks grainy. Smoothing softens those sparse areas into a soft blur, while the dense areas stay sharp. At 0 you see the raw grain of single points; at 100% the sparse areas blur up to twelve pixels wide. What you set here is the most it reaches: routes into it, and Behaviour’s walk, only ever take it lower.',
  haze:
    'Haze: the faint mist and grain around the flame. At 0 it shows as the visuals always showed it. Towards +100% more of it is lifted into view, until it is almost as bright as the flame; towards −100% less, and the faintest areas fade to black, leaving the bright structure clean. What you set here is the most it reaches either way: routes into it, and Behaviour’s walk, only ever take it toward 0.',
  tunnel:
    'Tunnel: the picture streams outward from the middle for ever, as if flying into it, while the flame itself keeps its size. This is how fast it goes at most: at 0 it is off, at 100% fastest. It can run with Dive. Routes into it, and Behaviour’s walk, only ever take it slower.',
  dive:
    'Dive: the view zooms for ever into one part of the flame that is a small, turned copy of the whole, so the zoom never reaches an end. This is how fast it goes at most: at 0 it is off, at 100% fastest. It can run with Tunnel. Routes into it, and Behaviour’s walk, only ever take it slower.',
  effects:
    'Effects: how strongly the look’s effects show (Effect 1 to 3 and Loop, in the Look). At 100% each as strongly as its Amount; at 0 none of them. Routes into it, and Behaviour’s walk, only ever take it lower.',
  fluid:
    'How much the picture flows like smoke. At 0 the flame’s trails simply fade where they were drawn, and nothing of the fluid runs. Up to 100% they drift and curl more and more in a flow the music stirs, gently by design: at 100% a strong flow, still a flame, not all smoke. How the music stirs it is in the routes’ Stir, Flow and Swirl columns: by default each band’s hits kick its part, beats push out from the centre in a pattern that turns with the tempo, downbeats shove harder and a new section sets a whirl going. Above 0 it costs a little more of the graphics chip. What you set here is the most it reaches: routes into it, and Behaviour’s walk, only ever take it lower.',
  family:
    'Where the colours of the flame’s six parts come from. Spectrum: spread round the whole colour wheel. Arc: along part of it, warm or cool. Two tones: two colours from opposite sides of the wheel, the bands taking them in turn. Three: three colours, each for two neighbouring bands. Each track’s look picks one of these four, with Hue, Lightness and Vividness. Cover: six colours from the playing album’s cover, the darkest for the bass and the lightest for the treble. Fire: deep red for the bass to yellow for the treble, the densest areas glowing white-hot. Choose one and it is pinned.',
  symmetry: 'How many times the flame repeats around its centre, like the arms of a star. At 1 it does not repeat; at 8 it has eight arms.',
  mirror: 'Off: the arms are turned copies of each other, all curling the same way. On: each arm also gets a mirror image, like in a kaleidoscope.',

  // The look a track or the dice sets (the preset); each can be moved, and then stays as you set it.
  look: 'The settings that decide what the flame looks like: its shape and its colours. When a track starts it sets them to that track’s own look, the same each time it plays; the dice set them at random. A setting you move stays where you put it (it is pinned) until you tap its pin.',
  behaviour:
    'Behaviour: how the visuals move with the music. Each of these settings is the most it goes to: Behaviour’s walk (a route into the Behaviour column) moves them a little below what you set and back, never above, and the routes into Smoothing, Haze, Fluid, Tunnel and Dive only ever take those lower (Haze toward 0). Below Speed, each column of the routes has its slider, in the grid’s order; the mark under each slider shows where it is now. Move one and it is pinned: the dice leave it and it no longer walks; tap the pin to let go.',
  form: 'The flame’s basic shape, from 0 to 999. Each number gives the six parts their own kinds of curve, and neighbouring numbers are not alike. Detail, Twist and Warp then shape whichever form it is.',
  detail:
    'How the flame’s parts fold into themselves. At 0 each part is a dense, compact core; at 100% it is large, open and wispy, with more fine detail.',
  twist: 'How much the flame’s parts turn as they repeat into themselves. At 0 they reach straight outwards; at 100% they wind into tight spirals.',
  warp: 'How much the flame’s parts bend. At 0 their lines stay straight and their edges sharp; at 100% they curl into swirls, bubbles and loops.',
  hue: 'Turns all the colours round the colour wheel together, keeping how they relate (not with Cover or Fire). 0° and 360° are the same colours: a full turn.',
  lightness: 'How light the colours are (not with Cover or Fire). At 0 they are deep and rich; at 100% light and pastel.',
  vividness: 'How vivid the colours are (not with Cover or Fire). At 0 they are soft and muted; at 100% as vivid as they go.',
  facets:
    'Facets: where the picture flows like smoke (Fluid above 0), it is carried along in cells, each holding one light, like a mosaic moving with the flow. At 0 there are none; towards 100% the cells’ edges grow harder, until they are hard tiles. Without Fluid it does nothing.',
  lattice:
    'The shape of the cells Facets makes: Square or Hex.',
  fx1:
    'Kaleidoscope: one of its effects at a time over the whole picture: Kaleid (mirrored slices round the middle), Repeat (tiles), Rotate, Scroll, Pixelate or Magnify; None for none. The three engines run in their order, Kaleidoscope, EYEDO, Hydra; a track’s look may bring one or two, and one you choose is pinned.',
  fx1Amount:
    'How strongly the Kaleidoscope’s effect shows. At 0 not at all; at 100% fully. The Effects setting in Behaviour, its routes, and the sidechaining when it holds the picture back take it lower.',
  fx2:
    'EYEDO: one of its effects at a time, after EYESY’s: Pulse, Burst and Halo fire on the beat; Rings and Interference ring on. None for none. The three engines run in their order, Kaleidoscope, EYEDO, Hydra; a track’s look may bring one or two, and one you choose is pinned.',
  fx2Amount:
    'How strongly EYEDO’s effect shows. At 0 not at all; at 100% fully. The Effects setting in Behaviour, its routes, and the sidechaining when it holds the picture back take it lower.',
  fx3:
    'Hydra: one of its effects at a time, after Hydra’s: Echo (the last picture fed back in), Ripple, Stream (drawn along the fluid’s flow), Colorama, Hue, Saturate or Posterize; None for none. The three engines run in their order, Kaleidoscope, EYEDO, Hydra; a track’s look may bring one or two, and one you choose is pinned.',
  fx3Amount:
    'How strongly Hydra’s effect shows. At 0 not at all; at 100% fully. The Effects setting in Behaviour, its routes, and the sidechaining when it holds the picture back take it lower.',
  fxLoop:
    'Loop: an effect inside the flame’s trails rather than over the picture. It acts each frame on the light as it is carried on, so it builds up into new shapes: Kaleid, Pixelate, Ripple, Stream, Rotate, Scroll or Rings; None for none.',
  fxLoopAmount:
    'How strongly the Loop effect acts. At 0 not at all; at 100% fully. The Effects setting in Behaviour, its routes, and the sidechaining when it holds the picture back take it lower.',
  fx1K1:
    'One of the chosen effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  fx1K2:
    'One of the chosen effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  fx2K1:
    'One of the chosen effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  fx2K2:
    'One of the chosen effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  fx3K1:
    'One of the chosen effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  fx3K2:
    'One of the chosen effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  fxLoopK1:
    'One of the Loop effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  fxLoopK2:
    'One of the Loop effect’s own controls: its label says what it sets. At 50% the effect is as it was made.',
  final: 'A lens over the whole picture, such as a sphere, a bubble or a swirl, each bending the flame as a whole, or a shape it gathers the flame into, such as a star or a ring. Off shows the flame without one.',
  lensSides: 'The corners of the Polygon and Star lenses; between whole numbers it morphs.',
  lensDepth: 'How deep the Star lens’s points go; at 0 it is the polygon.',
  pin: 'You moved this setting, so it keeps your value when the next track starts, and Another look and the dice leave it alone. Tap the pin to let each track set it again.',
  snapshots:
    'Snapshots: the visuals saved with every setting and a small picture. Tap one to bring it back: every setting returns as it was saved, and its Look settings are pinned, so the next track keeps that look. Hold one to rename or delete it. They are kept on the server, so every device sees them.',
  'snapshots:standard':
    'The visuals as Cratarr made them before snapshots existed, kept so they can always be brought back: every setting to its default, every pin let go, and each track its own look again. It cannot be renamed or deleted.',
  'snapshots:save':
    'Saves the visuals as they are now: every setting, from the Look and the routes to Quality and Brightness, with a small picture, named after the playing track and the time (tap the name to change it). In a sync group, bringing it back sets the shared settings on every device, and Quality, FPS and Brightness only on the device you bring it back on.',
  'snapshots:over':
    'Save over: puts the visuals as they are now into this snapshot, every setting and a new small picture, keeping its name and its place among the others.',
  another: 'Another look: every Look setting you have not pinned moves to a new random value.',
  randomise:
    'Randomise. A tap: everything at random, a new look (every Look setting you have not pinned) and new values for how the visuals move with the music: the routes and every Behaviour setting, each column’s slider among them, and each band’s times. A double tap: only how they move, the routes into Shape, Lights, Rotation and Size and those four sliders, Speed, Flash and the bands’ times; the look, the colours, the grain and smoke and every other route stay, and until the next roll, an undo or a Reset it may be as lively as Wild at 100% allows. Either way each roll is first tried against the next half minute of the playing track and passed over if it would be livelier than allowed or flash over a quarter of the screen more than three times a second (after eight tries, the one closest to fitting is taken). What you pinned (settings and routes), the Track row, Wild, Quality, FPS and Brightness stay. In a sync group, every device gets it.',
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
  'band:4': 'Upper mid: 2 to 5 kHz. The edge and clarity of voices and snares.',
  'band:5': 'Air: above 5 kHz. Cymbals, hi-hats and breath.',
  'band:all': 'Every band at once: a square set here sets that row for all six bands.',

  // Each band's times.
  lighting:
    'Linger: how long this band’s part of the flame keeps glowing after each hit in this band, such as a drum or a plucked note, and how fast its Envelope lets go when the sound stops. At 20 ms it flashes for an instant; at 1 s it glows and fades over a second.',
  evolution:
    'How quickly this band’s part of the flame changes shape when what moves it changes (at first, this band getting louder or quieter). At 20 ms it changes at once and can look nervous; at 2 s it changes smoothly, taking two seconds to catch up.',
  continuity:
    'How this band’s part of the flame turns and changes. At 0 only in steps, one on each hit in this band; at 100% only smoothly; in between, some of each.',

  // What moves a knob (decision 0056): its routes, from the knob's own page.
  moves:
    'What moves this: the things measured in the music that are routed to it, each with its curve. Open it to see them, change one, or add another.',
  // The routes.
  routes:
    'Routes connect the music to the picture. Each row is something measured in the music, each column something in the picture, and each square a curve: how that row moves that column, as the row goes from nothing to full. Tap a square to shape it. The top rows measure the whole track and act on every part of the flame. The rows below measure one band each, picked above them, and act on that band’s part; in the columns for the whole picture, the bands count together. Each square shows its curve, scaled to fit, and fills from the bottom as far as its route gives right now, green the one way, pink the other. A solid outline means pinned (the dice leave it), a dashed one the square you picked, and a border of long dashes on the move a route that walks with Behaviour.',
  'route:curve':
    'How this row moves this column. Across: the row, from nothing at the left to full at the right. Up: what the route does, from −100% at the bottom (the opposite way, or for Smoothing, Haze, Fluid, Tunnel and Dive, taking it lower; Haze toward 0) through 0 on the middle line to +100% at the top. Drag the two round handles to shape the curve, and the one at the right edge to scale the whole of it; green above the middle line, pink below. The white dot is where the row is now, and the faint shape behind the curve is where it has been over the last half minute of music: put the curve’s bend where it usually is. With All bands picked, the curve is the six bands’ together, dashed where they differ, and shaping it sets all six. Shaping a square pins it.',
  'route:walks':
    'Walks with Behaviour: this route’s effect walks a little below its curve and back as Behaviour’s walk goes, never above it, even while it is pinned. Its square gets a border of long dashes that moves as the walk does.',
  'route:zero': 'Zero: this route does nothing any more, its curve flat on the middle line and its square empty (with All bands picked, for all six). Like any change to a square, it pins the route, so the dice leave it at nothing.',
  'route:pin':
    'Pinned: the dice leave this route as it is, on or off, whatever else they change; its square has a solid outline. Shaping a square pins it; tap to let go. A pinned route still walks if it walks with Behaviour.',
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
  'aspect:smoothing': 'As the row rises, Smoothing comes up to what its slider sets; below 0, the row takes it down instead. Never above the slider.',
  'aspect:haze': 'As the row rises, Haze comes up to what its slider sets, more haze or less; below 0, the row takes it toward none instead. Never past the slider.',
  'aspect:fluid': 'As the row rises, Fluid comes up to what its slider sets and the picture flows more like smoke; below 0, the row takes it down. Never above the slider.',
  'aspect:stir':
    'How hard this row pushes the fluid. By default each band’s hits kick its part of the flame, the beats push out from the centre in a pattern that turns with the tempo, downbeats shove harder, a new section sets a whirl going, and each band’s level stirs it steadily. Across the curve is the push’s own strength (for a hit, how hard it is); below 0 it pushes the other way. Loudness does not scale it.',
  'aspect:flow':
    'How much life the flow has. By default the loudness sets its energy and the beat makes it breathe with the pulse. Below 0 the row calms it instead. Loudness does not scale it beyond its own row.',
  'aspect:swirl':
    'How much the flow whirls. By default a swell sets it turning and a new section starts a whirl. Below 0 it whirls the other way. Loudness does not scale it.',
  'aspect:presence':
    'As the row rises, its part of the flame takes more of the picture (more of the points are drawn as it); below 0, less. In the bands’ rows, each part follows its own sound.',
  'aspect:waves':
    'As the row rises, slow giant waves run across the picture, higher the more it gives; below 0 they run the other way. With Fluid the light rides on them as a broad current; without it the picture bends along them.',
  'aspect:tunnel':
    'As the row rises, Tunnel comes up to the speed its slider sets; below 0, the row slows it instead. Never faster than the slider.',
  'aspect:dive':
    'As the row rises, Dive comes up to the speed its slider sets; below 0, the row slows it instead. Never faster than the slider.',
  'aspect:effects':
    'As the row rises, the look’s effects come up to what Effects sets; below 0, the row takes them down instead. Never above the slider.',
  'aspect:facets':
    'As the row rises, Facets comes up to what its slider sets; below 0, the row takes it down. Never above the slider.',
  'aspect:mirror':
    'Each time the row rises past half way, Mirror turns the other way (on or off), unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:final':
    'Each time the row rises past half way, the Lens moves to another lens, or to none, unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:form':
    'Each time the row rises past half way, the flame takes another Form, unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:family':
    'Each time the row rises past half way, the Colours move to another of the look’s four (Spectrum, Arc, Two tones, Three); Cover and Fire, chosen by hand, stay, unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:lattice':
    'Each time the row rises past half way, the Facets’ cells turn from square to hex or back, unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:fx1':
    'Each time the row rises past half way, the Kaleidoscope moves to another of its effects, its controls back to their middle; where it is set to None it stays off (its Amount’s routes bring it in), unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:fx1Amount':
    'As the row rises, the Kaleidoscope’s Amount comes up to what its slider sets; below 0, the row takes it down instead. Never above the slider.',
  'aspect:fx1K1':
    'As the row rises, the Kaleidoscope’s first control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:fx1K2':
    'As the row rises, the Kaleidoscope’s second control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:fx2':
    'Each time the row rises past half way, EYEDO moves to another of its effects, its controls back to their middle; where it is set to None it stays off (its Amount’s routes bring it in), unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:fx2Amount':
    'As the row rises, the EYEDO’s Amount comes up to what its slider sets; below 0, the row takes it down instead. Never above the slider.',
  'aspect:fx2K1':
    'As the row rises, the EYEDO’s first control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:fx2K2':
    'As the row rises, the EYEDO’s second control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:fx3':
    'Each time the row rises past half way, the Hydra moves to another of its effects, its controls back to their middle; where it is set to None it stays off (its Amount’s routes bring it in), unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:fx3Amount':
    'As the row rises, the Hydra’s Amount comes up to what its slider sets; below 0, the row takes it down instead. Never above the slider.',
  'aspect:fx3K1':
    'As the row rises, the Hydra’s first control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:fx3K2':
    'As the row rises, the Hydra’s second control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:fxLoop':
    'Each time the row rises past half way, the Loop moves to another of its effects, its controls back to their middle; where it is set to None it stays off (its Amount’s routes bring it in), unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',
  'aspect:fxLoopAmount':
    'As the row rises, the Loop’s Amount comes up to what its slider sets; below 0, the row takes it down instead. Never above the slider.',
  'aspect:fxLoopK1':
    'As the row rises, the Loop’s first control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:fxLoopK2':
    'As the row rises, the Loop’s second control (named above for the chosen effect) moves up from where its slider sets it, across its whole range at a route’s full amount; below 0, down. With no such control (—), the row does nothing until an effect has one.',
  'aspect:look':
    'As the row rises, the flame’s look walks: its Detail, Twist, Warp, colours, Spread and Purity move a little further in a direction that changes every bar, never far from where the track set them. Below 0 the look walks back. Pinned settings stay where they are.',
  'aspect:behaviour':
    'As the row rises, the behaviour walks: the Behaviour settings, and the routes set to walk with it, move a little below what you set, in a direction that changes every bar, never above it and never far below. Below 0 they come back up to what you set. Pinned ones stay where they are.',
  'aspect:symmetry':
    'Each time the row rises past half way, the Symmetry setting moves to another of its eight, unless you have pinned it. Each such moment is a new look: they come at most every two seconds, further apart when the picture is busy.',

  // Route rows: what is measured in the music. The whole track's:
  'signal:loudness': 'How loud the music sounds right now, as you hear it.',
  'signal:beat': 'A pulse on every beat, strongest on the strong beats.',
  'signal:downbeat': 'A pulse on the first beat of every bar.',
  'signal:tempo': 'How fast the track is: half at 60 beats a minute, full at 120, and further past full for faster tracks. It fades where the beat does, in a break or a track without one.',
  'signal:section': 'A pulse where a new part of the track starts, such as a verse, a chorus or a drop.',
  'signal:harmony': 'Rises when the chords change, and stays low while they hold.',
  'signal:centroid': 'How bright the sound is: low for dark, muffled sounds, high for bright, sharp ones.',
  'signal:key': 'Which key the music is in, as a place on a circle where related keys sit side by side. It changes slowly, so it suits slow drifts such as Hue.',
  'signal:swell': 'Rises while the music grows louder over several seconds, and falls back when it stops growing.',
  'signal:buildUp': 'Rises through a build-up, as the music climbs towards a drop.',
  'signal:track':
    'A pulse as each new track starts. Into Look it decides what a new track does to the look: at −100% the look becomes the new track’s own, at −50% it goes halfway there, at 0 it stays as it is, and above 0 it moves away from the track’s own. Into Behaviour: at −100% the behaviour starts from what you set again, at 0 it walks on from where it was, and above 0 it starts further below.',
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
  'signal:envelope': 'How present this band’s sound is now: on each hit it flares to the hit’s strength and rings down over the band’s Linger, and while the sound holds it stays at up to 60% of the band’s level. A struck drum flares and is gone; a held pad glows; a struck chord flares, then glows while it rings.',
};
