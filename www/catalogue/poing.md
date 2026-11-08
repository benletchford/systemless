---
id: poing
kind: game
title: Poing!
summary: Place angled bumpers to redirect a moving ball into targets before time runs out.
developer: Rick Holzgrafe
publisher: Semicolon Software
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: d02a603f native; tree identical to merged c2798af5 runtime
    architecture: 68k
    environment: >-
      Complete unchanged original package and exact Poing! 1.1/Poing! executable,
      explicit 68K, default 25 MHz/800x600/depth 8 and original partition. Actual
      preferences confirm Sound/Music on and Slow Ball off. New Game and Begin lead
      through the original start countdown; a bounded 240-guest-tick wait shows the
      moving ball, bell target, original five targets and one-minute limit. Slash
      (key 44/char 47) and backslash (key 42/char 92) create both bumper types and
      redirect the ball versus an equal-clock omitted-key control. Each fresh repeat
      passes eight measured causal pixels and seven exact prior screenshots at
      frontend/guest tick 740/1337, with zero exhaustion. Actual qualified images
      inspected; own-run references establish consistency, not hardware accuracy.
      Native binary SHA-256 d961445d3cbcee316a041419a75a24eaaaf6d0be22bf33ec3d8a4e1a8ac73b5e.
      Earlier blank-field probes were within the original countdown; issue #5839 was
      corrected and closed without a source or game change. Diagnostic optimization
      overrides are not qualification evidence. Both qualified runs still show five
      targets and score zero; scoring, level completion, persistence, audio fidelity,
      browser pacing and released public play remain unverified. Launch stays disabled
      pending browser qualification. Runtime captures remain private evidence.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5840
runtime:
  executable_path: Poing! 1.1/Poing!
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mac/info-mac/game/arc/poing-11.hqx
    expected_sha256: 20dcaa3079d2fa4e237ee79f52d50adee57f67568aa8667c45e832e83eefce63
    expected_size: 569951
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/poing-11.hqx
    - https://github.com/benletchford/systemless/issues/5840
    license: Original complete unchanged nonsale-distribution grant; shareware evaluation terms
    rights_holder: Semicolon Software; Rick Holzgrafe; retained licensed music contributors
    permission: >-
      The complete original Author and Copyright chapter explicitly permits free copying
      and distribution of the entire unchanged Poing! package without selling it.
      Preserve the original two-to-four-week evaluation and registration terms, all
      six files, documentation, levels, artwork, music and notices. The guide prohibits
      separate artwork/music use; publish no extracted media or public thumbnail.
      Licensed music remains inside the complete original package for normal play.
    notes: >-
      Original ReadMe identifies version 1.1, October 25, 1997. Full 1816-byte ReadMe,
      1110-byte registration document, 1275-byte Author and Copyright chapter, credits,
      installation and relevant play/preference chapters reviewed. Music credits retain
      John Scott/MacSourcery and classical selections. Independent production packing
      matches all six files, twelve forks and six Finder records. Strict KPK end
      865887, SHA-256 5b2114d068fb1e69e9be2290eaf1eaada00ba0f21b399e93a9a84b5a58cbf0ec.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/poing-11.hqx
- https://github.com/benletchford/systemless/issues/5840
---

## Redirect the ball

Choose New Game, then Begin, and allow the original countdown to finish. Press
the slash and backslash keys to place angled bumpers ahead of the ball. Use Space
to erase a bumper just before the ball reaches it. Hit the targets before time
runs out.

Poing! is shareware. Its complete original guide explains the evaluation and
registration terms and accompanies the game download.
