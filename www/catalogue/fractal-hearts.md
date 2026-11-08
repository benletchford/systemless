---
id: fractal-hearts
kind: game
title: Fractal Hearts
summary: Pass cards, follow suit and avoid points in Fractal Software's classic Hearts.
developer: Fractal Software / Tom Hedges and Mark Zimmer
year: 1987
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: c796d05458731645f2bb63c001776656689d412b native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original version 0.8 and exact Hearts.cpt/Hearts executable,
      explicit 68K, default 25 MHz, 800x600 and depth 8, without a partition override.
      Ordinary player setup uses Expert direct-click input; the original help describes
      Beginner as select-before-playing, not an easier computer opponent. No seed,
      computer strategy or other options are changed. Three visible cards are selected
      and passed left. The computer leads the two of clubs; clicking the visible queen
      of clubs legally follows suit, completes the trick and advances to South Leads.
      Omitted-card control moves the same pointer but retains the queen and the two
      of clubs on the table. Each run passes eight measured causal pixels and five
      exact prior captures, thirteen checks at 624 frontend/1220 guest ticks with zero
      exhaustion. Actual images inspected. Own-run references prove consistency,
      not hardware accuracy. Native binary SHA-256
      7ca0f7195dc8910fe687bf0d470cf45a7e2de94a60a528a964fc5b7600867e03.
      Full games, scoring totals, save/load, sound fidelity, browser pacing and
      released public play remain unverified. Launch stays disabled pending browser
      qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5826
runtime:
  executable_path: Hearts.cpt/Hearts
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mac/info-mac/game/crd/fractal-hearts.hqx
    expected_sha256: 9a90455034daac39b9117cf14556b2bb843713e4f662ff879edb629da2a65e77
    expected_size: 40039
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/crd/fractal-hearts.hqx
    - https://github.com/benletchford/systemless/issues/5826
    license: Public domain as stated in the original About screen
    rights_holder: Fractal Software; retained third-party notices
    permission: >-
      The original About artwork explicitly states Public Domain Hearts Game Program
      and names developers Tom Hedges and Mark Zimmer. Preserve the complete unchanged
      original, all artwork, built-in instructions and bundled LightspeedC library notice.
    notes: >-
      Actual original About screen inspected, identifying version 0.8; game status
      identifies November 1, 1987. The later Info-Mac submission is not the rights grant.
      Independent extraction matches the resource-only application's empty data fork,
      55174-byte resource fork and Finder record. Production path retains the original
      Hearts.cpt container name, which independent unar omits from its output directory.
      Strict KPK end 55219, SHA-256
      83a191983092be64ad4a90bca12746ff3ad01fddce2ad80d04477e33c37e431c.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/fractal-hearts/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5826
    permission: Gameplay capture from the original public-domain program.
    notes: >-
      After the legal club play and completed trick. Original 800x600 native capture
      cropped to (170,31)-(632,465), 462x434, without resampling; excludes the Mac
      menu bar and host interface.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/crd/fractal-hearts.hqx
- https://github.com/benletchford/systemless/issues/5826
---

## Avoid the points

![The hand after a completed trick in Fractal Hearts](incoming/fractal-hearts/gameplay.png)

Enter player names and choose your input mode. Expert plays a card immediately
when clicked; Beginner lets you select before playing. Select three cards and
press Pass when asked. Follow the led suit whenever possible, and try to avoid
hearts and the queen of spades. The Help menu contains the original rules.
