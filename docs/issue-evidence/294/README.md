# Issue 294 evidence

These frames show Abuse after returning from its attract sequence to the stable title screen.

- `abuse-systemless.png`: Systemless presents an 800×600 drawing surface, so the game anchors its controls at the 800-pixel right edge and uses the larger playfield.
- `abuse-reference.png`: the reference runtime presents the game's expected 640×480 drawing surface at the top left of the same 800×600 capture.

The assets retain the same native pixel scale; the difference comes from the logical display mode offered to the application rather than post-capture scaling.
