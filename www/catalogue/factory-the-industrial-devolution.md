---
id: factory-the-industrial-devolution
kind: game
title: "Factory: The Industrial Devolution"
summary: >-
  Keep a tottering production line alive while its switches, belts and machines
  turn every workday into a comic exercise in industrial panic.
developer: Patrick Calahan
publisher: RoundHouse Software
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-16
    tester: Catalogue maintainer
    systemless_version: 0.41.4
    architecture: 68k
    environment: Deterministic headless run through the original installer into live play
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/1988
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 019ad79a7ee4cd91e8d1bfbf107532a18f6cfae43852a4b317c68159cba50743
    size_bytes: 764513
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=3820
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/factory-13.hqx
    license: RoundHouse Software shareware
    rights_holder: Patrick Calahan
    permission: >-
      The Factory FAQ included by its author encourages distribution of complete,
      unregistered and unmodified copies, including online distribution.
    notes: >-
      Untouched author-submitted Factory 1.3 BinHex archive retrieved from the
      Info-Mac mirror on 2026-09-16. The 764513-byte file has SHA-256
      019ad79a7ee4cd91e8d1bfbf107532a18f6cfae43852a4b317c68159cba50743. Research found no later Factory
      release or announcement by Patrick Calahan or RoundHouse Software; similarly named
      modern games are unrelated works.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d17faf8b709cffd9f15594ba08a7dbb885f0e0103991d95e94a362fdca1e1f8a
    size_bytes: 46689
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/149
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh unedited framebuffer capture made from the unchanged archive on
      2026-09-16. It shows live play and excludes browser, website, host desktop, emulator
      controls and Classic Mac desktop furniture.
references:
- https://info-mac.org/viewtopic.php?t=3820
- https://store.steampowered.com/app/770820/Factory_Hiro/
- >-
  https://frostedaxe.itch.io/frustration-factory/devlog/924011/announcing-frustration-factory
---

## A working day at the factory

![Factory gameplay](https://assets.systemless.org/catalogue/media/sha256/d1/d17faf8b709cffd9f15594ba08a7dbb885f0e0103991d95e94a362fdca1e1f8a.png)

The boss wants mouthwash. The line offers a maze of belts, vats, switches and
machines that are much happier making a mess. Factory gives you a few quiet
moments to study the assembly guide, then asks you to keep the whole contraption
moving before the day's quota—or your patience—runs out.

Its pleasure is in learning the machinery. A switch thrown at the right moment
sends work towards the next useful station; the same switch neglected for a few
seconds can turn an orderly floor into slapstick. Each product changes the puzzle,
so success feels less like solving a fixed board and more like getting to know a
temperamental old workshop.

## The original package

This is Patrick Calahan's complete Factory 1.3 distribution, exactly as submitted
to Info-Mac. Systemless opens its BinHex and StuffIt layers, runs the original
installer, and discovers the installed game without altering the archive.

The included Factory FAQ invited people to pass along complete, unmodified copies
of the shareware release. Players who enjoyed it were asked to register with
RoundHouse Software.
