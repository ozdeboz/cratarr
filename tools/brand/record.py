"""Cratarr's mark as one path: a record with three rings and a 60-degree slice to the right where only the rings
remain. Green shapes only, every cut a real gap (no clip paths, nothing drawn over anything), so it renders cleanly
at any size and serves as a monochrome (themed) icon as it is. Coordinates are the Android adaptive icon's: a 108 x 108
canvas, the record (radius 24) inside the 66 dp safe zone.

    python tools/brand/record.py        # prints the path, for android:pathData or an SVG <path d="...">
"""
import math

C = 54.0      # the centre
HALF = 30.0   # half the slice, in degrees: the slice is 60 degrees, its middle pointing right
EDGE = 24.0   # the record's edge
LABEL = 8.0   # the label's edge (the hole is dark)
SPINDLE = 3.0
RINGS = [13.0, 17.0, 21.0]
W = 0.6       # half a ring's width


def pt(r, deg):
    a = math.radians(deg)
    return f"{C + r * math.cos(a):.4f},{C + r * math.sin(a):.4f}"


def band(r0, r1, a0, a1):
    """The ring between radii r0 < r1 from angle a0 to a1 (degrees, clockwise on screen)."""
    large = 1 if (a1 - a0) % 360 > 180 else 0
    return f"M{pt(r1, a0)}A{r1:g},{r1:g} 0,{large},1 {pt(r1, a1)}L{pt(r0, a1)}A{r0:g},{r0:g} 0,{large},0 {pt(r0, a0)}Z"


def sector(r, a0, a1):
    large = 1 if (a1 - a0) % 360 > 180 else 0
    return f"M{C:.4f},{C:.4f}L{pt(r, a0)}A{r:g},{r:g} 0,{large},1 {pt(r, a1)}Z"


def path():
    outside = (HALF, 360 - HALF)  # the 300 degrees round the left
    edges = [LABEL] + [x for r in RINGS for x in (r - W, r + W)] + [EDGE]
    parts = [sector(SPINDLE, *outside)]
    parts += [band(edges[i], edges[i + 1], *outside) for i in range(0, len(edges), 2)]
    parts += [band(r - W, r + W, -HALF, HALF) for r in RINGS]  # in the slice, only the rings
    return "".join(parts)


if __name__ == "__main__":
    print(path())
