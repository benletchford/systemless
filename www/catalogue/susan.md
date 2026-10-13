---
id: susan
kind: game
title: Susan
summary: Place or slide stones on a hexagonal board and surround an opponent's stone.
developer: Stephen Linhart
publisher: Enigma Games
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-10-13"
    tester: Catalogue maintainer
    systemless_version: c103d671e2f52d3004d66f5262bf30eeab0f2f33
    architecture: 68k
    environment: >-
      Immutable native binary SHA-256
      3e82b39ce4c7dff452ac8f773be3572c913c904c684d738fdd44bf399f75099f, source d469bf84 with the merged bitmap-proxy implementation.
      Untouched author Susan 2.5 archive and exact executable below, default display,
      68K. Close the original About window, place a stone in the central row and observe
      the computer's upper-row reply and return to Black's Turn. Fresh replay passes
      eight measured stone/reply pixel assertions at 1112 frontend / 1706 guest ticks,
      zero exhausted frames. Captures inspected. All eight original files/sixteen forks
      and supported Finder metadata match independent extraction and production packing.
      Three CODE resources and no cfrg establish 68K only. Full games, sliding,
      learning persistence, optional art selection, saves, audio, browser and public
      qualification remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4774
runtime:
  executable_path: SUSAN™ 2.5 ƒ/SUSAN™ 2.5
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: cc62394e6c6cc8bbedf8c90bd998e7bcd6fcb73c57926dbedec59cb9fffda659
    size_bytes: 472838
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://slinhart.com/sue/suemac.html
    - https://slinhart.com/archive.html
    - https://slinhart.com/sue/SUSAN_Pack.sit
    - https://github.com/benletchford/systemless/issues/4774
    license: Original complete-archive free redistribution grant
    rights_holder: Stephen Linhart
    permission: >-
      The original Pack Read Me expressly permits free distribution of this complete
      archive when every file is included. It prohibits modifications to the program or
      associated files and prohibits charges except shipping, handling and media
      costs. Retain the complete original package and notices.
    notes: >-
      Free separate preservation download, unchanged and not incorporated into
      emulator binaries. Includes application, original notice, manual, Smart File, both
      optional art packs and both Finder icons. No source-code grant inferred. All eight
      files/sixteen forks and supported Finder metadata independently match production
      packing; KPK2 is 706548 bytes. No native PPC slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c4cd13cae56280c6734cf4086992097393154ea84e4c23a714b216f1d31c6adc
    size_bytes: 203107
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4774
    permission: >-
      Actual Systemless gameplay captured as catalogue evidence at the maintainer's
      request.
    notes: >-
      Original board after the player's central-row placement and computer's
      upper-row reply. Crop excludes the Classic menu bar, window framing, desktop margins,
      emulator framing and host UI.
references:
- https://slinhart.com/sue/suemac.html
- https://github.com/benletchford/systemless/issues/4774
---

Close the introductory **About Susan** window using its top-left close box.
Click an empty space to place a stone. The computer replies automatically.
Surround an opponent's stone to win. The complete original manual explains
sliding, draws and strategy; both optional art sets and the trained intelligence
file remain in the unchanged free download.

One placement and computer reply are verified natively. Browser approval,
full games, sliding, learning persistence, saves and audio remain pending.

![Susan original board after a placement and reply](https://assets.systemless.org/catalogue/media/sha256/c4/c4cd13cae56280c6734cf4086992097393154ea84e4c23a714b216f1d31c6adc.png)
