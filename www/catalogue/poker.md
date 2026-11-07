---
id: poker
kind: game
title: Poker
summary: Hold selected cards and draw replacements in this five-card-draw game using
  fictional money.
developer: John Stiles
year: 1998
architectures:
- 68k
- ppc
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: Complete unchanged original, exact executable, 68K/default 25 MHz/800x600/depth
      8. Draw deals a hand and fictional cash falls 300 to 250. Click first card to
      mark HELD, then Draw retains three of spades while replacing the other cards.
      Equal-clock omitted-hold control moves the pointer without buttons, then redraws
      all cards; first card becomes four of diamonds. Before-input captures match.
      Fresh repeats each pass eight measured causal pixels plus two exact prior captures,
      ten checks at frontend 492/guest 1092, zero exhausted frames. Captures inspected.
      Own-run consistency is not hardware accuracy. Packaged opening royal flush is
      not a won hand. Native binary SHA-256 35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34.
      Full sessions, payout coverage, persistence, PPC behavior, audio fidelity, browser
      pacing and released public play remain unverified. Launch disabled pending browser
      qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5781
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 4d0e5cac72164346374502adf57f6964b965200b private browser on
      777c6191 runtime
    architecture: 68k
    environment: Fresh isolated Chromium worker/WebGL profiles and SharedArrayBuffer,
      exact executable, 68K/default 25 MHz/800x600/depth 8. Each unchanged 20-second
      sample deals and redraws. Active marks first card HELD and retains king of hearts
      through redraw while remaining cards change. Control omits the hold button and
      replaces its first king of clubs with king of diamonds. Original independently
      generated hands differ between fresh sessions; no same-hand determinism claim.
      Fictional cash is 250 after the 50-unit deal. Captures inspected. Active/control
      steady guest rates 60.1035/59.0872, host rates 60.1035/60.1059 per second, maximum
      worker frames 30.2/43.0 ms, minimum audio queues 130.0/107.3 ms pass all unchanged
      pacing gates. Game ID, worker, architecture, 25 MHz, source stamp, isolation
      and shared memory checked. Actual archive and main/worker Wasm HTTP 200 bodies
      match independent pins. Wasm 17868872 bytes, SHA-256 c4a564f13297a340022045081f9dc110a1bbb540c7cf1c5c996d0309a284689d.
      No lifecycle/network errors. Rates are steady snapshots, not full-session averages.
      Full sessions, payout coverage, persistence, PPC behavior, audio fidelity and
      released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5781
runtime:
  executable_path: Poker 3.0/Poker PPC
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: af230a3c152c3434a763ec569b983866ee210ac6f3db2d8a4991984c737e416f
    size_bytes: 48629
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/d/sw/games/poker/c/poker.sit.hqx
    - https://www.gryphel.com/c/sw/games/poker/
    - https://github.com/benletchford/systemless/issues/5781
    license: Original free-distribution shareware terms
    rights_holder: John Stiles
    permission: The complete original readme permits free copying and distribution,
      prohibits charging, and restricts paid disk/CD collections and commercial bundles
      without written consent. Preserve complete original, shareware request, notices
      and restrictions. Fee-free Internet hosting is not a paid collection or commercial
      bundle. Original terms take precedence over the host public-domain label.
    notes: Full 2168-byte original PICT readme reviewed. Independent extraction matches
      two paths, four forks and two Finder records, strict KPK end 61894, pack SHA-256
      e157221d6bbad7f07855e37abc27b800afa399faaa9aabc5cba189a72ca80bbb. Published
      MD5 matches. Fat 68K/PPC executable; only 68K qualified natively. No external
      runtime supplied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4f5b303343fe069936b06b030df28f50cb08354a2f5f1cf7079a94c4439ecc35
    size_bytes: 2237
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5781
    rights_holder: John Stiles
    permission: Fresh gameplay capture from the unchanged freely distributable original.
    notes: Exact crop (240,261)-(559,376), 319x115, excluding Classic menu/window
      chrome, no resampling. Shows held first card and replacement hand after ordinary
      input.
references:
- https://www.gryphel.com/c/sw/games/poker/
- https://github.com/benletchford/systemless/issues/5781
launch_enabled: true
---

![Poker hand after holding the first card and drawing replacements](https://assets.systemless.org/catalogue/media/sha256/4f/4f5b303343fe069936b06b030df28f50cb08354a2f5f1cf7079a94c4439ecc35.png)

Click Draw to deal a hand. Click cards you want to keep so they show HELD, then click Draw again to replace the other cards. The cash and bet shown are fictional game counters.
