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
launch_enabled: true
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
  - date: "2026-10-13"
    tester: Catalogue maintainer
    systemless_version: c103d671e2f52d3004d66f5262bf30eeab0f2f33
    architecture: 68k
    environment: >-
      Optimized staged Chrome Worker/Canvas2D loads the actual managed original,
      no local archive fixture or browser security override. Ordinary held clicks
      close About, place two stones and produce two computer replies, returning
      to Black's Turn. Actual captures inspected. Default 25 MHz and 800x600,
      optimized Wasm SHA-256 4e06918320a7c25a99f5d8dfd2bd75e03fa9d9d662f0f0396602977be61a268d.
      Steady guest pacing is 13.220316576 ticks/sec, below unchanged minimum 50,
      despite 59.999898 host FPS and no console errors. Browser launch remains
      disabled; focused runtime defect #4779 is open. This failed pacing run
      does not qualify browser launch or public gameplay.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/4779
  - date: "2026-10-13"
    tester: Catalogue maintainer
    systemless_version: b28a959975d7193f1c0bc11b03f355c39fa65dea
    architecture: 68k
    environment: >-
      Browser launch approved after two fresh optimized Chrome 151 Worker/Canvas2D
      staging replays of the actual managed original archive, default 25 MHz,
      800x600. Private frontend bridge only; no archive fixture or security override.
      Ordinary held About close and two legal player placements produce computer
      responses and return to Black's Turn. Actual captures inspected. The fresh
      repeat passes eight fixed player-stone checks across the two placements.
      Opponent reply positions vary across fresh starts; a supplementary fixed
      native-opponent-square check failed and is excluded from browser assertions.
      Steady pacing after the replies is 60.054558 / 58.998550 guest ticks/sec,
      above unchanged minimum 50, no console errors. Guarded already-clear
      HiliteMenu no-op and exact idle proof fix #4780 merged after all nine
      exact-head checks passed; tested source 069c7a46 has the same Rust tree
      as merged b28a9599. Four menu tests and all 85 idle-proof tests. Native same eight gameplay checks and
      both complete captures remain unchanged at 1112 frontend / 1706 guest ticks,
      zero exhaustion. Native binary SHA-256
      d4bea97db4e1d77e0456c1c51c98dc31594305714663d429bf2bea1035b28b4e;
      optimized Wasm SHA-256
      77ea550597b8f4966125d12ffa65eba3ec6f50cf80098cdddfcea7ded9a0f5a2.
      Original archive and screenshot managed GETs match pinned bytes/hashes.
      Full games, sliding, learning
      persistence, saves, audio and direct public replay remain unverified.
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

One placement and computer reply are verified natively. Two placements,
computer responses and pacing pass in the staged browser. Full games,
sliding, learning persistence, saves and audio remain unverified.

![Susan original board after a placement and reply](https://assets.systemless.org/catalogue/media/sha256/c4/c4cd13cae56280c6734cf4086992097393154ea84e4c23a714b216f1d31c6adc.png)
