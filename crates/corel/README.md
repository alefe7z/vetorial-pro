# vectorcraft-corel

A bounded, panic-free reader for CorelDRAW documents (`.cdr`, templates `.cdt`).

CorelDRAW's format is not published. This reader is written from public descriptions of the
format (the RIFF container, the `loda` object records, `trfd` transforms, `fild` and `outl`
styles, the `cmpr` compressed lists, the zipped container of CorelDRAW X4 and later and the
external data streams of X6 and later) and is tested with synthetic files built from scratch
(`synth`). No CorelDRAW software or Corel-authored file is used or committed.

What it reads: pages (master pages skipped) and their layers, groups, rectangles (with round
corners), ellipses, arcs and pies, curves, paths, polygons and stars, transforms, solid fills
(RGB, CMYK, CMY, grey, HSB, HLS, Lab, registration) and outlines (width, colour, caps, joins,
dashes). Text, bitmaps, gradient, pattern and texture fills, lenses and effects are listed in the
warnings (non-solid fills come in as an approximate solid colour). Every file also exposes its
embedded thumbnail, the fallback when the drawing itself can't be read.
