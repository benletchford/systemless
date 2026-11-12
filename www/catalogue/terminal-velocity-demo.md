---
id: terminal-velocity-demo
kind: game
title: Terminal Velocity Demo 1.0.1a
summary: >-
  Fly over Ymir and fire the PAC in Terminal Reality's original Macintosh
  combat-flight demo.
developer: Terminal Reality
publisher: MacSoft
year: 1996
architectures:
- ppc
default_architecture: ppc
category: Simulation
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: a3037e0a native
    architecture: ppc
    environment: >-
      Complete unchanged original SIT at default machine settings. Use the game's
      Switch-to-256-colors button and Game/New Game, enter a pilot name and retain the
      original Easy difficulty and Keyboard controls. Escape advances the story, planet
      and craft briefings; the green mission button enters first-person flight. Hold the
      configured Spacebar fire key for 60 ticks. Matched pre-input RGB frames are
      identical. At the final checkpoint, the observed projectile region contains 110 bright
      PAC projectile pixels versus zero in the omitted-Spacebar control. Both runs
      finish 1550 frontend and 1529 guest ticks with zero exhausted frames. Scripts have
      zero assertions; external image predicates and actual captures were checked. A
      mouse-button attempt is excluded because its frames match the control; the original
      Controls/Keys dialog identifies Spacebar as Shoot. No game files, preferences,
      CPU, memory or display-size settings were changed. This verifies bounded
      first-person flight and PAC firing, not later missions, persistence, audio fidelity,
      independent hardware accuracy or browser input/pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6147
runtime:
  executable_path: Terminal Velocity (PPC)/Terminal Velocity Demo (PPC)
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: de94a2f8ded17d3c1e8e7cadd178b8100af222da8d4bafe30885e1b10f204d2a
    size_bytes: 4098279
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/terminal-velocity
    - https://classicmacdemos.com/download/terminal-velocity/
    - https://archive.org/details/macformat-053
    - https://github.com/benletchford/systemless/issues/6147
    license: Maintainer-accepted promotional demo
    rights_holder: Terminal Reality, MacSoft and original contributors
    permission: >-
      Accepted under the existing project policy for complete, unchanged original
      promotional demos preserved from period distribution. This records project-policy
      acceptance, not an affirmative publisher or third-party licence grant. No
      applicable explicit redistribution restriction was identified in the reviewed notices.
      Preserve all original files, credits, notices and demo limits.
    notes: >-
      All twenty original files are retained. Independent comparison with the
      checksum-verified MacFormat 53 August 1997 CD matches all twenty data forks and all
      resource payloads, names, IDs and attributes. Seventeen raw resource forks match;
      three resource-map wrappers differ, so whole-wrapper identity is not claimed. The
      archive contains no separate Readme. All 77 selected standard UI/text/version
      resources were readable, with zero compressed unread records; title, new-game,
      controls/key configuration, briefing and flight screens were inspected. PICT artwork was
      not decoded for text. Production packing independently matches twenty files,
      forty forks and twenty Finder records against unar/lsar. Opaque game data, scripts
      and guest instructions were not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 81a3fa88661b9eddc90877a542b1f169ded6c9143024969041626ddc10502417
    size_bytes: 195706
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6147
    rights_holder: Terminal Reality, MacSoft and original contributors
    permission: Fresh content-only capture of the project-policy-accepted original demo.
    notes: >-
      Unscaled 640x480 crop of actual first-person flight while firing the PAC from
      the complete unchanged demo. Excludes desktop and Classic menu bar. Original
      pixels are unretouched; no broader artwork licence is claimed.
references:
- https://classicmacdemos.com/terminal-velocity
- https://archive.org/details/macformat-053
- https://github.com/benletchford/systemless/issues/6147
---

Fly over **Ymir** in this original Macintosh **Terminal Velocity** demo.

Choose **Switch** at the color prompt, then **Game → New Game**. Enter a pilot name
and confirm. **Escape** advances the briefing screens; click the **green mission
button** when the map appears. The original controls use the keyboard, with
**Spacebar** to fire the PAC. Native flight and firing are verified; browser play
remains pending.

![Firing the PAC over Ymir](https://assets.systemless.org/catalogue/media/sha256/81/81a3fa88661b9eddc90877a542b1f169ded6c9143024969041626ddc10502417.png)
