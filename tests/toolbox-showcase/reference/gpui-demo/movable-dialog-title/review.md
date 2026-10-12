# Movable dialog title presentation

The standard movable dialog definition (5) was accepted by GPUI frame clipping,
but rejected by WindowFrameSnapshot::title_layout. The painter therefore skipped
its title strip. Definition 5 now uses the same font0/size12 advances, baseline
and clipping geometry as both guest chrome adapters, reaching the existing
smooth original-source text painter. Custom definitions remain guest-owned.
The movable title does not gain close/zoom controls; existing input still routes
through guest FindWindow/DragWindow paths.

Six targeted checks pass (0.09s): shared layout, active/inactive classic frame
painting, classic dialog structure/placement, and actual PPC title drawing with
visible descender ink and clipping. The separate PPC document/movable title
clipping filter also passes both cases. Existing classic frame checks use empty
titles; they establish frame behavior, not complete glyph appearance. No shared
GPUI composed image, physical interaction, scale matrix or independent Macintosh
oracle is claimed for this variant. All production gates remain open.

An initial new-test build referenced nonexistent chrome fields; that rejected
build is retained. The final check tests the actual ink/zoom-ink representation.
