---
id: arashi
kind: game
title: Arashi
summary: >-
  Move around a vector arena and fire down its lanes in this Tempest-inspired
  arcade game.
developer: Project STORM
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 7ee2672626e3098f39e9376bcb60d3e99d523947
    architecture: 68k
    environment: >-
      Bounded native v0.95.2 with the SetPort correction in PR 4472, original Arashi
      1.1 archive and exact executable, 800-by-600/8-bit display. Start, level
      selection, active arena and P-key pause inspected. The original pause dialog assigns
      A/D/F to left/right/fire. Fresh keyboard replay passes twelve pixel assertions at
      1898 frontend / 2498 guest ticks with zero exhausted frames. Inspected captures show
      left movement, right reversal and projectiles; a matched run without those
      keyboard inputs retains the crawler position and has no corresponding projectile
      stream. All twenty original forks independently match unar extraction. Browser
      gameplay, full playthroughs, saves and audio pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4471
runtime:
  executable_path: Arashi 1.1/Arashi 1.1
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 40947ddd897d260cfa5ac16ad31494111cdb2707d81337b081e52e5fd2643ad6
    size_bytes: 284885
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Arashi%201.1.sit
    - https://github.com/benletchford/systemless/issues/4471
    license: Original noncommercial redistribution grant
    rights_holder: Project STORM
    permission: >-
      The original June 9, 1993 ReadMe explicitly permits free distribution as long
      as it is not for profit. This free preservation distribution retains the complete
      unchanged archive, documentation and copyright notices. Its request for a
      complimentary CD copy is expressed as a courtesy.
    notes: >-
      Original 284885-byte StuffIt archive, not repacked or patched. Ten original
      files and all twenty data/resource forks match independent unar extraction. The
      application contains CODE resources 0 through 6 and no PPC CFM/PEF slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 1b1b146d10042c730a511da1e780775cc7ac4d13f38619f92ffa039c84e186f2
    size_bytes: 17658
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4471
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Inspected original fullscreen game surface after ordinary keyboard firing,
      without host UI, emulator framing or Classic menu bar. No redraw.
references:
- https://github.com/benletchford/systemless/issues/4471
---

![Arashi vector arena with projectiles after keyboard firing](https://assets.systemless.org/catalogue/media/sha256/1b/1b1b146d10042c730a511da1e780775cc7ac4d13f38619f92ffa039c84e186f2.png)

Start the game and choose a level. Move around the rim and fire down the lanes.
Press P to pause; the original control dialog lets you assign movement and firing
keys. Native testing used A for left, D for right and F for fire.

This is the complete original **Arashi 1.1** package, distributed under its
original permission for free, non-profit distribution. This version is 68K-only.
Browser launch approval is pending.
