---
id: hextris
kind: game
title: Hextris
summary: Stack and rotate four-hexagon pieces to complete zigzag rows.
developer: Mark S. Andersson
publisher: Hellosh Software
year: 1998
architectures:
- 68k
- ppc
default_architecture: 68k
launch_enabled: false
category: Puzzle
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
      Exact catalogue archive and executable selector below, through production archive
      expansion, ordinary scheduler and default display/time. The original Not Yet trial
      flow, Command-B, J, I and Space pass five measured pixel checks at 2330 frontend /
      2930 guest ticks with zero exhausted frames. The inspected capture shows score 100,
      moves 2, rotation 1 and slam 1. Separately, unchanged original installer forks pass
      nine completion-message, Quit/Continue-label and Continue-dismissal checks at 2832 /
      3432 ticks. This is bounded native input and installer UI evidence; full levels,
      audio fidelity, browser approval and public launch qualification remain pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5155
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: 317f7ef37bca989b5f08931053408c097fa9b830
    architecture: ppc
    environment: >-
      Same exact catalogue archive and immutable candidate as the current 68K row.
      Accept the original eight-bit display-depth prompt, then choose Not Yet. Command-B,
      J, I and Space pass five measured pixel checks at 2933 frontend / guest ticks,
      zero exhausted frames. The actual capture shows score 100, moves 2, rotation 1 and
      slam 1. Both CPU routes retain the original trial. This is bounded input evidence;
      full levels, audio fidelity, browser pacing/input and public qualification remain
      pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5155
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: c67b094c303187f464a58be2a00b967af366b310
    architecture: 68k
    environment: "Native ci-test combined candidate SHA-256 06c2bde97f27e9e787a52fccfeb69161bb4db19eb968093e9cb7c51d82e1a308, including palette initialization and dialog control repairs #5109/#5114/#5117. Original installer (SHA-256 2d6bf65ffbb528a881f6beec3bb73a5f49bb758c51d0e289186f23209fd25ad3), selector below, ordinary scheduler and default display/time. Dismiss the original trial notice with Not Yet; Command-B starts a game, J moves left, I rotates and Space slams. Fresh input and matched no-gameplay-input replays each pass five measured pixel assertions at 2330 frontend / 2930 guest ticks, zero exhausted frames. Actual input capture shows score 100, moves 2, rotations 1 and slams 1; the control remains at zero. Five further checks verify the restored registration logo, trial text, labels and second field at 1200 / 1800 ticks. These are bounded input checks, not a completed level, sound or full-game qualification. Browser approval pending."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5121
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: c67b094c303187f464a58be2a00b967af366b310
    architecture: ppc
    environment: >-
      Same original archive and immutable native candidate as the 68K record. Accept
      the game's normal eight-bit display-depth prompt, then choose Not Yet at the
      original registration dialog. Command-B, J, I and Space reach score 100 with
      movement, rotation and slam. Fresh input and matched-control replays each pass five
      measured pixel assertions at 2933 frontend / guest ticks, zero exhausted frames;
      actual captures inspected. A scoped register trace confirms that differing border
      artwork comes from the game's random texture selection. The original style-2 picture
      sheet contains its speckled artwork; cross-CPU screenshot differences alone are
      not a rendering oracle. Full levels, sound, browser pacing/input and public
      qualification remain pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5121
runtime:
  executable_path: Hextris Installer/Hextris 1.0 ƒ/Hextris
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: db29b2e7753f0eda79146525c15445953d0780f3f93a87aa7a2a646ef4190fe3
    size_bytes: 1661103
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/h/
    - https://www.vintageapplemac.com/files/games/Hextris%20Installer.sit
    - https://www.vintageapplemac.com/files/games/Hextris%20Installer.sit.sit
    - https://github.com/benletchford/systemless/issues/5121
    license: Hellosh Software original-package nonprofit distribution terms
    rights_holder: Mark S. Andersson and Hellosh Software
    permission: >-
      The installed Read Me permits nonprofit redistribution without prior written
      notice when the software is unchanged and the complete original installation
      package is included. For-profit distribution requires written permission.
    notes: >-
      Unchanged complete Hextris 1.0 installer, February 1998. Retains the game,
      instructions, music, registration application and distinct duplicate-named icon
      resources. The original 30-day trial and registration requirement remain intact;
      unregistered play is limited to level 3. No standalone game resource, picture, sound
      or music is separately redistributed. Independent unar inspection verifies the
      outer installer forks. Original-installer execution now independently verifies all
      18 payload paths and 36 forks from the corrected VISE parser in #5143; Finder
      flags are recorded separately. The original installer completion message,
      Quit/Continue labels and Continue dismissal now pass nine measured native checks
      with repair candidate #5155.
      The prior blank alert is recorded in #5147; browser approval remains pending.
      Production expansion yields 18 records. The game contains both 68K CODE resources
      and PowerPC PEF/cfrg code. This source adds a StuffIt wrapper around the intact
      installer distribution. Its installer data fork, resource data and resource map
      match the initially tested download; only 24 bytes in the resource-file header area
      differ. Before directory repair #5143, production expansion was byte-identical
      across all 18 records but collapsed eleven space-named sibling paths (KPK2 SHA-256
      1e78c7f6fec506fbf1581c701046d6fb8a23d0691efe82d66998367cd99f6b7c). Fresh replays
      of this exact wrapped download pass five gameplay assertions on each CPU at the
      same clocks recorded above, with zero exhausted frames; final captures are
      byte-identical to the initial source and visually checked.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 82b0e84ba6926f0c06983b663e706ee61f9a4d5674231c197666688fb7f965ab
    size_bytes: 81818
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5121
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Exact 68K verified input replay of the original archive at score 100 after
      moving, rotating and slamming. Cropped from the inspected native 800-by-600 frame to
      the game content rectangle (79,95)-(713,545), with no scaling or generated
      artwork. Excludes the Classic Mac menu bar and desktop.
references:
- https://www.vintageapplemac.com/software/games/h/
---

![Hextris gameplay](https://assets.systemless.org/catalogue/media/sha256/82/82b0e84ba6926f0c06983b663e706ee61f9a4d5674231c197666688fb7f965ab.png)

Arrange four-hexagon pieces to complete the playfield's zigzag rows. Start with
**Command-B**, move with **J/L**, rotate with **I/K**, and slam a piece down with
**Space**. The **Options** menu lets you configure the keys and textures.

This is Hellosh Software's original **Hextris 1.0** shareware distribution.
Choose **Not Yet** at the registration dialog to use the original trial. The
PowerPC version may first ask to switch to an eight-bit colour display.

The complete installer includes the instructions, music and registration
application. Its original 30-day trial terms and level-3 unregistered limit still
apply. Native input checks pass on both CPU paths; browser qualification is
pending, so launch remains disabled.
