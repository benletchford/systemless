---
id: star-wars-dark-forces
kind: game
title: "Star Wars: Dark Forces"
summary: >-
  Infiltrate an Imperial base in LucasArts' playable one-mission Macintosh
  demonstration.
developer: LucasArts
publisher: LucasArts
year: 1995
architectures:
- 68k
default_architecture: 68k
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-22"
    tester: Catalogue maintainer
    systemless_version: "0.48.0"
    architecture: 68k
    environment: >-
      Deterministic mission run from the unchanged original StuffIt demo archive,
      cross-checked through the same script under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2406
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 2a8f45911fb64904f3e56bd48a8a090ae03efec12aba16ddfd32e48c8891f637
    size_bytes: 3505677
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/star-wars-dark-forces
    - https://static.classicmacdemos.com/demos/star-wars-dark-forces/README.txt
    - https://www.starwars.com/news/dark-forces-remaster
    - https://www.lucasfilm.com/what-we-do/games/
    license: LucasArts Star Wars Dark Forces promotional demo distribution
    rights_holder: Lucasfilm Ltd.
    permission: >-
      LucasArts deliberately released this self-contained package as the Dark Forces
      demo. Its bundled Read Me describes it as a demonstration version with one
      playable mission, contrasts it with the 14-mission retail game and provides ordering
      information. That documented promotional distribution supports preservation of this
      exact unchanged demo; it does not extend to retail media, modified copies or
      modern releases.
    notes: >-
      The unchanged 3,505,677-byte StuffIt archive has SHA-256
      2a8f45911fb64904f3e56bd48a8a090ae03efec12aba16ddfd32e48c8891f637. It contains the 68K Dark Forces Demo
      application, its single mission, cutscenes, sounds, sprites, textures, weapons
      and LucasArts Read Me. The package identifies itself as version 1.0 from 5 June
      1995 and copyright 1995 LucasArts Entertainment Company. Lucasfilm Games continues
      to publish Star Wars interactive entertainment; the catalogue hosts only this
      historical promotional demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 35bb5733ba1e8d1dc117c4499cde67a0b1c336a61381801f6b8167ea813f571e
    size_bytes: 23859
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2406
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request. Underlying Star Wars artwork remains the property of Lucasfilm Ltd.
    notes: >-
      Fresh deterministic Systemless 0.48.0 capture made from the exact unchanged
      demo archive on 2026-09-22 after entering the playable mission, moving forward and
      firing the blaster. The 800x600 framebuffer was cropped to the 640x480 game
      surface, excluding emulator framing and host UI; no game pixels were altered. PNG
      SHA-256 35bb5733ba1e8d1dc117c4499cde67a0b1c336a61381801f6b8167ea813f571e, 23,859 bytes.
references:
- https://classicmacdemos.com/star-wars-dark-forces
- https://static.classicmacdemos.com/demos/star-wars-dark-forces/README.txt
- https://www.starwars.com/news/dark-forces-remaster
- https://www.lucasfilm.com/what-we-do/games/
---

## Operation Skyhook

![Star Wars: Dark Forces gameplay](https://assets.systemless.org/catalogue/media/sha256/35/35bb5733ba1e8d1dc117c4499cde67a0b1c336a61381801f6b8167ea813f571e.png)

Kyle Katarn enters an Imperial base with a blaster, a mission objective and no
shortage of stormtroopers between him and the exit. The first-person level is
built for exploration as much as combat: lifts, switches, branching corridors
and changes in elevation make the installation feel like a place to infiltrate
rather than a flat shooting gallery.

The Macintosh demo opens with Mon Mothma's Operation Skyhook briefing before
placing the player directly into that live mission. Movement, aiming, weapon
fire, ammunition, health and shields all remain under player control, making
this a playable slice of the game rather than a non-interactive trailer.

## LucasArts' one-mission demonstration

This is LucasArts' original June 1995 Macintosh demo, not the 14-mission retail
release and not the modern remaster. Its bundled Read Me explicitly calls it a
demonstration version, explains that it contains one simple mission and directs
players to retailers or LucasArts to purchase the complete game.

Systemless opens the unchanged StuffIt archive, loads its original data,
accepts the mission briefing and reaches interactive first-person play. A
deterministic test moves Kyle forward and fires the blaster, reducing the ammo
counter from 100 to 99. The same sequence completes under BasiliskII. Browser
review then confirmed the promoted immutable archive reaches the same playable
mission through the public launcher.
