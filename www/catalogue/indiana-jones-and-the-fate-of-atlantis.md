---
id: indiana-jones-and-the-fate-of-atlantis
kind: game
title: Indiana Jones and the Fate of Atlantis
summary: >-
  Talk, travel and trade your way through LucasArts' playable Macintosh
  demonstration of Indy's globe-spanning adventure.
developer: LucasArts Entertainment Company LLC
publisher: LucasArts Entertainment Company LLC
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic run through the original Macintosh demo's animated preview and
      into its interactive balloon-seller scene, cross-checked with the same script under
      BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2425
runtime:
  file_mappings:
    INDYDEMO.000: Atlantis Demo/INDYDEMO.000
    INDYDEMO.001: Atlantis Demo/INDYDEMO.001
    INDYDEMO.002: Atlantis Demo/INDYDEMO.002
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: ccaa0626d2fbcdcd90d1c0021c2707b958a149ded4a62e1dfd5566f88589d6da
    size_bytes: 1529275
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.scummvm.org/demos/
    - https://downloads.scummvm.org/frs/demos/scumm/atlantis-mac-demo.zip
    license: LucasArts Fate of Atlantis promotional demo distribution
    rights_holder: Lucasfilm Games / LucasArts and their successors
    permission: >-
      LucasArts deliberately released this self-contained package as the Macintosh
      demonstration of Indiana Jones and the Fate of Atlantis. It is still distributed as
      such by ScummVM's official demo library. This entry preserves only those demo
      files; it does not include or claim permission for the retail game.
    notes: >-
      Unchanged 1,529,275-byte ZIP with SHA-256
      ccaa0626d2fbcdcd90d1c0021c2707b958a149ded4a62e1dfd5566f88589d6da. This is ScummVM's original download with all seven
      entries intact, including both MacBinary files. At launch, catalogue runtime
      mappings expose the three INDYDEMO data files under the Atlantis Demo directory
      expected by the application without modifying the hosted archive.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 76585a9bc877b1a9968d7f5b382a2f3fe40d4aeb2ad4b65936447ab50c80953b
    size_bytes: 138860
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2425
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request. Underlying Indiana Jones and Fate of Atlantis artwork remains the property
      of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the staged demo archive
      on 2026-09-23 after entering the interactive balloon-seller scene. The complete
      800x600 guest framebuffer is preserved without alteration or host chrome. PNG
      SHA-256 76585a9bc877b1a9968d7f5b382a2f3fe40d4aeb2ad4b65936447ab50c80953b, 138,860
      bytes.
references:
- https://www.scummvm.org/demos/
---

## A ticket to adventure

![Indiana Jones and the Fate of Atlantis demo](https://assets.systemless.org/catalogue/media/sha256/76/76585a9bc877b1a9968d7f5b382a2f3fe40d4aeb2ad4b65936447ab50c80953b.png)

Indy's search for the lost city ranges from university archives to desert
markets, but the Macintosh demonstration starts with a more immediate problem:
a balloon seller, a guarded ticket and a conversation that can go several ways.
The familiar SCUMM verb interface leaves the solution to the player, with
dialogue, inventory objects and the scene itself all open to experimentation.

Before that playable sequence, an animated tour presents the full game's
locations, characters and three distinct paths through the story. It is a
compact period showcase of LucasArts' cinematic pixel art and conversational
adventure design.

## LucasArts' Macintosh demonstration

This is the original promotional demo, not the commercial game. ScummVM's
official demo library identifies it specifically as the Macintosh demo and
continues to provide the original package. The catalogue hosts that package
byte-for-byte and maps its data files into the directory expected by the
original application only inside the guest filesystem at launch.

Systemless decodes the MacBinary application and launches the 68K program,
plays its animated preview and reaches the interactive balloon scene. A
deterministic run selects the scene and advances into conversation, while
BasiliskII independently confirms the same package, timing and interaction.
