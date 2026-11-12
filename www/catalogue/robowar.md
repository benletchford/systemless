---
id: robowar
kind: game
title: RoboWar
summary: >-
  Pit programmable robot gladiators against each other in the original Macintosh
  arena.
developer: David Harris and Lucas Dixon
publisher: RoboWar maintainers
year: 1990
architectures:
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: ppc
    environment: >-
      Complete original creator-hosted 4.5.2 package at default machine settings.
      Dismiss the application's bundled-preference update notice and title screen, open
      Robots/Coroner1 through File/Open Robot, then File/Duplicate. Clicking Battle
      starts combat; an equal-clock omitted-click control stays idle. Pre-input RGB images
      match exactly. After 30 ticks the active arena shows two moved robots, Chronons 15
      and Halt, versus stationary robots, Chronons 0 and Battle in the control. The
      arena differs by 1259 pixels. After 210 ticks the active battle has ended at
      Chronons 69 while the control remains at zero. Both runs finish 1112 frontend and 1071
      guest ticks with no exhausted frames. Actual captures and external image
      comparisons were inspected; scripts have zero assertions. No robot program or preferences
      were externally changed. This verifies bounded combat, not robot editing,
      tournaments, persistence, audio fidelity, independent hardware accuracy or browser
      pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6168
runtime:
  executable_path: RoboWar 4.5.2/RoboWar 4.5.2
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 20f38deab4cc16fccb054009376108b50a3040f7fe8dd87b132eb59d7e53e0df
    size_bytes: 2501047
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://robowar.sourceforge.net/RoboWar5/MacIndex.html
    - https://robowar.sourceforge.net/files/RoboWar4.5.2.sit
    - https://github.com/benletchford/systemless/issues/6168
    license: Original freeware terms
    rights_holder: David Harris, Lucas Dixon and original contributors
    permission: >-
      The bundled Instructions state that RoboWar is freeware and individuals may use
      it and give it to friends, with copyright retained. Companies interested in CDs
      or pay sites must contact Lucas Dixon. Preserve the complete original package and
      all notices; no commercial distribution or public-domain claim.
    notes: >-
      Exact complete archive linked by the maintainers as the last released Mac
      version. Production expansion matches independent unar/lsar extraction for all 62
      files, 124 forks and 62 Finder records, including original RoboPrefs, supplied
      robots, tournament material and converter. Main and converter Readmes, current
      Instructions and About notices were reviewed. Older speech resources retain legacy
      shareware wording alongside the newer explicit freeware notice. Four tournament
      text/RTF documents have no distribution-term matches. No third-party robot source, game
      instructions or opaque bundles were reverse engineered. This is scoped acceptance
      of the unchanged freeware package, not a grant of rights over separate artwork,
      robots or source code.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 05c8cc87dbe6b8a0753b2542dc09481ab4e97bcdf2508153565d27a3bb29515a
    size_bytes: 4138
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6168
    rights_holder: David Harris, Lucas Dixon and original contributors
    permission: >-
      Real content-only capture of normal play from the accepted original freeware
      package.
    notes: >-
      Unscaled 500x301 crop of the native battle at tick 30, excluding desktop,
      window frame and Classic menu bar. Original pixels are unretouched.
references:
- https://robowar.sourceforge.net/RoboWar5/MacIndex.html
- https://github.com/benletchford/systemless/issues/6168
---

Choose prebuilt robots and watch them battle, or design your own robot gladiators
in **RoboWar 4.5.2**.

Dismiss the startup notice and title screen. Choose **File → Open Robot**, open
**Robots → Coroner1**, then choose **File → Duplicate** and click **Battle**.
The original package includes more robots and instructions in the Apple menu.
Browser play is pending qualification.

![Two robot gladiators fighting in the arena](https://assets.systemless.org/catalogue/media/sha256/05/05c8cc87dbe6b8a0753b2542dc09481ab4e97bcdf2508153565d27a3bb29515a.png)
