---
id: missions-of-the-reliant
kind: game
title: The Missions of Starship Reliant 2.2
summary: >-
  Command a starship crew, defend Alliance planets and battle enemy fleets in
  Michael Rubin's real-time space action game.
developer: Michael Rubin
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 0.102.0 deterministic play runner at d71b3b09
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. It reached the
      main menu, the New Mission selection dialog, mission briefing and the vessel
      enhancement screen. After an enhancement was chosen, the ship was flying in the live
      tactical view. Holding Up and then Left moved and turned the ship relative to the HQ
      station. A matched control script without the keys was identical before the
      input and differed by 1,488 pixels after Up. An exact replay matched all three
      checkpoints, with zero exhausted frames. Sprites, crew portraits and the menu
      background behind dialogs show speckled colour corruption. Combat, saves, audio and
      browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5412
runtime:
  executable_path: Missions 2.2/Missions of the Reliant! 2.2
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 9f2d551bf858eae8203accdd3db2690f152b98970160aae797a0515344fbb5af
    size_bytes: 2223028
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/missions-of-reliant-22.hqx
    license: Original noncommercial complete-package distribution notice
    rights_holder: Michael Rubin
    permission: >-
      The original "How To Register" file permits distributing the game anywhere, as
      long as all original files are included and no charge is made. Selling the game
      or including it in a for-profit collection requires the author's permission.
      Preserve the complete unchanged original archive.
    notes: >-
      The unregistered shareware copy plays the first mission, "Assault on the
      Alliance". The archive has eight files, including the manual application, registration
      notice and data files. The application is 68K only.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4b01a69778df7a10e0d163babc2afb543bc1eabfe46db4e3bd78cb369514188c
    size_bytes: 47810
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - >-
      https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/missions-of-reliant-22.hqx
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: >-
      Full-screen native 68K tactical view after the player turns the ship near the
      HQ station. The game draws over the whole screen; no desktop, menu bar or emulator
      interface is visible.
references:
- https://github.com/benletchford/systemless/issues/5412
- >-
  https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/missions-of-reliant-22.hqx
---

## Take the helm

![Starship Reliant tactical view near the Alliance HQ](https://assets.systemless.org/catalogue/media/sha256/4b/4b01a69778df7a10e0d163babc2afb543bc1eabfe46db4e3bd78cb369514188c.png)

Lead the Reliant and its crew in real-time battles to defend the Interplanetary
Alliance. Give orders to the weapons officer, helmsman, engineer and doctor, and
manage shields, lasers and torpedoes. The unregistered shareware copy includes
the complete first mission.

Choose **New Mission**, then **Begin**. Pick a vessel enhancement to reach the
tactical view. Use the arrow keys to fly, and the numbered crew commands to
give orders.
