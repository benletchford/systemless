---
id: barrack
kind: game
title: Barrack
summary: Fire barriers to clear the arena while avoiding its bouncing balls.
developer: Greg Lovette
publisher: Ambrosia Software
year: 1996
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: d4e39b207e0ab8c6d02478c124f17e0047dd11b8
    architecture: ppc
    environment: >-
      Native optimized candidate combining streamed pictures #5072, zero-base
      store updates #5077 and port picture clipping #5078. Executable SHA-256
      e02385344b48e7951d1cb046acc5eeb5f84c24a1103db37ebcad89e679c268da.
      Untouched original archive, exact executable below, default display and
      fixed default startup time. Dismiss the original trial notice, choose Play
      Barrack, move the gun and fire a barrier near the bottom of the field.
      Fresh replay passes eight measured pixel checks and one non-silent audio
      check at 1632 frontend / 1862 guest ticks, zero exhausted frames. Captures
      inspected: firing clears 9 percent and scores 90 points; the matched
      no-fire control passes its separate check and remains at zero percent.
      This is bounded input and barrier play, not full levels, sustained play,
      settings persistence or audio-fidelity qualification. Original 68K slice
      remains under repair and is not offered here. Browser input/pacing,
      independent inner-installer fork parity and public qualification pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5080
runtime:
  executable_path: Barrack 1.0.4/Barrack 1.0.4 ƒ/Barrack
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Barrack%201.0.4.sit
    expected_sha256: 83e99f16e5d3f368247dbc31c8db8ccbe4ea63e0ab854f0a1a16da83e1c366f3
    expected_size: 3081805
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Barrack%201.0.4.sit
    - https://github.com/benletchford/systemless/issues/5080
    license: Ambrosia Software nonprofit distribution licence
    rights_holder: Greg Lovette and Ambrosia Software, Inc.
    permission: >-
      The original bundled Barrack License expressly permits nonprofit distribution
      without prior written notice when the software is unchanged and the complete
      works are included. Distribution for profit requires written permission.
    notes: >-
      Complete original 1.0.4 StuffIt installer archive, served without alteration.
      Retains the game, registration application, licence, release notes, FAQs,
      web references, icons and media. The original 30-day shareware trial remains
      intact; no registration code or bypass. Production expansion yields nineteen
      file records, including both original Documentation Icon records with different
      resource forks; no duplicate is removed. Independent inner-installer parity
      remains pending. The original fat application contains both 68K and PPC code;
      only the bounded-tested PPC route is offered by this entry.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/barrack/screenshot.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5080
    permission: Original gameplay screenshot captured for this catalogue at the maintainer's request.
    notes: >-
      Exact native PPC verified replay of the unchanged archive, after firing a
      barrier and clearing nine percent of the field. The complete 800-by-600 frame
      is game-owned fullscreen artwork, including its decorative border; no emulator
      framing, host UI or Classic Mac menu bar is present. No generated artwork.
references:
- https://www.vintageapplemac.com/files/games/Barrack%201.0.4.sit
---

![Barrack gameplay](incoming/barrack/screenshot.png)

Move your barrier gun with the mouse and click to fire. Clear **80% of the arena**
to advance while keeping the bouncing balls away from your growing barriers.

This is the original **Barrack 1.0.4** shareware distribution. Choose **Not Yet**
at its original trial notice, then **Play Barrack**. The **Instructions** and
**Setup** buttons explain the other controls and let you configure them.

The complete installer retains its licence, registration program, documentation
and media. Its original 30-day trial terms still apply. Bounded native testing
covers menu entry, movement, a scored barrier and non-silent audio; browser
qualification is pending, so launch remains disabled.
