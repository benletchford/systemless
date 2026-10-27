---
id: barrack
kind: game
title: Barrack
summary: Fire barriers to clear the arena while avoiding its bouncing balls.
developer: Greg Lovette
publisher: Ambrosia Software
year: 1996
architectures:
- 68k
- ppc
default_architecture: ppc
launch_enabled: false
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: 317f7ef37bca989b5f08931053408c097fa9b830
    architecture: 68k
    environment: >-
      Native ci-test dialog-repair candidate #5155, executable SHA-256
      666aad38bc6fffede6def2ec97aae497817022ff7b84f87b32d91bccde307b9d.
      Exact Tucows catalogue archive and executable selector below, through production
      archive expansion with the directory-depth fix #5149. Original trial dismissal,
      Play Barrack, gun movement and a fired barrier pass ten measured checks at 1632
      frontend / 2232 guest ticks, zero exhausted frames. The inspected capture shows
      score 90, level 1 and eight lives. Separately, unchanged original installer forks
      pass nine completion-message, Quit/Continue-label and Continue-dismissal checks at
      3012 / 3577 ticks, zero exhausted frames. This is bounded native gameplay and
      installer UI evidence; browser pacing/input, audio fidelity and public qualification
      remain pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5155
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: 317f7ef37bca989b5f08931053408c097fa9b830
    architecture: ppc
    environment: >-
      Same exact catalogue archive and immutable candidate as the current 68K row.
      Original trial dismissal, menu entry and mouse-fired barriers clear level 1 and
      advance to level 2 with score 2610 and five lives. Seven measured pixel checks pass
      at 3567 frontend / 3861 guest ticks, zero exhausted frames; the actual capture was
      inspected. This is one bounded level transition, not a full-game or audio-fidelity
      qualification. Browser input/pacing and public qualification remain pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5155
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: d9210e9622c520d48117eeb5819baf262876de0f
    architecture: 68k
    environment: "Native combined candidate SHA-256 7bb073961616428b1f96bbb09dd05bd63533b5fed9d6406e90dd8d693c7b77cd, built from the recorded combined source, including the fade preservation and animation-table synchronization repairs in #5125. Both CPU showcase suites pass against unchanged reference images. Exact managed Tucows archive and selector below, ordinary scheduler and default display/time. Not Yet, Play Barrack, gun movement and a fired barrier pass ten existing measured checks at 1632 frontend / 2232 guest ticks. The matched no-fire control passes two checks at the same clocks. Actual captures show intact menu artwork and score 90 with nine percent cleared; control remains at zero. Both runs have zero exhausted frames. This is bounded gameplay, not a completed 68K level or audio-fidelity test. Browser input/pacing and public qualification remain pending."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5124
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: d9210e9622c520d48117eeb5819baf262876de0f
    architecture: ppc
    environment: >-
      Same exact archive and immutable native candidate as the current 68K row.
      Ordinary trial dismissal, menu entry and mouse-fired barriers clear the first level
      and advance to level 2 with score 2610. Seven fixed measured assertions pass at
      3567 frontend / 3861 guest ticks, zero exhausted frames. Actual level-transition
      captures inspected. This demonstrates one level transition, not a full game,
      sustained pacing or audio fidelity. Browser qualification remains pending. This
      historical run preceded the independent inner-installer parity verification below.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5080
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: d4e39b207e0ab8c6d02478c124f17e0047dd11b8
    architecture: ppc
    environment: "Native optimized candidate combining streamed pictures #5072, zero-base store updates #5077 and port picture clipping #5078. Executable SHA-256 e02385344b48e7951d1cb046acc5eeb5f84c24a1103db37ebcad89e679c268da. Untouched Tucows original archive, exact executable below, default display and fixed default startup time. Dismiss the original trial notice, choose Play Barrack, move the gun and fire a barrier near the bottom of the field. Fresh replay passes eight measured pixel checks and one non-silent audio check plus six exact screenshot comparisons at 1632 frontend / 1862 guest ticks, zero exhausted frames. Captures inspected: firing clears 9 percent and scores 90 points; the matched no-fire control passes its separate check and remains at zero percent. This is bounded input and barrier play, not full levels, sustained play, settings persistence or audio-fidelity qualification. At this historical verification, the 68K slice remained under repair; see the newer 68K record. Browser input/pacing, independent inner-installer fork parity and public qualification pending."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5080
runtime:
  executable_path: Barrack Installer/Barrack 1.0.4 ƒ/Barrack
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 1d02dfb509546b8fc2962823537119c95a835a7621dbdd9abab192c656677974
    size_bytes: 3081907
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://archive.org/details/tucows_205647_Barrack
    - https://archive.org/download/tucows_205647_Barrack/barrack.sit
    - https://www.vintageapplemac.com/files/games/Barrack%201.0.4.sit
    - https://github.com/benletchford/systemless/issues/5080
    license: Ambrosia Software nonprofit distribution licence
    rights_holder: Greg Lovette and Ambrosia Software, Inc.
    permission: >-
      The original bundled Barrack License expressly permits nonprofit distribution
      without prior written notice when the software is unchanged and the complete works
      are included. Distribution for profit requires written permission.
    notes: >-
      Complete original Tucows 1.0.4 StuffIt installer archive, served without
      alteration. Independent unar extraction confirms identical installer data/resource
      forks and FinderInfo to the initially tested Vintage Apple Mac archive. All nineteen
      expanded records, both forks and supported metadata match after removing the
      outer folder name. Fresh native replay passes nine gameplay/audio checks and all six
      full-frame comparisons to that initial source. These comparisons are not an
      independent emulator oracle. Retains the game, registration application, licence,
      release notes, FAQs, web references, icons and media. The original 30-day shareware
      trial remains intact; no registration code or bypass. Production expansion yields
      nineteen distinct paths after directory repair #5149, retaining the separate
      Documentation and Web Site URL folder icons and their original resource forks.
      Independent execution of the original installer with CODE-loading repair #5150
      verifies all nineteen payload paths and thirty-eight forks against corrected
      production extraction, with matching file types and creators. Finder flags
      are recorded separately; no complete Finder-metadata parity claim is made.
      With the word-aligned CODE repair #5154 and dialog-repair candidate #5155,
      the original completion message, Quit/Continue labels and Continue dismissal
      pass nine measured native checks. The original fat application contains both
      68K and PPC code;
      both original slices now have bounded native gameplay evidence.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 91c1da601ed697169c1b7b09757d0a0b2bb35ecbbb900bb6e4df252fd4f88c40
    size_bytes: 1091662
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5080
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Exact native PPC verified replay of the unchanged archive after completing the
      first level, at level 2 and score 2610 with restored pattern artwork. The
      complete 800-by-600 frame is game-owned fullscreen artwork, including its decorative
      border; no emulator framing, host UI or Classic Mac menu bar is present. No
      generated artwork.
references:
- https://www.vintageapplemac.com/files/games/Barrack%201.0.4.sit
---

![Barrack gameplay](https://assets.systemless.org/catalogue/media/sha256/91/91c1da601ed697169c1b7b09757d0a0b2bb35ecbbb900bb6e4df252fd4f88c40.png)

Move your barrier gun with the mouse and click to fire. Clear **80% of the arena**
to advance while keeping the bouncing balls away from your growing barriers.

This is the original **Barrack 1.0.4** shareware distribution. Choose **Not Yet**
at its original trial notice, then **Play Barrack**. The **Instructions** and
**Setup** buttons explain the other controls and let you configure them.

The complete installer retains its licence, registration program, documentation
and media. Its original 30-day trial terms still apply. Bounded native testing
covers movement and scored barriers on both CPU paths, and a first-level clear
and transition to level 2 on PPC. Browser qualification is pending, so launch
remains disabled.
