---
id: hoyle-card-games-demo
kind: game
title: Hoyle Card Games Demo
summary: >-
  Play Gin Rummy against Sierra's animated opponents in this original Macintosh
  card-game collection demo.
developer: Sierra On-Line
publisher: Sierra On-Line
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: f8c23613 native-equivalent runtime
    architecture: ppc
    environment: >-
      Complete unchanged demo with original guest defaults. Start Gin Rummy,
      Control-click the nine of hearts upcard to draw, then Control-click the ace of hearts in
      hand to discard. Harley responds with the queen of diamonds on the discard pile.
      The omitted-input control retains the original hand and nine of hearts upcard.
      Both pass six pixel assertions at 1267 frontend and 1235 guest ticks with zero
      exhausted frames. RGB-normalized pre-input captures match exactly; post-input
      captures differ at 7126 pixels. Actual captures inspected. This verifies one bounded
      native turn. Memory Match displays the original full-version-only message and
      remains unavailable. Whole hands, other games, persistence, audio, browser pacing and
      independent hardware accuracy remain unverified. Frozen binary adcaa1a4 uses
      source 01638c43 with the runtime tree merged in f8c23613.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6063
runtime:
  executable_path: Hoyle Card Games Demo/Hoyle Card Games Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: d38e0ec4818c76dc921e52acf790a8b842ba22779df6f54a4f133a15550ac648
    size_bytes: 15466248
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/hoyle-card-games
    - https://archive.org/details/macworld-uk-cd-august-1999
    - https://github.com/benletchford/systemless/issues/6063
    license: Maintainer-accepted promotional demo
    rights_holder: Sierra On-Line and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in reviewed notices. Preserve the complete
      archive, documentation, notices and demo limits.
    notes: >-
      All 9 core file paths and data and resource forks match the August 1999
      Macworld UK disc installer, including all 70 resource payloads, names, IDs and
      attributes. File types and creators match; Finder flags differ. The received SIT wrapper
      is preserved unchanged and is not claimed period-byte-identical. Independent unar
      extraction matches production packing on 9 files, 18 forks and 9 Finder records.
      The complete 116-page PDF help text was extracted and notice-term matches
      reviewed; the image-only cover and final page were inspected. The linked mirror README
      is byte-identical to bundled help. All 32 selected received UI/text/version
      records and 61 period installer records were reviewed; none of these selected records
      is compressed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 629238dd7480fc53aa965b73f62b4aa7a3bdc479ae2452ea8c2bb79a48236ee4
    size_bytes: 426471
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6063
    rights_holder: Sierra On-Line and original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted demo.
      Underlying artwork retains its original copyright.
    notes: >-
      Unscaled 640x480 game content cropped from the verified native Gin Rummy turn.
      Excludes the Mac menu bar, desktop and window framing.
references:
- https://classicmacdemos.com/hoyle-card-games
- https://github.com/benletchford/systemless/issues/6063
---

![Gin Rummy after a player turn and Harley's response](https://assets.systemless.org/catalogue/media/sha256/62/629238dd7480fc53aa965b73f62b4aa7a3bdc479ae2452ea8c2bb79a48236ee4.png)

Choose Gin Rummy from the Go To menu. Control-click the upcard to take it, then
Control-click a card in your hand to discard it. Build matching sets and runs
while watching what your opponent takes and throws away.

The original demo limits are preserved. Browser qualification is pending.
