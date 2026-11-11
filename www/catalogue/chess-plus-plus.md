---
id: chess-plus-plus
kind: game
title: Chess++
summary: >-
  Play Steve Bushell's original Macintosh chess game against its built-in
  opponent.
developer: Steve Bushell
year: 1993
architectures:
- 68k
default_architecture: 68k
launch_enabled: false
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1858f837 native baseline; browser qualification pending
    architecture: 68k
    environment: >-
      Complete unchanged original archive; exact Chess++.cpt/Chess++ executable.
      68K/default25MHz/default800x6008-bit/original partition; original difficulty,sound
      and other settings retained. File → New Game, then drag e2–e4. Computer replies
      e7–e5. Equal-frontend-clock control omits only the drag button edges and retains the
      starting board. Both repeats pass four causal square pixels and six exact prior
      captures at618 frontend ticks, guest TickCount1188active/1196control,zero
      exhausted frames. Menu/drag tracking changes guest clock; no timing normalization.
      Actual captures inspected. Own references establish consistency,not hardware accuracy.
      Native binary
      SHA-256148f46937353de4e3b322b2c14595204579a07b288754c853003bbc0b4a47cc1. Full games,other modes,persistence,audio fidelity and released public play
      unverified. Launch disabled pending actual hosted-original browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5864
runtime:
  executable_path: Chess++.cpt/Chess++
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: eea984e7ef13011618948cf333aee9e7200547a111faa6769a61817e607e6282
    size_bytes: 323157
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/brd/chess-pp.hqx
    - https://github.com/benletchford/systemless/issues/5864
    license: Original unchanged-distribution grant
    rights_holder: Steve Bushell
    permission: "The complete original readme expressly permits unchanged distribution of the program and/or its source with credit to Steve Bushell, for enjoyment and education. Retain all original notices and restrictions: no sale other than normal connection fees, and no modified reposting without express permission. This is the complete unchanged original free online archive, with author credit; no paid bundle or game modification."
    notes: >-
      Full original readme reviewed; version3.0.1 dated5May1993,copyright1992–1993.
      Complete two-file package preserves executable and original readme. Strict
      verification matches two paths,four forks and Finder records after mapping exactly the
      outer Chess++.cpt production prefix and unar output folder, without changing
      original bytes or relaxing fork/Finder/size/EOF checks. Production pack
      411024bytes,SHA-25698eb143d1d7ddc14769a0b24511fd6ee7b67e476afca9f73e35d207cd89acdaa. No runtime
      added or installed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c9f639612b010c1f9f2e986b4918943b46aeaaf2e92f58358b54586e2949f354
    size_bytes: 3949
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5864
    permission: Original gameplay capture of the unchanged freely distributed game.
    notes: >-
      Exact native content crop [44, 68, 300, 324] without resampling; excludes
      desktop,menu bar and window chrome. Human move and computer reply visible.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/brd/chess-pp.hqx
- https://github.com/benletchford/systemless/issues/5864
---

![Original Chess++ gameplay](https://assets.systemless.org/catalogue/media/sha256/c9/c9f639612b010c1f9f2e986b4918943b46aeaaf2e92f58358b54586e2949f354.png)

Drag a white piece to its destination; the computer plays black. Choose File → New Game to start. The original author documents that castling, en passant and saving boards are not implemented. The unchanged original program and documentation are preserved.
