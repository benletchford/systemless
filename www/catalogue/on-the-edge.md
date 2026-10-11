---
id: on-the-edge
kind: game
title: On the Edge
summary: Place face cards around the edge and clear number cards totalling ten.
developer: John Stiles
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: ed9db4de2310b638252f7d5d8f43158896e45598
    architecture: 68k
    environment: >-
      Optimized native replay of the complete unchanged On the Edge 3.1
      BinHex/StuffIt archive, selector On the Edge 3.1/On the Edge, explicit 68K and depth 8.
      Ordinary Command-N starts a new game. Clicking the inner square places the dealt six of
      hearts, draws the next ace of clubs and changes Score 0 to 100. Fresh replay
      passes twelve measured before/after pixel checks at 786 frontend / 1386 guest ticks
      with zero exhausted frames; captures inspected. Both files, all four original
      forks and Finder type/creator/flags match independent unar expansion. This proves
      one placement only. Full matches, card removal, undo, saving, audio and public
      replay remain unverified. Browser qualification is recorded separately below.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4661
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 400a685b6365aa226cb220bf7b8da61d4600bbbb
    architecture: ppc
    environment: >-
      Explicit native PPC probe of the identical original archive halts at frontend
      tick 1 / guest tick 0 before its requested startup checkpoint. PC and LR are zero,
      SP 07F6FFC0, 41206 cycles; only an empty framed window appears. Original
      register and memory-write traces identify zero-extended negative Random results
      producing negative shuffle indexes, overwriting the quit flag. A later signed-result fix is recorded
      below. Actual CODE 0/1/3 and a pwpc PEF/cfrg
      confirm both game slices; slice presence does not establish PPC playability.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/4663
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: ed9db4de2310b638252f7d5d8f43158896e45598
    architecture: ppc
    environment: >-
      Signed Random result fix from merged PR 4665, optimized native build,
      identical original archive and selector, explicit PPC/depth 8. Startup now
      runs 600 frontend / 600 guest ticks with four measured table-pixel assertions
      and zero exhausted frames. Command-N and an inner-square click also finish
      786/786 ticks without the previous halt. Captures inspected. The table appears,
      but dealt-card artwork and score text are incomplete; no PPC gameplay
      qualification is claimed. Further rendering fixes and browser validation remain pending. The 68K slice remains the proposed default.
    status: boots
    evidence: https://github.com/benletchford/systemless/pull/4665
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: b3b61ca5510101a1eace9afb7e497d3a7bba65db
    architecture: 68k
    environment: >-
      Real Chrome 151 preview, WebGL worker, 800x600 guest display and 8-bit depth,
      default 25 MHz. Production-origin frontend interception only; the hosted
      original archive is fetched directly with HTTP 200, 22676 bytes and its
      unchanged SHA-256, cross-origin isolation true. Ordinary File/New Game and
      one placement show six of hearts, next ace of clubs and Score 100. Fresh
      replay passes twelve measured before/after browser pixel checks. Captures
      inspected. The separate pacing gate fails: about 19.33 guest ticks/second
      versus the unchanged 50 minimum, despite about 60 host FPS. This 25 MHz configuration did not qualify launch; the later 8 MHz
      approval is recorded below.
      Full matches, card removal, undo, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4661
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: b3b61ca5510101a1eace9afb7e497d3a7bba65db
    architecture: 68k
    environment: >-
      Rebuilt optimized Chrome 151 preview with the supported 8 MHz catalogue
      setting and local launch switch. WebGL worker, 800x600/depth 8, isolated.
      Ordinary File/New Game deals five of hearts; an inner-square placement
      draws six of spades and changes Score 0 to 100. All twelve unchanged measured
      before/after browser checks pass. Captures inspected. The unchanged pacing
      gate also passes: about 57.98 guest ticks/second against minimum 50, about
      60.01 host FPS, no console errors. Direct untouched hosted archive response
      is HTTP 200, 22676 bytes and the original SHA-256. Hosted content-only native
      screenshot independently matches its 3228-byte size and hash. Browser launch
      approved for this bounded 68K route at 8 MHz. PPC rendering, full matches,
      card removal, undo, saves, audio, human release and public replay remain
      unverified or pending; no PPC gameplay claim follows from this approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4664
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 17dd70029960382d69cbb36ee9f4de277abbf028
    architecture: ppc
    environment: >-
      Experimental current-port synchronization fix in PR 4668, optimized native
      build, identical original archive/selector, explicit PPC/depth 8. Command-N
      and an inner-square placement now render the ace of hearts, next three of
      clubs and Score 100. Fresh replay passes fourteen assertions at 786 frontend
      / 786 guest ticks with zero exhausted frames: twelve measured before/after
      pixels and two words proving QDGlobals selects the actual window port.
      Captures inspected. CopyBits uses the window's local bitmap bounds instead
      of screen coordinates. All five focused SetPort tests pass after the new
      regression fails on the prior code. Native bounded placement is verified;
      fix CI/merge, PPC hosted-browser qualification, full games, removal, undo,
      saves, audio and public replay remain pending. The browser-approved default
      68K route is unchanged.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4668
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 0d61a87001810ba0e7a37634b567f259bbe1cfd1
    architecture: ppc
    environment: >-
      Optimized real Chrome 151 worker/WebGL preview, ordinary PowerPC selector,
      identical untouched hosted archive; response HTTP 200, 22676 bytes and
      original SHA-256 independently checked, no archive interception. Frontend
      preview interception only. File/New Game and inner-square placement show
      ace of hearts, next three of clubs and Score 0 to 100. Fresh replay passes
      twelve measured before/after browser assertions; captures inspected. At
      supported 8 MHz, unchanged pacing gate passes about 60.11 guest ticks/second
      and 60.11 host FPS with no console errors. Saturated PPC instruction counter
      makes displayed MIPS invalid; it is excluded from performance evidence.
      Browser launch approved for this bounded PPC placement. Rendering fix
      PR 4668 merged with all nine checks passing. Full games, removal, undo,
      saves, audio, human release and public replay remain pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4668
runtime:
  runtime_pacing:
    cpu_mhz: 8
  executable_path: On the Edge 3.1/On the Edge
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 666d412e535e9be3d4b994be9ef1e04ec43ed77fa8a0820d7a877a9a4e80bde3
    size_bytes: 22676
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/edge/
    - https://github.com/benletchford/systemless/issues/4661
    license: Original free-distribution shareware grant
    rights_holder: John Stiles
    permission: >-
      The original accompanying PICT readme expressly permits free copying and
      distribution when no charge is made. Sold disk/CD software libraries, for-profit
      collections and commercial bundling require written author consent. This free
      preservation distribution retains the complete original archive, accompanying readme,
      copyright notice and five-dollar shareware request. No sale, commercial-bundling or
      modified-software permission is inferred.
    notes: "Original 22676-byte ontheedge.sit.hqx, not the repackaged HFS image. Production packing preserves both original files and all four forks exactly against independent unar extraction, including Finder type, creator and flags. Application: 15058-byte data fork / 21540-byte resource fork. PICT readme: 2514-byte data fork / empty resource fork. Original vers identifies 3.1; preserved edition dates to 1995. Both 68K CODE and native PPC PEF/cfrg are present. No program, documentation, registration or archive bytes are altered."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 379b25620693cd71d32c8bd3289cf1f4fbb19b93973df969c30276c67d46d724
    size_bytes: 3228
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4661
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Content-only crop from the inspected native 68K assertion replay after one
      legal placement, score 100. Excludes desktop, menu bar and window title/frame. The
      original card-table artwork is retained; no filters or repainting.
---

![On the Edge after placing the six of hearts](https://assets.systemless.org/catalogue/media/sha256/37/379b25620693cd71d32c8bd3289cf1f4fbb19b93973df969c30276c67d46d724.png)

Use **File → New game** or **Command-N** to start. Place Kings, Queens and Jacks
in their labelled edge squares. Number cards may occupy the inner squares or
available edge squares. When the grid is full, remove combinations totalling ten.
Fill every face-card position to win; drawing an unplaceable face card ends the game.

This is the complete original **On the Edge 3.1** shareware package. Its original
readme and five-dollar shareware request remain intact. The default edition uses 68K.
