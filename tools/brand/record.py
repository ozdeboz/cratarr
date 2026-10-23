"""Cratarr's mark as one path: a record with rings and a 60-degree slice to the right where only the rings remain.
Shapes only, every cut a real gap (no clip paths, nothing drawn over anything), so it renders cleanly and serves as a
monochrome (themed) icon as it is. Coordinates are the Android adaptive icon's: a 108 x 108 canvas, the record
(radius 24) inside the 66 dp safe zone.

Three versions by size, as the owner chose (2026-10-15); thin rings shimmer when there are few pixels:
  full  three rings, from 48 px up (the launcher, the app, the web app's icons)
  two   two rings twice as thick, about 32 px (the 32 px tab icon, the status bar on hdpi screens)
  one   one thick ring, 16 to 24 px (the 16 px tab icon, the status bar on mdpi screens)

    python tools/brand/record.py [full|two|one]   # prints the path, for android:pathData or an SVG <path d="...">
    python tools/brand/record.py --web DIR        # writes the web app's icons into DIR (needs resvg)
    python tools/brand/record.py --android RES    # writes the app's launcher and notification icons into RES
"""
import math
import pathlib
import subprocess
import sys

C = 54.0      # the centre
HALF = 30.0   # half the slice, in degrees: the slice is 60 degrees, its middle pointing right
EDGE = 24.0   # the record's edge
LABEL = 8.0   # the label's edge (the hole is dark)
SPINDLE = 3.0
# (ring radii, ring width) per version; each keeps the gap at the edge smaller than the gap at the label
VERSIONS = {"full": ([13.0, 17.0, 21.0], 1.2), "two": ([14.0, 19.5], 2.4), "one": ([16.5], 3.0)}

GREEN, DARK = "#12A36A", "#06281A"  # 0.8: the one accent for both themes (the owner, 2026-10-24)


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


def path(version="full"):
    rings, width = VERSIONS[version]
    w = width / 2
    outside = (HALF, 360 - HALF)  # the 300 degrees round the left
    edges = [LABEL] + [x for r in rings for x in (r - w, r + w)] + [EDGE]
    parts = [sector(SPINDLE, *outside)]
    parts += [band(edges[i], edges[i + 1], *outside) for i in range(0, len(edges), 2)]
    parts += [band(r - w, r + w, -HALF, HALF) for r in rings]  # in the slice, only the rings
    return "".join(parts)


def icon_svg(version, px, bare):
    """A whole icon. Bare: the mark alone, the record's circle and nothing round it (the mark is only the circle, the
    owner said 2026-10-16), filling the icon. Otherwise the dark square, full bleed, with the mark as the launcher shows
    it (72 of 108), for where the platform cuts the icon's shape itself."""
    if bare:
        return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="{C - EDGE:g} {C - EDGE:g} {2 * EDGE:g} {2 * EDGE:g}">'
                f'<path fill="{GREEN}" d="{path(version)}"/></svg>')
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="18 18 72 72">'
            f'<rect x="18" y="18" width="72" height="72" fill="{DARK}"/><path fill="{GREEN}" d="{path(version)}"/></svg>')


def web(out):
    out = pathlib.Path(out)
    # (file, size, version, bare): the square only where the platform shapes the icon itself (a maskable icon; iOS,
    # which fills what is see-through)
    icons = [("icon-192.png", 192, "full", True), ("icon-512.png", 512, "full", True),
             ("maskable-512.png", 512, "full", False), ("apple-touch-icon.png", 180, "full", False),
             ("favicon-32.png", 32, "two", True), ("favicon-16.png", 16, "one", True)]
    for name, px, version, bare in icons:
        svg = out / (name + ".svg")
        svg.write_text(icon_svg(version, px, bare))
        subprocess.check_call(["resvg", str(svg), str(out / name)])
        svg.unlink()


LAUNCHER = """<!-- Cratarr's mark: a record with three rings, a 60-degree slice to the right where only the rings remain, inside the
     adaptive icon's safe zone (66 of 108 dp). Green shapes only, every cut a real gap, so the same drawable serves as
     the themed (monochrome) icon. Made by tools/brand/record.py; change it there. -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="108dp"
    android:height="108dp"
    android:viewportWidth="108"
    android:viewportHeight="108">
    <path
        android:fillColor="{green}"
        android:pathData="{path}" />
</vector>
"""

NOTIFICATION = """<!-- Cratarr's mark in notifications ({version}: {why}). The record (radius 24 of the 108 canvas) at radius 10 of 24 dp.
     Made by tools/brand/record.py; change it there. -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="24dp"
    android:height="24dp"
    android:viewportWidth="24"
    android:viewportHeight="24">
    <group
        android:scaleX="{scale}"
        android:scaleY="{scale}"
        android:translateX="{shift}"
        android:translateY="{shift}">
        <path
            android:fillColor="#FFFFFF"
            android:pathData="{path}" />
    </group>
</vector>
"""


def android(res):
    res = pathlib.Path(res)
    (res / "drawable" / "ic_launcher_foreground.xml").write_text(LAUNCHER.format(path=path("full"), green=GREEN))
    scale = 10 / 24
    # 24 dp is 24 px on mdpi screens (the unqualified folder counts as mdpi), 36 on hdpi, 48 and more from xhdpi
    for folder, version, why in [("drawable", "one", "24 px, one ring"), ("drawable-hdpi", "two", "36 px, two rings"),
                                 ("drawable-xhdpi", "full", "48 px and more, three rings")]:
        (res / folder).mkdir(exist_ok=True)
        (res / folder / "media3_notification_small_icon.xml").write_text(
            NOTIFICATION.format(version=version, why=why, scale=f"{scale:.6f}", shift=f"{12 - C * scale:g}", path=path(version)))


if __name__ == "__main__":
    if sys.argv[1:2] == ["--web"]:
        web(sys.argv[2])
    elif sys.argv[1:2] == ["--android"]:
        android(sys.argv[2])
    else:
        print(path(sys.argv[1] if len(sys.argv) > 1 else "full"))
