---
id: yahtzee
kind: game
title: Yahtzee!
summary: >-
  Roll five dice, choose which to reroll and fill your scorepad in classic or
  Triple Yahtzee.
developer: Mikael Sheikh
publisher: Necromancer Software
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-10-13"
    tester: Catalogue maintainer
    systemless_version: 1440d1368794ee60ffcd82043f072438416bbc80
    architecture: 68k
    environment: "Optimized native source tree matches merged 1440d136; immutable binary b2254088fc7aed31eb9e32623a46baa53804ca3a5afd854213c9e79b60dc1a1a. Untouched original 3.2.0 archive, exact selector below, 68K and depth 8. Ordinary New Game, one-player setup, first-die selection and two rerolls reach category selection. Original disabled ICONs show 3+5+1+3+3; scoring Chance commits 15 to Chance, Lower Total and Grand Total, then starts the next turn. All 25 displayed 32x32 faces across five captures independently match their original ICON resource bits (25,600 pixels). Fresh replay passes 872 resource-derived dice and measured score checks at 1620 frontend / 2216 guest ticks with zero exhausted frames. Actual captures inspected. Both original files/four forks and Finder type/creator/flags match independent extraction and production packing. CODE 0/1 and no cfrg establish a 68K-only game. PlotIcon rendering fix #4662 is merged; no original game or random-number patch is supplied. Full matches, Triple Yahtzee, multiplayer, saving, audio, browser and public qualification remain unverified. Browser launch remains held."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4757
runtime:
  executable_path: Yahtzee! v3.2.0/Yahtzee!
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: a6918a4776377cfca37c9be377ccc0c79591fb8927b3a70098a7e400a8147c28
    size_bytes: 25001
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/y/
    - https://www.vintageapplemac.com/files/games/Yahtzee%21%20v3.2.0.sit
    - https://github.com/benletchford/systemless/issues/4757
    license: Original unchanged-package conditional redistribution grant
    rights_holder: Mikael Sheikh and Necromancer Software
    permission: >-
      The original README expressly permits redistribution by any means when it
      accompanies the unchanged application, neither file is altered and charges remain
      within reasonable reproduction costs. Sale, incorporation into other products and
      commercial exploitation require the author's written consent. Preserve the complete
      original package, copyright, credits, optional compensation/source-request notice
      and restrictions.
    notes: >-
      Gratis noncommercial preservation distribution of this original standalone
      archive, separate from emulator code and native packages; no product incorporation,
      commercial use or modification permission is inferred. Original StuffIt 3 archive
      contains the resource-only application and its complete 5818-byte README with
      resource fork. Both files, all four forks, filenames and Finder metadata match
      independent extraction. No native PPC slice exists in this edition. Neither
      application nor README is edited. Original host rejected GitHub's download with HTTP 403; a
      temporary checksum-pinned HTTPS transfer supplies identical bytes for normal CI
      promotion. Original provenance remains above; retire the transfer after managed
      storage and its pinned bytes are verified.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d3aaac88295a62f0698b6c5ab24a94e71e816a59fc15ca973596658d23460663
    size_bytes: 3584
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4757
    permission: >-
      Actual Systemless gameplay captured as catalogue evidence at the maintainer's
      request; no standalone game-art extraction or reuse.
    notes: "Original scorepad and dice content surfaces after the first reroll, before scoring Chance: 3+5+1+3+3 total 15. Crop excludes the Classic menu bar, window title bars, outer desktop margins, emulator framing and host UI."
references:
- https://www.vintageapplemac.com/software/games/y/
- https://github.com/benletchford/systemless/issues/4757
- https://github.com/benletchford/systemless/pull/4662
---

Choose **File → New Game** (Command-N), enter a player name, click **Add** and
then **OK**. The original setup also offers **Triple Yahtzee**.

Click dice to select them for a reroll, then click **Roll**. After up to two
rerolls, click an unused category in your player's score column to record the
turn. **Chance** scores the sum of all five dice. The original instructions
remain in the complete download and under **File → Open Help**.

One native scoring turn and original dice rendering are verified. Browser
launch, full games, Triple Yahtzee, multiplayer, saves and audio remain
unverified. This free original package retains its complete redistribution
terms and optional author-compensation notice; it is distributed separately
from Systemless binaries.

![Yahtzee original scoring turn](https://assets.systemless.org/catalogue/media/sha256/d3/d3aaac88295a62f0698b6c5ab24a94e71e816a59fc15ca973596658d23460663.png)
