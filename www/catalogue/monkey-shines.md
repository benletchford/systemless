---
id: monkey-shines
kind: game
title: Monkey Shines
summary: Guide Bonzo through haunted rooms, jumping between platforms and collecting keys.
developer: Bonzo Enterprises
publisher: Fantasoft LLC
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 40af948aa013a0959a789a4d4cf6ce0a027f6c5a
    architecture: 68k
    environment: >-
      Bounded native frontend-tick replay of the intact original 13-file 1.1.2 archive,
      exact game selected, default 800-by-600 8-bit display. Original Not Yet dismisses;
      New Game icon opens world selection with registered worlds locked; Spooked loads
      its first room. Right arrow moves Bonzo, up arrow jumps, and enemies animate in
      inspected captures. Repeat passes seven measured pixel assertions at 5906
      frontend ticks with zero exhausted budgets. Browser, room completion, key
      collection, sustained play, saves, audio and editor remain unverified. Original
      PPC slice faults after trial mouse-down and has missing menu artwork,
      tracked independently in issue 4434.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4431
runtime:
  executable_path: Monkey Shines 1.1.2/Monkey Shines
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://github.com/benletchford/systemless/releases/download/catalogue-intake-20261010/monkey-shines-112.sit
    expected_sha256: e41df59a63aed3127f344435c4cc65b48f5212b227c727f427c4d19157dac045
    expected_size: 3684474
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/m/
    - https://www.vintageapplemac.com/files/games/Monkey%20Shines%201.1.2.sit
    - https://github.com/benletchford/systemless/issues/4427
    - https://github.com/benletchford/systemless/releases/tag/catalogue-intake-20261010
    license: Original Monkey Shines unregistered shareware distribution notice
    rights_holder: Fantasoft LLC and Bonzo Enterprises
    permission: >-
      The original README explicitly permits free website and FTP distribution of
      the unregistered version when all original files are included unchanged and
      no download fee is charged. The original 30-day trial and first-world
      restriction are retained. The editor manual, order form and bug-report
      document have also been reviewed.
    notes: >-
      Complete original 13-file distribution: game, five official world files,
      level editor and manual, README, order form, bug-report document, web reference
      and icon. No registration code, changed world, patched executable or repacked
      archive. Original game has both 68K and PPC slices; only 68K is qualified here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/monkey-shines/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4431
    permission: Original Systemless gameplay capture requested by the maintainer.
    notes: >-
      Inspected native 68K capture of Bonzo jumping in the original first Spooked
      room. Unedited 800-by-600 frame contains the centred 640-by-480 game surface;
      no host application or browser interface is included.
references:
- https://www.vintageapplemac.com/software/games/m/
- https://gamefaqs.gamespot.com/mac/679295-monkey-shines
---

![Monkey Shines gameplay](incoming/monkey-shines/gameplay.png)

Play the original **Monkey Shines 1.1.2** shareware trial. Choose **Not Yet**,
click the **New Game icon**, then select **Spooked**. Use the **left and right
arrow keys** to walk and the **up arrow** to jump. Collect red keys to reveal
an exit; blue keys lead to bonus rooms.

This complete, unchanged distribution retains the original 30-day trial and
unregistered first-world restriction. The other worlds and third-party levels
require registration and remain locked. No registration code is supplied.

Bounded native 68K testing covers entering the first room, walking and jumping.
Room completion, key collection, sustained play, saves, audio and the editor
remain unverified. The original PPC slice is not yet qualified.
