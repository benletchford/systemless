# Catalogue qualification progress

Tracking issue: [#4353](https://github.com/benletchford/systemless/issues/4353). Updated 11 October 2026. Scope and discovery links are in [the 1,000-entry target list](CATALOGUE-TARGETS-1000.md).

This ledger tracks all 812 primary intake slots. A stage is complete only when its evidence is linked. `Pending` means unverified; `N/A` requires inspected executable evidence. A promoted archive or merged catalogue draft is not a live qualified game. Production must be checked against the deployed release and catalogue route. The 188-record baseline includes 18 launch-disabled records; its live qualified count has not been independently established. Therefore 812 additions alone do not yet prove the goal achieved.

Stages: **rights** = applicable affirmative archive redistribution terms; **archive** = URL, SHA-256, size and intact contents; **68K/PPC** = slice validation and bounded gameplay; **browser** = Systemless-hosted exact archive, input, screenshot, and approval; **CI** = validation and asset promotion on the submitted revision; **live** = released deployment and route verified. Runtime defects receive focused issues/PRs and stay open until their exact archive is retested.

## Active intake

- **Swoop (additional discovery):** original 1.0.2 installer has an unchanged complete nonprofit distribution grant and preserved 30-day trial. Native 68K starts an active wave, moves and fires with measured/repeated capture evidence after [loader #4400](https://github.com/benletchford/systemless/pull/4400). The original FAQ excludes a PPC-native slice. CI promotion, production validation and public browser gameplay passed. Live in v0.88.0 with original trial, active wave, movement and firing verified; whole release run, all six native packages and checksums have passed. [Evidence](https://github.com/benletchford/systemless/issues/4398).

[Delivery goal](CATALOGUE-GOAL.md): 1,000 distinct qualified games live, with publication and compatibility work advancing together.

**Merged catalogue size: 197 records, with 179 launch-enabled records.** iPuzzle and Kalaha are independently checked live on v0.85.0 after [#3946](https://github.com/benletchford/systemless/pull/3946) and [release #4386](https://github.com/benletchford/systemless/pull/4386). FiveStones is now independently checked live on both 68K and PPC in v0.86.0 after [#3977](https://github.com/benletchford/systemless/pull/3977) and [release #4390](https://github.com/benletchford/systemless/pull/4390). Bubble Trouble [#4392](https://github.com/benletchford/systemless/pull/4392) and Chiral [#4394](https://github.com/benletchford/systemless/pull/4394) are independently checked live in v0.87.0 after [release #4396](https://github.com/benletchford/systemless/pull/4396): Bubble Trouble movement on both slices and Chiral 68K atom placement. Swoop [#4401](https://github.com/benletchford/systemless/pull/4401) is independently verified live on v0.88.0 after [release #4402](https://github.com/benletchford/systemless/pull/4402): original trial dismissal, an active wave, movement and firing from the actual hosted archive. The whole release run passed, including website deployment, all six native packages and checksums. Angband [#4414](https://github.com/benletchford/systemless/pull/4414) is independently checked live on both original ports in v0.89.0 after [release #4411](https://github.com/benletchford/systemless/pull/4411): character creation, descent to 50 feet, movement and ascent back to town. [Public archive/body and gameplay evidence](https://github.com/benletchford/systemless/issues/4405#issuecomment-6099326509). The whole release run passed, including website deployment, all six native packages and checksums. Solitaire Till Dawn [#4416](https://github.com/benletchford/systemless/pull/4416) is independently checked live in v0.90.0 after [release #4417](https://github.com/benletchford/systemless/pull/4417): original trial dismissal, Klondike legal move, revealed card and stock draw from the exact hosted archive. [Public evidence](https://github.com/benletchford/systemless/issues/4415#issuecomment-6099620883). The complete release run passed with all six native packages and checksums. Sigma Chess Lite 4.0 [#4419](https://github.com/benletchford/systemless/pull/4419) is independently checked live in v0.91.0 after [release #4420](https://github.com/benletchford/systemless/pull/4420): e2–e4, d2–d4 and both computer replies update the board and move record from the exact hosted archive. [Public evidence](https://github.com/benletchford/systemless/issues/4418#issuecomment-6099963380). The [complete release run](https://github.com/benletchford/systemless/actions/runs/38069235958) passed with all six native packages and checksum files published. The separate 5.1.3 installer investigation remains open. These are nine independently qualified new live games; full playthroughs are not claimed. The enabled baseline remains unaudited: 179 launch flags do not establish 179 independently qualified games. Previously merged Ares, Monkey Shines and Pac the Man compatibility fixes do not independently qualify more live games.

**Browser progress:** Chiral has inspected atom-placement evidence from the exact hosted archive in the deployed v0.84.1 browser benchmark and a separate normal worker preview. Brief stationary trial/menu clicks work on the public v0.87.0 player. The historical held-click case remains under investigation in [#4373](https://github.com/benletchford/systemless/issues/4373). Production-origin archive fetching works; localhost requires preview asset delivery. iPuzzle and Kalaha now have normal release worker input evidence plus separate production hosted-archive checks. Continue other catalogue additions alongside compatibility fixes.

- **Avara (slot 50):** Avara 1.0.1 expands to 132 files. Its bundled licence permits complete unmodified nonprofit distribution and retains the 30-day trial. The game contains 17 CODE resources and four PPC acceleration modules; PPC preference still starts the 68K route. Player setup and the mission roster are visible, but Game → Start reaches a blue horizon and “no HECTOR available”; gameplay remains unqualified. Original source and a matching 2×1 offscreen picture trace identify missing guest picture bottleneck dispatch as the structural blocker. Track the callback fix and mission/spawn retest in [#4355](https://github.com/benletchford/systemless/issues/4355). Modern MIT source-code permission is not being applied to historical archive contents. See [archive receipts](CATALOGUE-SOURCES.md#avara-expansion-intake). Gameplay, hosting, browser approval and publication remain pending.

**Two new games delivered:** [PR #3946](https://github.com/benletchford/systemless/pull/3946) enables iPuzzle 1.0 and Kalaha 1.1. The normal [v0.85.0 release pipeline](https://github.com/benletchford/systemless/actions/runs/38047040930) passed, published all six native packages and deployed the website. Fresh production-origin browser checks verified both enabled routes, exact archive hashes, and normal worker input. Kalaha is an additional rights-cleared intake outside the 812 primary slots. Full puzzle/match completion, saves and audio remain unverified.

## Primary intake ledger

| Slot | Target | Rights | Archive | 68K | PPC | Browser | CI | Live |
| ---: | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Sid Meier's Civilization II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 2 | Myst | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 3 | Riven | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 4 | Day of the Tentacle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 5 | Monkey Island 2: LeChuck’s Revenge | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 6 | The Secret of Monkey Island | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 7 | The Dig | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 8 | Quake | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 9 | Unreal Tournament | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 10 | Myth: The Fallen Lords | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 11 | Myth II: Soulblighter | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 12 | Age of Empires | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 13 | Total Annihilation | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 14 | Age of Empires II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 15 | Quake II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 16 | Sid Meier's Alpha Centauri | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 17 | The Sims | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 18 | Heroes of Might and Magic III | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 19 | Tomb Raider (Gold demo; retired candidate) | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 20 | Tomb Raider II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 21 | Duke Nukem 3D | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 22 | A-Train | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 23 | King’s Bounty | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 24 | Shufflepuck Café | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 25 | Sid Meier’s Colonization | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 26 | SimFarm | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 27 | The Playroom | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 28 | Uninvited | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 29 | Star Wars: X-Wing | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 30 | Star Wars: Rebel Assault | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 31 | Star Wars Episode I: Racer | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 32 | Star Wars: Galactic Battlegrounds | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 33 | Descent II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 34 | Descent 3 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 35 | Carmageddon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 36 | Shadow Warrior | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 37 | Heretic | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 38 | Dark Castle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 39 | Trinity | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 40 | Railroad Tycoon II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 41 | Caesar III | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 42 | Command & Conquer | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 43 | Aliens versus Predator | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 44 | Oni | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 45 | Rune | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 46 | Bugdom | Original demo identified; full-package grant unverified | [Exact official DMG receipt](CATALOGUE-SOURCES.md#original-udif-loading-compatibility-fix) | Pending | Original DMG reaches trial and textured 3D title; gameplay unqualified | Pending | Pending | Pending |
| 47 | Nanosaur | Original licence restricts distribution; permission pending | [Exact official DMG receipt](CATALOGUE-SOURCES.md#original-udif-loading-compatibility-fix) | Pending | Original DMG reaches trial/title, then black; gameplay unqualified | Pending | Pending | Pending |
| 48 | Cro-Mag Rally | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 49 | Otto Matic | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 50 | Avara | [Conditional nonprofit grant](CATALOGUE-SOURCES.md#avara-expansion-intake); remaining terms pending | [1.0.1 receipt and payload](CATALOGUE-SOURCES.md#avara-expansion-intake) | [Mission/spawn unqualified](https://github.com/benletchford/systemless/issues/4355) | Embedded modules; route unqualified | Pending | Pending | Pending |
| 51 | Ares | [Conditional game grant; auxiliary terms pending](CATALOGUE-SOURCES.md#ares-expansion-intake) | [1.1.1 receipt; 56 files](CATALOGUE-SOURCES.md#ares-expansion-intake) | Pending | [Special-handler fix attempted; coercion import halt](https://github.com/benletchford/systemless/pull/4367) | Pending | Pending | Pending |
| 52 | Barrack | [Complete unchanged nonprofit grant; 30-day trial retained](CATALOGUE-SOURCES.md#barrack-expansion-intake) | [1.0.4 exact receipt, 18 files](CATALOGUE-SOURCES.md#barrack-expansion-intake) | Black startup; unqualified | Trial dismissal; incomplete menu; [callback defect #4403](https://github.com/benletchford/systemless/issues/4403) | Pending | Pending | Pending |
| 53 | Bubble Trouble | [Complete nonprofit original grant](CATALOGUE-SOURCES.md#bubble-trouble-expansion-intake) | SHA-256 / 1,670,970 bytes | Native + browser movement | Native + browser movement; background defect | [Both release-player slices checked](CATALOGUE-SOURCES.md#bubble-trouble-browser-qualification) | [Merged #4392](https://github.com/benletchford/systemless/pull/4392) | [Live v0.87.0, both slices checked](https://systemless.org/bubble-trouble/) |
| 54 | Chiral | [Complete nonprofit original grant](catalogue/chiral.md) | SHA-256 / 1,043,239 bytes | Native + browser atom placement | 68K-only original | [Fresh release-player startup + placement](CATALOGUE-SOURCES.md#chiral-release-player-qualification) | [Merged #4394](https://github.com/benletchford/systemless/pull/4394) | [Live v0.87.0, 68K checked](https://systemless.org/chiral/) |
| 55 | Cythera | [Conditional game grant; auxiliary terms pending](CATALOGUE-SOURCES.md#cythera-expansion-intake) | [1.0.2 receipt; 44 files](CATALOGUE-SOURCES.md#cythera-expansion-intake) | Player creation and archetype; gameplay unqualified | Colour prompt, then PC-zero halt; unqualified | Pending | Pending | Pending |
| 56 | Mars Rising | [Conditional game grant; auxiliary terms pending](CATALOGUE-SOURCES.md#mars-rising-expansion-intake) | [Three exact receipts; installer inspected](CATALOGUE-SOURCES.md#mars-rising-expansion-intake) | Installer succeeds; game PPC-only per README | Original installer-to-game launch pending | Pending | Pending | Pending |
| 57 | Pillars of Garendall | Demo identified; grant unverified | [PPC demo; 52 files](CATALOGUE-SOURCES.md#additional-demo-intake-10-october-2026) | Pending | PC-zero halt at tick 32 | Pending | Pending | Pending |
| 58 | Gubble | Demo identified; auxiliary terms pending | [PPC demo; 24 files](CATALOGUE-SOURCES.md#additional-demo-intake-10-october-2026) | Pending | PC-zero startup halt | Pending | Pending | Pending |
| 59 | MacSki | Decoded mass-distribution restriction; website permission pending | [v1.7 and v1.6 receipts](CATALOGUE-SOURCES.md#macski-expansion-intake) | v1.6 slope starts and scrolls; bounded input | v1.6 read MemoryFault; v1.7/1.7.2 accessor fix reaches integrity warning [#4425](https://github.com/benletchford/systemless/issues/4425); unqualified | Pending | Pending | Pending |
| 60 | Pararena | Demo identified; grant unverified | [2.01 demo; three files](CATALOGUE-SOURCES.md#additional-demo-intake-10-october-2026) | Colour prompt and cropped title; unqualified | Pending | Pending | Pending | Pending |
| 61 | Bonkheads | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 62 | Bonkheads Deluxe | Demo identified; grant unverified | [1.0 demo; four files](CATALOGUE-SOURCES.md#additional-demo-intake-10-october-2026) | Main menu; gameplay unqualified | Pending | Pending | Pending | Pending |
| 63 | Monkey Shines | [Explicit free web trial grant; full review pending](CATALOGUE-SOURCES.md#monkey-shines-expansion-intake) | [1.1.2 receipt; 13 files](CATALOGUE-SOURCES.md#monkey-shines-expansion-intake) | Trial notice; world gameplay unqualified | [Startup fix reaches trial notice; gameplay pending](https://github.com/benletchford/systemless/pull/4370) | Pending | Pending | Pending |
| 64 | Space Cab | Unverified | [Original installer; one file](CATALOGUE-SOURCES.md#additional-demo-intake-10-october-2026) | Installer SysError 15 | Preference selects 68K; no PPC proof | Pending | Pending | Pending |
| 65 | King of Dragon Pass | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 66 | Afterlife | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 67 | Alley 19 Bowling | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 68 | Allied General | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 69 | Close Combat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 70 | Combat Mission | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 71 | Deadlock | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 72 | Imperialism II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 73 | Capitalism | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 74 | MacGo | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 75 | Macgammon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 76 | Solitaire Till Dawn | [Complete unchanged no-sale distribution grant](catalogue/solitaire-till-dawn.md) | [Original publisher 4.0.1, 46 files, pinned hash/size](catalogue/solitaire-till-dawn.md) | Klondike legal move, revealed card and stock draw; four measured assertions | N/A: original has CODE resources and no PPC slice | [Optimized Chrome move and stock draw; hosted hash checked](https://github.com/benletchford/systemless/issues/4415) | [Catalogue #4416](https://github.com/benletchford/systemless/pull/4416); CI promotion passed | [Live v0.90.0; public legal move and stock draw](https://systemless.org/solitaire-till-dawn/) |
| 77 | Mike's Cards | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 78 | Angband | [Complete historical nonprofit copying grant](catalogue/angband.md) | [Two intact original 2.7.8 ports, separately hash-pinned](catalogue/angband.md) | Character creation, town and dungeon movement; four measured native assertions | Character creation, town and dungeon movement; four measured native assertions; text-cell fix merged | [Both public v0.89.0 ports: descent, movement, ascent and exact archive hashes](https://github.com/benletchford/systemless/issues/4405#issuecomment-6099326509) | [Merged #4414](https://github.com/benletchford/systemless/pull/4414); CI promotion and full release passed | [Live v0.89.0, both ports checked](https://systemless.org/angband/) |
| 79 | Moria | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 80 | Rogue | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 81 | MacSokoban | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 82 | Sokoban | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 83 | MacBrickout | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 84 | Pac the Man | Freeware label; explicit hosting grant unverified | [4.0.1e receipt; eight files](CATALOGUE-SOURCES.md#pac-game-expansion-intake) | [Level 1; Right differs from matched idle](CATALOGUE-SOURCES.md#pac-game-expansion-intake) | [Level 1 and directional input after fix; audio unverified](CATALOGUE-SOURCES.md#pac-the-man-global-input-list-compatibility-fix) | Pending | Pending | Pending |
| 85 | PacMac Deluxe | Collection permission and plugin terms unverified | [1.3 receipt; 141 files](CATALOGUE-SOURCES.md#pac-game-expansion-intake) | Fatal Sound Library error -5 | Pending | Pending | Pending | Pending |
| 86 | MacSnake | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 87 | Tetris Max | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 88 | Oxyd | Documentation inspected; hosting grant unverified | [3.7 receipt; 12 files](CATALOGUE-SOURCES.md#oxyd-expansion-intake) | Serial read loop before capture; unqualified | Pending | Pending | Pending | Pending |
| 89 | per-oxyd | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 90 | Lode Runner 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 91 | Mantra | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 92 | Mantra II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 93 | Sapiens | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 94 | Macnopoly | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 95 | Risk! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 96 | Monopoly | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 97 | Clue | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 98 | Game of Life | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 99 | Battleship | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 100 | Yahtzee! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 101 | Shanghai Dynasty | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 102 | Mahjong | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 103 | MacTaipei | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 104 | Pegged | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 105 | Powerball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 106 | Brickles Plus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 107 | Brick Attack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 108 | Bubbles | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 109 | Airburst | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 110 | DX Ball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 111 | 3-D Ultra Pinball: The Lost Continent | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 112 | 3-D Ultra Pinball: Thrillride | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 113 | Pro Pinball: Big Race USA | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 114 | Crystal Caliburn | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 115 | Tristan | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 116 | Royal Flush | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 117 | F/A-18 Korea | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 118 | Chuck Yeager's Air Combat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 119 | X-Plane | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 120 | Apache Longbow | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 121 | Hind | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 122 | Achtung Spitfire! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 123 | Over the Reich | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 124 | IndyCar Racing II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 125 | NASCAR Racing | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 126 | 4x4 Evolution | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 127 | F1 Championship Season 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 128 | Vette! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 129 | Links Pro | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 130 | PGA Tour Golf | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 131 | Mario's Game Gallery | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 132 | The Incredible Machine | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 133 | Logical Journey of the Zoombinis | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 134 | Where in Time Is Carmen Sandiego? | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 135 | The Time Warp of Dr. Brain | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 136 | Aquazone | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 137 | Amber: Journeys Beyond | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 138 | Obsidian | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 139 | Morpheus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 140 | Sensory Overload | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 141 | Prime Target | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 142 | Shattered Steel | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 143 | The Journeyman Project 2: Buried in Time | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 144 | The Journeyman Project 3: Legacy of Time | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 145 | Jewels of the Oracle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 146 | Discworld | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 147 | Conquest of the New World | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 148 | Empire Deluxe | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 149 | Civil War | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 150 | Sub Battle Simulator | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 151 | Worms | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 152 | Tomb Raider Chronicles | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 153 | Tomb Raider III: Adventures of Lara Croft | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 154 | Tomb Raider: The Last Revelation | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 155 | Warcraft III: Reign of Chaos | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 156 | Star Wars: Rebel Assault II: The Hidden Empire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 157 | Wing Commander IV: The Price of Freedom | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 158 | Ultima IV: Quest of the Avatar | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 159 | Gabriel Knight: Sins of the Fathers | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 160 | Close Combat: A Bridge Too Far | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 161 | Combat Mission: Barbarossa to Berlin | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 162 | Populous II: Trials of the Olympian Gods | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 163 | Madden NFL 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 164 | Links LS 1997 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 165 | Links LS 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 166 | Putt-Putt Enters the Race | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 167 | Putt-Putt Goes to the Moon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 168 | Freddi Fish 3: The Case of the Stolen Conch Shell | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 169 | Freddi Fish 4: The Case of the Hogfish Rustlers of Briny Gulch | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 170 | Freddi Fish and the Case of the Missing Kelp Seeds | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 171 | Pajama Sam 2: Thunder and Lightning Aren't So Frightening | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 172 | Fatty Bear's Birthday Surprise | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 173 | Living Books Sampler | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 174 | Living Books: Arthur's Birthday | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 175 | Living Books: Dr. Seuss's ABC | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 176 | Living Books: Harry & the Haunted House | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 177 | Living Books: Just Grandma and Me | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 178 | Living Books: Little Monster at School | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 179 | Living Books: Ruff's Bone | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 180 | Living Books: Sheila Rae, The Brave | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 181 | Living Books: The New Kid on the Block | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 182 | Living Books: The Tortoise and the Hare | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 183 | Gadget: Invention, Travel, & Adventure | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 184 | Gadget: Past as Future | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 185 | I Have No Mouth, and I Must Scream | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 186 | Broken Sword: The Shadow of the Templars | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 187 | Harpoon 3 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 188 | The Battle of Britain II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 189 | 3-D Ultra NASCAR Pinball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 190 | Absolute Zero | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 191 | Active Lancer | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 192 | After Dark Games | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 193 | Al Unser Jr. Arcade Racing | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 194 | Alien Nations | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 195 | Alone in the Dark 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 196 | Alone in the Dark 3: Ghosts in Town | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 197 | AmoebArena | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 198 | AstroRock 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 199 | Backyard Football | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 200 | Bad Milk | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 201 | battle-girl | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 202 | Birdie Shoot | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 203 | Blade | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 204 | Blood Bath at Red Falls | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 205 | Blue's ABC Time Activities | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 206 | Bomber III | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 207 | Brain Pizza | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 208 | Bumbler Bee-Luxe | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 209 | Ceremony of Innocence | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 210 | Championship Manager: Season 00/01 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 211 | Championship Manager: Season 01/02 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 212 | Championship Manager: Season 99/00 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 213 | Chaos Overlords | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 214 | CheckMate | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 215 | Chess Mates | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 216 | Command HQ | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 217 | Crop Circles: Escape from Planet 3 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 218 | Crystal Crazy | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 219 | Curse of Dragor | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 220 | Cyberwar | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 221 | D-Day: America Invades! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 222 | Dark Vengeance | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 223 | Deathground | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 224 | Deer Avenger | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 225 | Deliverance | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 226 | Der Schatz von Tattoom | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 227 | Derrat Sorcerum | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 228 | Deus Ex | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 229 | DinoPark Tycoon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 230 | Doulber | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 231 | Driver - You Are The Wheelman | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 232 | Dust: A Tale of the Wired West | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 233 | Dynasty League Baseball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 234 | Earth 2140 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 235 | Elite Air Hockey | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 236 | Elite Darts | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 237 | Escape from Monkey Island | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 238 | Expert Backgammon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 239 | Flight Commander 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 240 | Flying Circus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 241 | Flying Nightmares | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 242 | Foul Play: Mystery at Awkward Manor | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 243 | Frankenstein: Through the Eyes of the Monster | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 244 | Full Tilt! Pinball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 245 | Fury of the Furries | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 246 | Future Boy! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 247 | Future Cop: LAPD | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 248 | Galapagos: Mendel's Escape | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 249 | Ghosts | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 250 | Goofy Golf Deluxe | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 251 | Harry Potter and the Chamber of Secrets | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 252 | HAVOC | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 253 | HeadRush | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 254 | Heavy Metal: F.A.K.K. 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 255 | Hexen: Beyond Heretic | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 256 | Hoyle Board Games | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 257 | Hoyle Card Games | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 258 | Hoyle Casino 1999 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 259 | Hoyle Casino 5 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 260 | Hoyle Word Games | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 261 | Ice & Fire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 262 | Icebreaker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 263 | Inbred with RedNex | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 264 | Infocom Sampler | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 265 | Intellivision Lives! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 266 | Jan Pienkowski's Haunted House | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 267 | Jazz Jackrabbit 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 268 | Just Me & My Mom | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 269 | Kamikaze Chess | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 270 | Karma: Curse of the 12 Caves | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 271 | Kawasaki ATV PowerSports | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 272 | Kawasaki Jet-Ski Watercraft | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 273 | Kids Arcade Pak | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 274 | Killing Time | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 275 | Krilo | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 276 | Kuba | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 277 | Last Call | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 278 | Locus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 279 | Majesty: The Fantasy Kingdom Sim | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 280 | Manhattan Apartment Hunter | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 281 | Mario Teaches Typing | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 282 | Marty and the Trouble with Cheese | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 283 | Maximum Pool | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 284 | Mia 2: Romaine’s New Hat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 285 | MINDit | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 286 | Multi Pong | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 287 | Muppet Treasure Island | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 288 | Myth III: The Wolf Age | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 289 | NetherWorld | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 290 | Nightfall | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 291 | NOIR: A Shadowy Thriller | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 292 | Nuclear Hammer | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 293 | Onslaught | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 294 | Out of the Sun | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 295 | Pac-In-Time | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 296 | Panic in the Park | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 297 | Pax Imperia | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 298 | Payback | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 299 | PlayMaker Football | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 300 | Postal | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 301 | Power Rangers Zeo Versus The Machine Empire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 302 | PowerMonger | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 303 | PowerPOKER | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 304 | Pro Pinball: The Web | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 305 | PT Boat Simulator | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 306 | Quest for Glory V: Dragon Fire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 307 | Racing Days R | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 308 | realMyst | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 309 | RealPool | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 310 | Redjack: Revenge of the Brethren | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 311 | Remington Top Shot | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 312 | Return of the Incredible Machine: Contraptions | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 313 | Rugrats Adventure Game | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 314 | Safecracker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 315 | Secrets of the Luxor | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 316 | ShadowWraith | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 317 | Sheep | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 318 | Shockwave Assault | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 319 | Shogo: Mobile Armor Division | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 320 | Shomei | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 321 | Sid Meier's Alien Crossfire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 322 | SiN | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 323 | Skull Cracker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 324 | Sky Shadow | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 325 | SkyFighters 1945 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 326 | Space Girl | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 327 | Spaceway 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 328 | speed bump | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 329 | Spins! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 330 | Star Trek: 25th Anniversary | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 331 | Star Trek: Voyager – Elite Force | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 332 | Star Wars Episode I: The Gungan Frontier | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 333 | Star Wars: Pit Droids | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 334 | Star Wars: Yoda's Challenge Activity Center | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 335 | Stay Tooned! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 336 | Strat-o-Matic Baseball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 337 | Super Maze Wars | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 338 | Super Wing Commander | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 339 | System Shock | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 340 | TacOps | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 341 | Tanaka | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 342 | Tempest 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 343 | Terminal Velocity | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 344 | Terminus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 345 | The Alchemist | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 346 | The Castle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 347 | The Forgotten: It Begins | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 348 | The Labyrinth of Time | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 349 | The Settlers II: Veni, Vidi, Vici | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 350 | The Tinies | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 351 | The Untouchable | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 352 | Theme Park | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 353 | Theme Park World | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 354 | Time Gate: Knight's Chase | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 355 | Titanic: Adventure Out of Time | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 356 | Tom Clancy's Ghost Recon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 357 | Tom Clancy's Rainbow Six | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 358 | Tom Clancy's Rainbow Six: Rogue Spear | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 359 | Tony Hawk's Pro Skater 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 360 | Traitors Gate | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 361 | TriBond | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 362 | Trophy Bass | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 363 | Tropico | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 364 | Troubled Souls | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 365 | Tubular Worlds | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 366 | Under a Killing Moon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 367 | Virtual Pool | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 368 | Virtual Wings Pro | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 369 | WaterRace | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 370 | Weekend Warrior | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 371 | Wheels! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 372 | Wingnuts: Temporal Navigator | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 373 | World at War: Operation Crusader | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 374 | Worms Blast | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 375 | Wrath of the Gods | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 376 | You Don't Know Jack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 377 | You Don't Know Jack Movies | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 378 | You Don't Know Jack Sports | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 379 | You Don't Know Jack Television | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 380 | You Don't Know Jack Volume 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 381 | You Don't Know Jack Volume 3 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 382 | Zone Raiders | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 383 | Zone Warrior | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 384 | ZPC | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 385 | Quagmire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 386 | Galactic Empire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 387 | Radical Castle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 388 | A Day at Work | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 389 | S.C. Out | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 390 | U-Boat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 391 | Wagon Train 1848 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 392 | M&M's: The Lost Formulas | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 393 | Oberin | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 394 | Qrax / Prometheus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 395 | Valley of Peril | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 396 | Wacky Jacks | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 397 | I Spy Fantasy | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 398 | Ocean Bound | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 399 | Ultima II: The Revenge of the Enchantress | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 400 | Caesar | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 401 | Sabrina, the Teenage Witch: Brat Attack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 402 | Zap | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 403 | CABOL II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 404 | F-117A Nighthawk Stealth Fighter | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 405 | Galactic Core | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 406 | P51 Mustang Flight Simulator | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 407 | Sabrina, the Teenage Witch: Spellbound | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 408 | Sacrifice | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 409 | U-Boat II: Drumbeat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 410 | B-Room | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 411 | CinC | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 412 | Eagle Strike | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 413 | Yacht-3D | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 414 | Abalone | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 415 | Bakudanjin | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 416 | Candy Crisis | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 417 | DragonMaze | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 418 | ExaChess_Lite | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 419 | Fanorona | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 420 | GL Tron | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 421 | HipHop | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 422 | Imp Fodder | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 423 | Jailbreak | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 424 | Kalah | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 425 | Lunar Commando | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 426 | MacSolitaire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 427 | Nethergate | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 428 | Oilcap Pro | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 429 | Project Magellan | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 430 | Quidditch Practice | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 431 | R.I.P | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 432 | Sigma Chess | Lite 4.0 original-form distribution grant; 5.1.3 installed terms pending | [Two exact archives; 164-file Lite package](https://github.com/benletchford/systemless/issues/4418) | Lite 4.0 two legal moves, computer replies, four measured assertions; 5.1.3 installer resource-hook gap | Lite game has CODE, no PPC slice; PPC generators are auxiliary | Optimized Chrome and public v0.91.0 pass both player moves and replies | [Catalogue #4419](https://github.com/benletchford/systemless/pull/4419); CI promotion passed; managed archive hash/size checked; launch approved | [Live v0.91.0; opening moves and replies verified](https://systemless.org/sigma-chess/) |
| 433 | TakeAway | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 434 | Unprovoked | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 435 | Vampire Chess | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 436 | Wacky Mini Golf | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 437 | X Ball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 438 | YA-Mancala | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 439 | Zap'T'Balls | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 440 | À la recherche du paquet de Choco Krispies | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 441 | Ça se transforme | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 442 | Acquire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 443 | Blood Pong | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 444 | Classic Gin Rummy | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 445 | Domitrix | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 446 | Earl Weaver Baseball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 447 | Farkle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 448 | Gollo Obo | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 449 | Hold'em Poker Lite | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 450 | InsectiSide | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 451 | Jared | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 452 | K'Kai Adventure | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 453 | LandShark | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 454 | Mealy Mouse | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 455 | Net Othello | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 456 | Old Camera Puzzle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 457 | PACA PONG | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 458 | Q*bert | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 459 | Race Car Simulator | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 460 | Sniffer Dog | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 461 | Tangram | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 462 | Ultimate Blackjack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 463 | Valley of the Vampire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 464 | WallRunners | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 465 | Xenos | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 466 | Yacht Race | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 467 | Zap'T'Balls II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 468 | Acorn on the Moon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 469 | Brick Ball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 470 | Colibricks | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 471 | David's BackGammon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 472 | Earth Command | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 473 | FastGun | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 474 | Grid Warrior | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 475 | Halma | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 476 | iConcentration | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 477 | Jetpack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 478 | Kablooey | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 479 | Lites Off | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 480 | Morgana | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 481 | Net Trek | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 482 | OmaMoku | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 483 | Pair Picker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 484 | QuackMan | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 485 | Racing Days | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 486 | Snoop | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 487 | Tony the Turtle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 488 | Ultimate Math Machine | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 489 | Vampire Castle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 490 | Wallup | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 491 | X-Files Trivia Challenge | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 492 | Yalta | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 493 | Zargon Zoo | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 494 | Adventure | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 495 | Balloon Man | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 496 | Canfield | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 497 | Dawn of Aces | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 498 | Eat-o's | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 499 | Final Impact | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 500 | Grump | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 501 | Hang2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 502 | IconPuzzle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 503 | Jewel of Arabia | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 504 | Kaged-The Magic Orbs | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 505 | Lites Out | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 506 | Munchkinstein | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 507 | NetBoxes | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 508 | On the Edge | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 509 | Pajatso | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 510 | Quarry | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 511 | Radiant Weirdness Zone | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 512 | Soi | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 513 | Tablin | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 514 | Ultimate Pool | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 515 | Vanessa Chess | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 516 | War Machines | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 517 | X-Men: The Ravages of Apocalypse | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 518 | YAMS! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 519 | Zark | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 520 | Adventure Island | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 521 | Banshee | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 522 | Cannon ZERO | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 523 | Deer Avenger 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 524 | Ebola Monkey Bingo | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 525 | Firefly Frenzy | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 526 | Galactic Patrol | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 527 | Hangman | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 528 | IconQuest | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 529 | Jewel of Arabia-Dreamers | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 530 | Kai-Jin | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 531 | Lab-Rat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 532 | Musket Fire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 533 | NetKombat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 534 | One eyed Fred | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 535 | Palace Of Sand | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 536 | Quarters | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 537 | Radical Rebound | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 538 | Speed Demon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 539 | Tablut | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 540 | Ultimaze | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 541 | Vic Kombat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 542 | War of Flowers | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 543 | X-Moto | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 544 | Yipe III | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 545 | Zauron | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 546 | Adventures of Sean | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 547 | Barney Blaster | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 548 | Cannons and Castles | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 549 | Demon Warrior | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 550 | Eclipse | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 551 | Fist Fighters | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 552 | Galactic Alliance | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 553 | HangmanPlus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 554 | Infotron | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 555 | Jewelbox | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 556 | Kazcheckers | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 557 | LaBrisca | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 558 | Mac Football | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 559 | NetLuff | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 560 | Ophiuchus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 561 | Pandora's Box | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 562 | Quarto! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 563 | Raptor | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 564 | Strategram | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 565 | Tank Run | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 566 | UltraDice | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 567 | VincoBingo | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 568 | Warbirds | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 569 | Youngsword | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 570 | Zaz | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 571 | Alan's Euchre | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 572 | BarneyCarnage | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 573 | Captain Bumper | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 574 | Dragon Cavern | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 575 | Egyptian Solitaire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 576 | Five Dice | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 577 | Galactic Conquest | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 578 | Happyweed! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 579 | Ingemar's skiing game | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 580 | Jotto ][ | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 581 | Keno Buddies | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 582 | Labyrinth | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 583 | Mac Football Manager | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 584 | NetMech | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 585 | OrbMazez | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 586 | PanZee Two | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 587 | Quasar Phase II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 588 | RChess | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 589 | Super Snake | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 590 | Tank Wars | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 591 | UltraYahtzee | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 592 | Vintage Quest | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 593 | WaterBalloons | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 594 | Yamagi Quake 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 595 | Zebulon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 596 | Alans Solitaire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 597 | BasketCase | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 598 | Captain Magneto | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 599 | Dart Board | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 600 | Elfin Clash | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 601 | FiveStones | [Non-profit unchanged grant](https://github.com/benletchford/systemless/issues/3976) | SHA-256 / 109,973 bytes | Native full match + browser moves | Browser moves + AI replies | [Both release-player slices checked](CATALOGUE-SOURCES.md#fivestones-launch-qualification) | [Merged #3977](https://github.com/benletchford/systemless/pull/3977) | [Live v0.86.0, both slices checked](https://systemless.org/fivestones-24/) |
| 602 | Galactic Frontiers | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 603 | Hardwood Solitaire II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 604 | IntelliBots | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 605 | Joy Of Hex | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 606 | KeyWack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 607 | LandSlide | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 608 | Mac Invaders | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 609 | NetRisk | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 610 | OTChess | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 611 | Par Pyramid | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 612 | Querkz | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 613 | Re-Pete | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 614 | Sword Dream II 3D | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 615 | Tanks of Terror | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 616 | Ultris | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 617 | Violent Moon | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 618 | Whack-a-Dole | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 619 | Yard Sale Junkie | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 620 | Zen | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 621 | Alien | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 622 | battalion | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 623 | Caribbean Stud Poker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 624 | Darts | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 625 | Enigma | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 626 | Fixation | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 627 | Galactic Revolt | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 628 | Haunted House | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 629 | Invaders! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 630 | JSokoban | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 631 | Killer Dice | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 632 | Lazer Zone | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 633 | Mac Libs | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 634 | NetRPG | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 635 | Otello | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 636 | PC Invaders | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 637 | Quest of Yipe! II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 638 | Reckless Drivin' | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 639 | Same | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 640 | Techoids | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 641 | Uncle Zebulon's Will | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 642 | Virtual Maze Book | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 643 | Woden | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 644 | Yellow Brick Road (イエロー・ブリック・ロード | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 645 | ZenMac | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 646 | Alien Assault | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 647 | Battle for the Planets | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 648 | casino | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 649 | Darkwood | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 650 | Enigmatic Movements | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 651 | Flaps | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 652 | Galactic Trader | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 653 | Haunted Mansion | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 654 | iPoker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 655 | Juxto | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 656 | Killer Dice Y2K | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 657 | Lines of Action | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 658 | Mac Mine Sweeper | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 659 | NetTower | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 660 | Otiru | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 661 | Peg Solitaire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 662 | Quick Draw Poker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 663 | Reflect! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 664 | Same Game | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 665 | Tetris 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 666 | Unicycle! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 667 | Virtual Pet | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 668 | Word Find | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 669 | Yellow Brick Road II (イエロー・ブリック・ロードII | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 670 | Zonez | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 671 | Alien Attack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 672 | Battle Pong | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 673 | Casino BlackJack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 674 | DeadEnd | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 675 | Escape! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 676 | Flip Out | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 677 | Galactica | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 678 | HeartQuest | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 679 | iPoker 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 680 | J. B. Harold: Blue Chicago Blues (J.B.ハロルド ブルー・シカゴ・ブルース | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 681 | King Albert II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 682 | Lobster | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 683 | Mac Pan | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 684 | NetTron | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 685 | Overload | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 686 | Peng! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 687 | QuickShot | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 688 | Regicide | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 689 | Samurai | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 690 | TetrisLight | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 691 | Unscramble | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 692 | Virtual Pong | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 693 | Word Math | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 694 | Zroids | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 695 | Alien Empire | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 696 | BattleTanks | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 697 | Casino Rose | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 698 | Death Blade | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 699 | Ether Contention | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 700 | Flippant | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 701 | Galaxis | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 702 | Hearts | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 703 | iPuzzle | [Permitted unchanged](CATALOGUE-SOURCES.md#browser-qualification--10-october-2026) | SHA-256 / 89,881 bytes | Native + release browser moves | N/A (68K only) | Release worker input + hosted archive checked | [#3946 passed and merged](https://github.com/benletchford/systemless/pull/3946) | [Live v0.85.0](https://systemless.org/ipuzzle-10-68k/) |
| 704 | Jack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 705 | Kingyo | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 706 | Logic Puzzle DA | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 707 | Mac Video Poker | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 708 | Netzee | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 709 | O!Kay! CD-ROMs 1999 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 710 | Pente | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 711 | QuickShot Deluxe | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 712 | Renegade Space Ninja | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 713 | Samurai Guy | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 714 | The 10 Tile Puzzle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 715 | UFO: Alien Invasion | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 716 | VirtualHamster | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 717 | Wordblock | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 718 | Zak & Jack in Showdown at Monstertown | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 719 | Alien Invaders | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 720 | Baxter | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 721 | Castle in the Clouds | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 722 | Death From Above | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 723 | Euchre | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 724 | Flipper | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 725 | Galaxus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 726 | Hearts Deluxe | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 727 | Iraq Attack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 728 | Jack and the Beanstalk Starring You | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 729 | Kirei! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 730 | Lord of the Deck | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 731 | MacBandit | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 732 | Nim | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 733 | O!Kay! CD-ROMs 2000 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 734 | Pentominoes | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 735 | QuickTile | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 736 | Rescue! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 737 | Save the World | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 738 | The Cow Catching Game | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 739 | Ultima III: Exodus (1985 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 740 | Vortex | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 741 | WordMix | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 742 | Zandronum (Doom Sourceport | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 743 | Allostris | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 744 | BazFaz | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 745 | Castle of Lost Souls | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 746 | Deep Sea | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 747 | Evolutionary War | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 748 | FlipSide | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 749 | Galaxus Hydra | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 750 | Hemiroids | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 751 | Iris Puzzle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 752 | Jack Keane | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 753 | Klondike | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 754 | Lose Your Marbles! | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 755 | MacBaseball | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 756 | Nomis | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 757 | O!Kay! CD-ROMs 2001 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 758 | Pentris | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 759 | Quiver | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 760 | Reversi | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 761 | Scrambler | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 762 | The Cursed Puzzle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 763 | Ultimate Dozen Dash Collection | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 764 | V for Victory: Gold - Juno - Sword | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 765 | Wumpus | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 766 | Zarvox Is Here | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 767 | AlphaBoat | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 768 | Beached | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 769 | Castle Quest | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 770 | Delirium | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 771 | Exobattle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 772 | Flower Puzzle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 773 | Gamble | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 774 | Henge | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 775 | Island Defender | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 776 | Jack Nicklaus 4 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 777 | Knockoff | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 778 | Lost Crystal | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 779 | MacCheckers | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 780 | NotAdventure II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 781 | O!Kay! CD-ROMs 2002 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 782 | PerTetride | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 783 | QBeez 2 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 784 | Reversi LP | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 785 | Scruffy ][ | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 786 | The Fantastic War | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 787 | V for Victory: Market Garden | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 788 | WWII SkyFighters | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 789 | Zaum Gadget | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 790 | Anacrostics | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 791 | Beached II | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 792 | Catacombs | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 793 | Deluxe Klondike | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 794 | Expert Blackjack | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 795 | Fly Don't Die | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 796 | Game of the Winds | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 797 | Hexmines | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 798 | iTarget | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 799 | Jack Nicklaus' Greatest 18 Holes of Major Championship Golf | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 800 | Knot | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 801 | Lost New York | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 802 | MacCrypt | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 803 | NS-SHAFT | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 804 | O!Kay! CD-ROMs 2003 + 2004 | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 805 | Pillbug Golf | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 806 | QBz | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 807 | Reversi Unlimited | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 808 | Scrungle | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 809 | The Fishin' Hole | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 810 | V for Victory: Utah Beach | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 811 | Wacky Races: A Tsuyoshi Takashiro Digital film (チキチキマシン猛レース | Pending | Pending | Pending | Pending | Pending | Pending | Pending |
| 812 | Anagrams | Pending | Pending | Pending | Pending | Pending | Pending | Pending |

## Reserve policy

Keep a rejected candidate and its evidence in this ledger; record the reserve ID, replacement title and date in that slot. Do not silently renumber or delete blockers. Track catalogue consolidation separately because it changes the baseline. The 100 reserve candidates remain in the target list until selected.
