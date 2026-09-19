---
id: escape-velocity
kind: game
title: Escape Velocity
summary: Trade cargo, explore star systems and choose sides in a spacefaring conflict.
developer: Matt Burch
publisher: Ambrosia Software
year: 1996
architectures:
- 68k
- ppc
default_architecture: 68k
category: Space Trading
aliases:
- /ev
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-13
    tester: Catalogue maintainer
    systemless_version: 0.40.0
    architecture: 68k
    environment: Local website preview in the in-app browser
    status: playable
    evidence: >-
      https://github.com/benletchford/systemless.org/blob/master/entries/escape-velocity.md
controls:
  mobile:
    button_groups:
    - buttons:
      - key: Space
        label: Fire
      - key: Shift
        label: Sec
      - key: W
        label: Weap
      - key: Tab
        label: Targ
      - key: R
        label: Near
      - key: S
        label: Safe
      label: Wpn
    - buttons:
      - key: Z
        label: Burn
      - key: A
        label: Auto
      - key: J
        label: Jump
      - key: H
        label: Hyp
      - key: \
        label: HSel
      - key: L
        label: Land
      label: Nav
    - buttons:
      - key: Enter
        label: OK
      - key: "Y"
        label: Comm
      - key: B
        label: Brd
      - key: C
        label: Recl
      - key: V
        label: Hold
      - key: F
        label: Retg
      label: Dock
    - buttons:
      - key: M
        label: Map
      - key: P
        label: PInf
      - key: I
        label: MInf
      - key: U
        label: Clok
      - key: Escape
        label: Paus
      label: Info
    enabled: true
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 5c6b8ed5ee1efb67f3f7cbe62b051f36d26753337f18a238febbdc023ef0a5a2
    size_bytes: 5401984
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/escape-velocity
    - https://download.macintoshgarden.org/games/EV_Installer_1.0.5.bin
    license: Ambrosia Software shareware license
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      Non-profit redistribution of the complete, unmodified package is permitted.
      Distribution for profit requires written permission.
    notes: "Checked the bundled Escape Velocity License.text in the independently retrieved 1.0.5 installation. Registration is required after the trial period. Retrieved 2026-09-13: 5401984 bytes; SHA-256 5c6b8ed5ee1efb67f3f7cbe62b051f36d26753337f18a238febbdc023ef0a5a2."
- id: plugin-map-patch-for-escape-velocity
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/2011EVMapPatch.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/2011EVMapPatch.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-a-loner-to-die-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/A_Loner_to_Die.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/A_Loner_to_Die.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aev-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AEV.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AEV.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alertsounds-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AlertSounds.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AlertSounds.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alexs-great-war-2-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alexs%20Great%20War%202.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alexs%20Great%20War%202.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alien-empire-1-1-hqx-1
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alien%20Empire%201.1.hqx.1
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alien%20Empire%201.1.hqx.1
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alien-empire
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alien%20Empire.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alien%20Empire.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alien-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/alien.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/alien.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alpha-console-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alpha%20Console.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Alpha%20Console.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alpha1-0b12-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/alpha1.0b12.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/alpha1.0b12.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-alteredstates105-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AlteredStates105.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AlteredStates105.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ambrosia-1-0-1-plug-in-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Ambrosia%201.0.1%20Plug-In.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Ambrosia%201.0.1%20Plug-In.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-anarchists1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/anarchists1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/anarchists1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-angelsofvengeance-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AngelsOfVengeance.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AngelsOfVengeance.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aqua-buttons-1-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Aqua_Buttons_1.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Aqua_Buttons_1.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aquinacorp1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AquinaCorp1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AquinaCorp1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-arctco110-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ArcTCo110.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ArcTCo110.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-area-51-1-6
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Area_51_1.6.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Area_51_1.6.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-arena-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/arena.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/arena.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-argonaut1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Argonaut1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Argonaut1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-armada128-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Armada128.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Armada128.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-artifact1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Artifact1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Artifact1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-asdom2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Asdom2.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Asdom2.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-asteridian-3-0-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Asteridian%203.0.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Asteridian%203.0.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-asteroidbeam-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AsteroidBeam.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AsteroidBeam.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-asteroidships10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AsteroidShips10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/AsteroidShips10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-astex2-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/astex2.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/astex2.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-avichai1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/avichai1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/avichai1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-azertykeyboard-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/azertykeyboard.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/azertykeyboard.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-babylon5
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Babylon5.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Babylon5.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-balatic1-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/balatic1.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/balatic1.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-battle-front-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Battle_Front.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Battle_Front.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-battleofvalhalla091-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BattleOfValhalla091.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BattleOfValhalla091.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-beefyships1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BeefyShips1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BeefyShips1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-behemothpluginpack2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BehemothPluginPack2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BehemothPluginPack2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-betterramscoop-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BetterRamscoop.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BetterRamscoop.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-betweentimeandspaceteaser-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BetweenTimeAndSpaceTeaser.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BetweenTimeAndSpaceTeaser.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bigevplug-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bigevplug.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bigevplug.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-black-pearl-1-0-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Black_Pearl_1.0.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Black_Pearl_1.0.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-blackdrek1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/blackdrek1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/blackdrek1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-blackpanel-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BlackPanel.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BlackPanel.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-blackthornekestrel-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BlackthorneKestrel.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BlackthorneKestrel.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bladez-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bladez.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bladez.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bmw-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BMW.sea.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BMW.sea.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-borg1-00-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/borg1.00.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/borg1.00.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-botkupdater1-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BOTKupdater1.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BOTKupdater1.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bountyplus201-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bountyplus201-1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bountyplus201-1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bountyplus201
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BountyPlus201.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BountyPlus201.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bountyplus21-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bountyplus21.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/bountyplus21.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bountyplusgraphics2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BountyPlusGraphics2.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BountyPlusGraphics2.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-brotherhoodofthekestrel-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BrotherhoodOfTheKestrel.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BrotherhoodOfTheKestrel.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-button-changer-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Button_Changer_1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Button_Changer_1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bvgraphicsaddon-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BVGraphicsAddOn.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/BVGraphicsAddOn.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cascadeindustries-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CascadeIndustries.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CascadeIndustries.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-claviusandbeyond32s-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ClaviusAndBeyond32s.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ClaviusAndBeyond32s.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-claviusandbeyond-333-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ClaviusAndBeyond_333.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ClaviusAndBeyond_333.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-claviusandbeyondhints-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ClaviusAndBeyondHints.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ClaviusAndBeyondHints.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cleanrecordv10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CleanRecordv10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CleanRecordv10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cleanrecordv11-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CleanRecordv11.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CleanRecordv11.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cleanrecordv13-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CleanRecordv13.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CleanRecordv13.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cloakedships-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CloakedShips.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CloakedShips.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-colossus1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/colossus1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/colossus1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-conex12-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ConEx12.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ConEx12.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-conex12u-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ConEx12u.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ConEx12u.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-confed-crusier-fix
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Confed_Crusier_Fix.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Confed_Crusier_Fix.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cool-ships-are-added-ne-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Cool_Ships_are_Added__NE.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Cool_Ships_are_Added__NE.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-coolerbutton101
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CoolerButton101.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CoolerButton101.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-corvettebay0-6b3-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/corvettebay0.6b3.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/corvettebay0.6b3.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-crazy-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Crazy%21.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Crazy%21.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cssplug-in-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CSSPlug-in.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/CSSPlug-in.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cydonianmissions-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/cydonianmissions.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/cydonianmissions.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-darkstorm1-02-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/darkstorm1.02.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/darkstorm1.02.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-davonia1-0-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Davonia1.0.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Davonia1.0.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-deathstar2-9-8-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/deathstar2.9.8.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/deathstar2.9.8.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-deathstar20-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DeathStar20.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DeathStar20.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-deathstarenv-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/deathstarenv.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/deathstarenv.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-decoy-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/decoy.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/decoy.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-delorean2-3-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DeLorean2.3.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DeLorean2.3.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-destroyer-bay
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Destroyer%20Bay
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Destroyer%20Bay
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-destroyerwars
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-destroyerwars1-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars1.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars1.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-destroyerwars2-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars2.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars2.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-destroyerwars5-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars5.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars5.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-destroyerwars5-2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars5.2.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DestroyerWars5.2.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-devins-ev-programs-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Devins%20EV%20Programs.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Devins%20EV%20Programs.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dialogexpander1-0-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DialogExpander1.0.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DialogExpander1.0.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dilithium1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/dilithium1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/dilithium1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ds9vsdominion-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DS9vsDominion.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/DS9vsDominion.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-e1-war-without-end-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E1-WAR_WITHOUT_END.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E1-WAR_WITHOUT_END.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-e1updater-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E1updater.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E1updater.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-e2-dark-horizons-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E2-DARK_HORIZONS.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E2-DARK_HORIZONS.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-e2updater-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E2updater.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E2updater.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-e2updater-sit-2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E2updater.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E2updater.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-e3-endgame-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E3-ENDGAME.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E3-ENDGAME.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-e3updater1-2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E3updater1.2.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/E3updater1.2.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-electronium-1-0-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Electronium%201.0.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Electronium%201.0.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-elitefrontiers-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EliteFrontiers.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EliteFrontiers.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-elphan-iv1-1-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Elphan_Iv1.1.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Elphan_Iv1.1.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-empire-sea-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Empire.sea.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Empire.sea.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-empireofcrime-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EmpireOfCrime.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EmpireOfCrime.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-empirevsrebellion-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EmpireVsRebellion.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EmpireVsRebellion.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-enhanced-buttons-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Enhanced%20Buttons.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Enhanced%20Buttons.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-enhancedstuff101
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/enhancedstuff101.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/enhancedstuff101.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-enterprise0-96-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/enterprise0.96.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/enterprise0.96.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-epicmissions1-22-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/epicmissions1.22.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/epicmissions1.22.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-era-a-02
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Era-A.02
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Era-A.02
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-escortcarrier11-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EscortCarrier11.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EscortCarrier11.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-ai-upgrade-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20AI%20Upgrade%20.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20AI%20Upgrade%20.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-expand-v1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20expand%20v1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20expand%20v1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-expand-v1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20expand%20v1.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20expand%20v1.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-game-expander-1-0u2-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20Game%20Expander%201.0u2.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20Game%20Expander%201.0u2.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-graphics-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20GRAPHICS.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20GRAPHICS.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-plug-in-asteridian2-0
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20Plug-in%20Asteridian2.0
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20Plug-in%20Asteridian2.0
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-radicalcolumn-3-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20RadicalColumn%203.0%20.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20RadicalColumn%203.0%20.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-super-helper-v2-0
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20SUPER%20Helper%20v2.0
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20SUPER%20Helper%20v2.0
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-targets-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20Targets.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV%20Targets.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-borg-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV-Borg.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV-Borg.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-sole-survivor-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV-Sole%20Survivor.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV-Sole%20Survivor.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-target-graphics-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ev-target-graphics.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ev-target-graphics.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-better-graphics
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Ev_Better_Graphics
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Ev_Better_Graphics
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-better-graphics-1-0-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Ev_Better_Graphics_1.0.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Ev_Better_Graphics_1.0.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-data-str-fix
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Data_STR_Fix.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Data_STR_Fix.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-destroyer-1-0-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Destroyer_1.0.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Destroyer_1.0.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-destroyer-1-2-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Destroyer_1.2.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Destroyer_1.2.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-destroyer-1-4-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Destroyer_1.4.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Destroyer_1.4.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-mods-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Mods.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_Mods.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-solid-1-0a-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_solid_1.0a.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EV_solid_1.0a.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-webboard-governments-s
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ev_webboard_governments.s.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ev_webboard_governments.s.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evadventures10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVAdventures10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVAdventures10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evbluecolumn1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVBlueColumn1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVBlueColumn1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evboom-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVBoom.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVBoom.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evdeutsch103-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVDeutsch103.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVDeutsch103.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evenabler101-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVEnabler101.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVEnabler101.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evfirebird-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evfirebird.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evfirebird.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evgeproxhelp-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evgeproxhelp.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evgeproxhelp.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evgovtfixer1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVGovtFixer1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVGovtFixer1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evmagma10-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evmagma10.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evmagma10.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evmissioncontrol-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVMissionControl.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVMissionControl.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evnewdata-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVNewData.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVNewData.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evoshipsetc2-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVOShipsEtc2.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVOShipsEtc2.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evpluginpackage-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVPluginPackage.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVPluginPackage.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evplugs-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evplugs.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evplugs.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evplus-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVPlus.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVPlus.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evtg-sw-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evtg-sw.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evtg-sw.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evtg-swana-se-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evtg-swana-se.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evtg-swana-se.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evtg-swana-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evtg-swana.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/evtg-swana.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evulas3dtargets-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Evulas3DTargets.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Evulas3DTargets.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evultra
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVUltra.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EVUltra.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extramissions1-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/extramissions1.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/extramissions1.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extraships1-2-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/extraships1.2.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/extraships1.2.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extraships1-3-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/extraships1.3.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/extraships1.3.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-eyeoforion13-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EyeOfOrion13.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/EyeOfOrion13.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-fair-boy-bin
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/fair%20boy.bin.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/fair%20boy.bin.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ffb2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FFB2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FFB2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-fighterbaysgalore-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FighterBaysGalore.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FighterBaysGalore.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-final-battle-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Final%20Battle%20.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Final%20Battle%20.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-finalbattle41
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/finalbattle41.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/finalbattle41.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flightacademy-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/flightacademy.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/flightacademy.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-floating-fortress-1-2-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Floating%20Fortress%201.2.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Floating%20Fortress%201.2.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-floating-fortress-1-2-1-sea-2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Floating_Fortress_1.2.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Floating_Fortress_1.2.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flying-high-1-5-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%201.5.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%201.5.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flying-high-2-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%202.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%202.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flying-high-2-1-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%202.1-1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%202.1-1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flying-high-3-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%203.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Flying%20High%203.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flyinghigh1-7-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FlyingHigh1.7.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FlyingHigh1.7.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-forkliftmissions-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ForkliftMissions.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ForkliftMissions.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-forkliftremover-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ForkliftRemover.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ForkliftRemover.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-foundationpackage14-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FoundationPackage14.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/FoundationPackage14.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-free-ev-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/free-ev.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/free-ev.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-freemen1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/freemen1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/freemen1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-funky-colored-sidebar-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Funky-Colored%20Sidebar.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Funky-Colored%20Sidebar.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gae-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/gae.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/gae.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gae10b4f-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GAE10b4F.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GAE10b4F.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gae-landscape-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GAE_Landscape.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GAE_Landscape.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gaenhanced-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GAEnhanced.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GAEnhanced.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galacticall2-0p8-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/galacticall2.0p8.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/galacticall2.0p8.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galacticexpress1-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/galacticexpress1.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/galacticexpress1.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galacticinsanity-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GalacticInsanity.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GalacticInsanity.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galacticjavind1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/galacticjavind1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/galacticjavind1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-goldilocks10b2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Goldilocks10b2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Goldilocks10b2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-goodshiplp1-31-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/goodshiplp1.31.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/goodshiplp1.31.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gozerla-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Gozerla.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Gozerla.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-great-war-1-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Great%20War%201.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Great%20War%201.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-greatcivilwar1-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/greatcivilwar1.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/greatcivilwar1.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-greatwar0-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GreatWar0.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GreatWar0.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gs-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GS.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GS.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gsto145-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GSto145.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/GSto145.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gundrones1-31-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/gundrones1.31.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/gundrones1.31.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-heartofdarkness-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HeartOfDarkness.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HeartOfDarkness.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-helper-ev
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Helper%20EV.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Helper%20EV.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hft1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/hft1.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/hft1.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hodupdater1-1-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HODupdater1.1.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HODupdater1.1.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hornet10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Hornet10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Hornet10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hostiletakeover1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HostileTakeover1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HostileTakeover1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hsifados2-4-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/hsifados2.4.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/hsifados2.4.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hyperionproject12-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HyperionProject12.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/HyperionProject12.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-inherentgovt1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/InherentGovt1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/InherentGovt1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-interfaceenhancer-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/InterfaceEnhancer.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/InterfaceEnhancer.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-jamesfond3-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/JamesFond3.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/JamesFond3.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-janiss-plugin-1-0-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Janiss%20Plugin%201.0%20.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Janiss%20Plugin%201.0%20.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-jet092-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Jet092.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Jet092.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-josie-maran-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Josie%20Maran.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Josie%20Maran.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-jumpnow-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/JumpNow.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/JumpNow.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-kestrelcollection-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KestrelCollection.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KestrelCollection.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-kestrelplus-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KestrelPlus.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KestrelPlus.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-kingfisherind1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/kingfisherind1.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/kingfisherind1.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-klingonandfederationwar-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KlingonAndFederationWar.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KlingonAndFederationWar.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ksv-entreprises-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KSV_Entreprises.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/KSV_Entreprises.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-kys-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Kys.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Kys.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-laser-beams-and-upgrades-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Laser%20Beams%20and%20Upgrades.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Laser%20Beams%20and%20Upgrades.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lbe1-1-02-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/lbe1-1.02.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/lbe1-1.02.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-legion1-52-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/legion1.52.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/legion1.52.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-levofix-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LevoFix.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LevoFix.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-light-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/light.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/light.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lightningjump-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/lightningjump.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/lightningjump.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lone-cruiser-plug-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Lone_Cruiser_plug.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Lone_Cruiser_plug.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lotsobays10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LotsoBays10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LotsoBays10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lovelyhunsruck-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LovelyHunsruck.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LovelyHunsruck.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lproof-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LProof.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/LProof.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mach-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mach.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mach.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-maelstrom-15-plugin-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Maelstrom-15-plugin.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Maelstrom-15-plugin.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-maelstrom1-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/maelstrom1.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/maelstrom1.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-magma-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Magma.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Magma.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-megaplugin1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/megaplugin1.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/megaplugin1.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-merchant-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Merchant.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Merchant.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-merchantworlds2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/MerchantWorlds2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/MerchantWorlds2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-missionimpossible1-4-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/missionimpossible1.4.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/missionimpossible1.4.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-missionpatch10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/MissionPatch10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/MissionPatch10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-missionsgalore2-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/missionsgalore2.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/missionsgalore2.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mmbay-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/mmbay.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/mmbay.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-modified-strings-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Modified%20strings.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Modified%20strings.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-moreevvoices-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/moreevvoices.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/moreevvoices.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mostwanted1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/mostwanted1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/mostwanted1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mti1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/MTI1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/MTI1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mugabi-and-destiny-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mugabi_and_Destiny.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mugabi_and_Destiny.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mugabi-and-destiny-her-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mugabi_and_Destiny_her.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mugabi_and_Destiny_her.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mxplug-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/mxplug.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/mxplug.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mysterious-force-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mysterious_Force.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Mysterious_Force.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-navallightnings-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NavalLightnings.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NavalLightnings.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-navallightnings102-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/navallightnings102.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/navallightnings102.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-neptron15-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Neptron15.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Neptron15.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-nerdsidebar-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NerdSidebar.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NerdSidebar.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-neutronic-kestrel-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Neutronic_Kestrel.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Neutronic_Kestrel.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-neutronturret-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NeutronTurret.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NeutronTurret.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-new-letheancydonian-ships-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/New%20LetheanCydonian%20ships.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/New%20LetheanCydonian%20ships.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-new-ships-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/New%20ships.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/New%20ships.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newbars-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NewBars.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NewBars.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newbrodania101-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NewBrodania101.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NewBrodania101.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newcanada1-00-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newcanada1.00.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newcanada1.00.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newendorp-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Newendorp.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Newendorp.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newhorizons-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newhorizons.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newhorizons.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newmilitia0-b3-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newmilitia0.b3.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newmilitia0.b3.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newplanets1-03-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newplanets1.03.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newplanets1.03.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtechnology1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newtechnology1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/newtechnology1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-nfsa-1-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NFSA%201.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NFSA%201.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-nfsa-1-2-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NFSA%201.2.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/NFSA%201.2.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ninnyman10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ninnyman10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ninnyman10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-nod1-0b1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/nod1.0b1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/nod1.0b1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-nova-bracket-warning-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/nova_bracket_warning.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/nova_bracket_warning.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-obsidianbuttons-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ObsidianButtons.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ObsidianButtons.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-omegafaction1-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/omegafaction1.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/omegafaction1.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-onyx1-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/onyx1.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/onyx1.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-oreste-4-1-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Oreste%204.1.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Oreste%204.1.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-otp-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/otp.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/otp.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-pale1-9-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Pale1.9.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Pale1.9.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-paletitles-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/paletitles.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/paletitles.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-paralleluniverse-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/paralleluniverse.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/paralleluniverse.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-parrots-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/parrots.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/parrots.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-pdainc503-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PDAInc503.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PDAInc503.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-pdaincforward105-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PDAIncForward105.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PDAIncForward105.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-pegasusmissions1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/pegasusmissions1.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/pegasusmissions1.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-people1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/People1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/People1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-personalitiesplug-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PersonalitiesPlug.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PersonalitiesPlug.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-persons-of-evwebboard-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Persons_of_EVwebboard.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Persons_of_EVwebboard.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-phoenixcorvette1-12-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/phoenixcorvette1.12.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/phoenixcorvette1.12.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-pilots-helper-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/pilots_helper.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/pilots_helper.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-pilotutilitypack-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PilotUtilityPack.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PilotUtilityPack.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-pinkbeard-the-pirate-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Pinkbeard%20the%20Pirate.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Pinkbeard%20the%20Pirate.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-planetarydefenses1-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/planetarydefenses1.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/planetarydefenses1.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-planetsplug
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PlanetsPlug
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PlanetsPlug
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-planetsplug2000
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PlanetsPlug2000
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/PlanetsPlug2000
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-q-merch-alphor-1-0-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Q_Merch__Alphor_1.0.0..sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Q_Merch__Alphor_1.0.0..sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-qumre104-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/qumre104.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/qumre104.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-qumre104u-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/qumre104u.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/qumre104u.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-raiders1-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/raiders1.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/raiders1.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rangers1-05-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rangers1.05.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rangers1.05.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-raptor1-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/raptor1.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/raptor1.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-realmofprey-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RealmOfPrey.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RealmOfPrey.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebel-command
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Rebel%20command
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Rebel%20command
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebel-upgrade-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Rebel%20Upgrade.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Rebel%20Upgrade.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebelalliance1-50-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rebelalliance1.50.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rebelalliance1.50.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebelbattleship-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RebelBattleship.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RebelBattleship.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebeldreadnought1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rebeldreadnought1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rebeldreadnought1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebelescortcarrier-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RebelEscortCarrier.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RebelEscortCarrier.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebelescortcarrier1-0-1-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RebelEscortCarrier1.0.1.sea.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RebelEscortCarrier1.0.1.sea.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebels-plug-1-0-0-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Rebels_Plug_1.0.0.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Rebels_Plug_1.0.0.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebelstriker-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rebelstriker.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rebelstriker.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-recession1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/recession1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/recession1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-red-nose-ships-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Red%20Nose%20Ships.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Red%20Nose%20Ships.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reddwarf-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RedDwarf.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RedDwarf.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-redplug-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RedPlug.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/RedPlug.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-return10-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/return10.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/return10.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ropupdater1-2-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ROPupdater1.2.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ROPupdater1.2.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rsf0-99b1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rsf0.99b1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rsf0.99b1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rumblingplanet1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rumblingplanet1.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/rumblingplanet1.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-saab10-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/saab10.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/saab10.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-salvager-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Salvager.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Salvager.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sateliteoflove-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/SateliteOfLove.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/SateliteOfLove.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-satori-station-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Satori_Station.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Satori_Station.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-satori-station-update-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Satori_Station_Update.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Satori_Station_Update.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-scoutship-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Scoutship%20.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Scoutship%20.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-seeker1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/seeker1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/seeker1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shipsgalore-1-12-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ShipsGalore%201.12.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ShipsGalore%201.12.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shipsgalore-2-5-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ShipsGalore%202.5.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ShipsGalore%202.5.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shipsgalore-ii-1-0-cpt
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ShipsGalore%20II%201.0.cpt.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ShipsGalore%20II%201.0.cpt.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shuttles-vs-freighters-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Shuttles_VS_Freighters.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Shuttles_VS_Freighters.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-si-sr2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/SI_SR2.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/SI_SR2.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-smv-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/SMV.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/SMV.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spaceballs-ev
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spaceballs%20EV.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spaceballs%20EV.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spacerocks1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/spacerocks1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/spacerocks1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spam1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/spam1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/spam1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sparklien1-0b-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/sparklien1.0b.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/sparklien1.0b.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spartus1-1-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.1.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.1.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spartus1-2-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.2.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.2.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spartus1-3-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.3.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.3.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spartus1-5-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.5.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Spartus1.5.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ssemulator1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ssemulator1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ssemulator1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-st-the-plug-in-55b1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ST%2C%20The%20Plug%20In%20.55b1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ST%2C%20The%20Plug%20In%20.55b1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-star-wars-ana-68k
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Star%20Wars-ANA%2068k.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Star%20Wars-ANA%2068k.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-star-wars-ana-ppc
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Star%20Wars-ANA%20PPC.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Star%20Wars-ANA%20PPC.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starclashcv10
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/StarclashCv10.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/StarclashCv10.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starfighters-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Starfighters.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Starfighters.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starfx1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/StarFX1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/StarFX1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starwars2-01-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwars2.01.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwars2.01.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starwars2-05upd-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwars2.05upd.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwars2.05upd.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starwarsdeathstar-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwarsdeathstar.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwarsdeathstar.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starwarsfix1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwarsfix1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwarsfix1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starwarsfix2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwarsfix2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/starwarsfix2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-stev-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/STEV.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/STEV.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sting-and-hive
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Sting%20and%20Hive
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Sting%20and%20Hive
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-strife-teaser-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/strife-teaser.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/strife-teaser.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-super-pirates-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Super%20Pirates.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Super%20Pirates.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-supershield-1-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Supershield%201.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Supershield%201.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-swedishkeyboard-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/swedishkeyboard.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/swedishkeyboard.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-switch-sides-1-1-cpt
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Switch_Sides_1.1.cpt.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Switch_Sides_1.1.cpt.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-switchgovt1-00-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/switchgovt1.00.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/switchgovt1.00.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-swoopv1-0-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Swoopv1.0.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Swoopv1.0.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-swtge-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/swtge.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/swtge.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-t-g-w-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/T.G.W.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/T.G.W.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-talon11u-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Talon11u.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Talon11u.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-talon18-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/talon18.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/talon18.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-targetenhancer105r2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TargetEnhancer105r2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TargetEnhancer105r2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-taxi-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Taxi.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Taxi.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-terran-rebellion-folder
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Terran%20Rebellion%20Folder.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Terran%20Rebellion%20Folder.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-terran-rebellion-1-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Terran_Rebellion_1.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Terran_Rebellion_1.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-terran-rebellion-2-0
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Terran_Rebellion_2.0.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Terran_Rebellion_2.0.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-terrannaval1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/terrannaval1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/terrannaval1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-terrawattsg-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/terrawattsg.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/terrawattsg.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thamahawk-37-morpher
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/thamahawk_37_morpher
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/thamahawk_37_morpher
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-the-year-of-the-rebels-2-0-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The%20Year%20of%20the%20Rebels%202.0.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The%20Year%20of%20the%20Rebels%202.0.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-the-grinchians1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The_Grinchians1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The_Grinchians1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-the-sickness-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The_Sickness.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The_Sickness.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-the-weaponator
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The_Weaponator
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/The_Weaponator
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thebladez-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheBladez.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheBladez.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thecygnusalliance-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheCygnusAlliance.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheCygnusAlliance.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thefrontier-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheFrontier.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheFrontier.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-theprivateers102
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/theprivateers102.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/theprivateers102.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-theprivateersreadme-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ThePrivateersReadMe.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ThePrivateersReadMe.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thevisitordemo-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheVisitorDemo.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TheVisitorDemo.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-tiefighter1-02-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/tiefighter1.02.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/tiefighter1.02.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-titan-a-e
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Titan%20.A.E.
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Titan%20.A.E.
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-torgo-and-beyond-messed-up
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Torgo%20And%20Beyond%20Messed%20Up.Bin.
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Torgo%20And%20Beyond%20Messed%20Up.Bin.
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-torgo-and-beyond-spobs
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Torgo%20and%20Beyond%20Spobs.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Torgo%20and%20Beyond%20Spobs.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-torture-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/torture.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/torture.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-turinvpatch-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TurinVPatch.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TurinVPatch.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-tweakit-v1b-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TweakIt_v1B.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/TweakIt_v1B.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-uglyplug0-5b3-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/uglyplug0.5b3.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/uglyplug0.5b3.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultimateevdata1-0-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/UltimateEVData1.0.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/UltimateEVData1.0.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-usw-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/USW.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/USW.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-utopia0-999-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/utopia0.999.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/utopia0.999.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-valhalla1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Valhalla1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Valhalla1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-valkyrie1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/valkyrie1.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/valkyrie1.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-variety
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/variety.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/variety.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-varsis-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Varsis.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Varsis.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-velocityplus2-5-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/velocityplus2.5.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/velocityplus2.5.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-viper-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/viper.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/viper.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-vorlons-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/vorlons.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/vorlons.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-vpk10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/vpk10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/vpk10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-walker0-42b2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/walker0.42b2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/walker0.42b2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-warriors-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/warriors.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/warriors.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-weapon-plug-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Weapon-plug.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Weapon-plug.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-wicked0-b9-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/wicked0.b9.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/wicked0.b9.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-williams1-52-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/williams1.52.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/williams1.52.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-winthewar-cpt
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/WinTheWar.cpt.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/WinTheWar.cpt.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-xenhancments-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/xenhancments.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/xenhancments.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ydwtk1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/YDWTK1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/YDWTK1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-yellow-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Yellow.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Yellow.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-your-private-cruisers-sea-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Your_Private_Cruisers.sea.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/Your_Private_Cruisers.sea.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-yster1-00-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/yster1.00.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/yster1.00.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-zen-v-beta-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ZEN-%20v.Beta.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/ZEN-%20v.Beta.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-zzzpalevisagefix-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/zzzpalevisagefix.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EV.zip/EV/Plugins/zzzpalevisagefix.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 5294854e83df7be34719c02df8ffc3f6f4a9b48f1b6389b4afeb92fa4f39bfd0
    size_bytes: 25182
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/tree/master/entries
    permission: >-
      Original screenshot captured for this catalogue at the maintainer’s request.
      Underlying game artwork remains the property of its respective rights holders.
    notes: >-
      Unedited 800×600 game framebuffer captured headlessly on 2026-09-15 while
      replaying a fresh pilot from the untouched 1.0.5 MacBinary installer. No browser,
      operating-system frame, or catalogue controls are present.
plugins:
- id: map-patch-for-escape-velocity
  label: Map Patch for Escape Velocity
  description: >-
    Patch creates new hyperjump links between long distance (and originally
    unconnected) points so that you don\t have to waste five minutes jumping from one side
    of the galaxy to the next. Two hyperjump circular routes were made
    --Satori-1027-0595-1896-6564-48...
  download_artifact: plugin-map-patch-for-escape-velocity
  install: []
- id: a-loner-to-die-sea
  label: A Loner to Die.sea
  description: >-
    This makes all the governments attack you nomatter how much they like you. Only
    download if your really good. If you have any questions, comments, or
    suggestions than just e-mail me at or if you would like more plug-ins just visit my website
    at
  download_artifact: plugin-a-loner-to-die-sea
  install: []
- id: aev-sit
  label: AEV.sit
  description: "Advanced EV is a plug-in for the original Escape Velocity. Very simply, it makes EV more difficult, by making the ships the computer uses more powerful without altering the ships you purchase. It also changes a few things: For example, if you buy a Confed s..."
  download_artifact: plugin-aev-sit
  install: []
- id: alertsounds-sit
  label: AlertSounds.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-alertsounds-sit
  install: []
- id: alexs-great-war-2-1
  label: Alexs Great War 2.1
  description: >-
    This is the great war with a few super ships. So go kill some Aliens. If you
    can buy it get the confed dreadnought. Have Fun.
  download_artifact: plugin-alexs-great-war-2-1
  install: []
- id: alien-empire-1-1-hqx-1
  label: Alien Empire 1.1.hqx.1
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-alien-empire-1-1-hqx-1
  install: []
- id: alien-empire
  label: Alien Empire
  description: >-
    This changes the confeds and rebels to Aliens and Terran Rebels. All ships
    weapons are more powerful so you can really heck it out! Made by Alex
    Wouters!!!!!!!!!!!!!
  download_artifact: plugin-alien-empire
  install: []
- id: alien-sit
  label: alien.sit
  description: >-
    This new alien plug-in contains 4 new systems with 4 new planets, not too big
    so it wont be such a pain, new missions to get money or weapon rewards.
  download_artifact: plugin-alien-sit
  install: []
- id: alpha-console-sit
  label: Alpha Console.sit
  description: >-
    Alpha Console© v1.0 brings a fresh, new, younger look to EV that wont fail to
    impress. Theres a new options screen, the in game side panel has a fantastic new
    look, the like of which has not yet been seen, new target graphics, and all the
    in-game menu butto...
  download_artifact: plugin-alpha-console-sit
  install: []
- id: alpha1-0b12-sit
  label: alpha1.0b12.sit
  description: >-
    Right now it has several missions at Deneb III. Soon it will edit everything
    (and have a whole crap load of missions).
  download_artifact: plugin-alpha1-0b12-sit
  install: []
- id: alteredstates105-sit
  label: AlteredStates105.sit
  description: >-
    Altered States alters outfit sizes, weapon physics, outfit descriptions, and
    ship specifications. Generally, outfits are more compact and weapons are more
    capable.
  download_artifact: plugin-alteredstates105-sit
  install: []
- id: ambrosia-1-0-1-plug-in-sit
  label: Ambrosia 1.0.1 Plug-In.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ambrosia-1-0-1-plug-in-sit
  install: []
- id: anarchists1-0-sit
  label: anarchists1.0.sit
  description: >-
    The Anarchists are wealthy privateers and pirates who believe in killing and
    piracy for fun they even kill each other. Join their cause or supress it.
  download_artifact: plugin-anarchists1-0-sit
  install: []
- id: angelsofvengeance-sit
  label: AngelsOfVengeance.sit
  description: "Beer, blues, bulbous-headed aliens and babes: This plug-in has em all. Over 50 new missions, two new opposing governments, several new ships and weapons with unique 3d graphics. Two seperate plots, each with its own shocking conclusion!"
  download_artifact: plugin-angelsofvengeance-sit
  install: []
- id: aqua-buttons-1-1-sit
  label: Aqua_Buttons_1.1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-aqua-buttons-1-1-sit
  install: []
- id: aquinacorp1-sit
  label: AquinaCorp1.sit
  description: >-
    The AquinaCorp plug-in integrates a set of four nifty (and yellow!) starships
    into the Escape Velocity universe. Included are a light, very effective
    escort/fighter, a slow luxury liner, a large, multi-functional bulk freighter and an
    extremely fast "jumbo"...
  download_artifact: plugin-aquinacorp1-sit
  install: []
- id: arctco110-sit
  label: ArcTCo110.sit
  description: >-
    An ion storm deposits you in an alternate EV universe where three new
    governments, and many new planets, ships, outfits and missions are woven together into an
    ingenious plot that can be played out from three sides to two different endings.
  download_artifact: plugin-arctco110-sit
  install: []
- id: area-51-1-6
  label: Area 51 1.6
  description: An abandoned station, a secret base, and an Alien invasion...
  download_artifact: plugin-area-51-1-6
  install: []
- id: arena-sit
  label: arena.sit
  description: >-
    Arena is a battle simulator for EV. Start with a Hawk and work your way up
    through the ranks by fighting ships you select to fight in the Arena. Difficult in my
    opinion, so it should be a challenge for experienced players.
  download_artifact: plugin-arena-sit
  install: []
- id: argonaut1-sit
  label: Argonaut1.sit
  description: >-
    Argonaut 1.0 adds one new ship. In addition there are fourteen new or modified
    missions, thirteen modified ships, fifteen modified outfits, and six modified
    weapons. Many of the dudes and fleets have been realigned and several new
    personalities have been ad...
  download_artifact: plugin-argonaut1-sit
  install: []
- id: armada128-sit
  label: Armada128.sit
  description: >-
    Armada is based on Infinitech Industries, a small company that creates advanced
    ships, weaponry, and accessories. Infinitechs flagship, the Armada, features
    awesome and completely new 3D graphics, as does everything else in the plugin.
    Includes new stuff fo...
  download_artifact: plugin-armada128-sit
  install: []
- id: artifact1-sit
  label: Artifact1.sit
  description: >-
    This plug-in was inspired by the Babylon5 movie "Third Space". It adds several
    ships, weapons, sounds, and outfits, as well as a mission set of 20 or so
    missions. The missions start at Levo.
  download_artifact: plugin-artifact1-sit
  install: []
- id: asdom2
  label: Asdom2
  description: >-
    Welcome to Asdom. We are part of the Orclev system. We are a newly created
    goverment, now that we have teamed up with Acrous shipyards, we are now a force to be
    reckoned with. We are located in the west by the Pirate system. Feel free to
    stop by anytime and...
  download_artifact: plugin-asdom2
  install: []
- id: asteridian-3-0-sit
  label: Asteridian 3.0.sit
  description: "Fixes about 3 billion bugs from the last version. It runs pretty clean and safely now.Adds so many planets I forgot how many I added, around 20 new ship (forgot that too!),and 3 new governments. (I actually remembered how many!)Well worth the download. : )"
  download_artifact: plugin-asteridian-3-0-sit
  install: []
- id: asteroidbeam-sit
  label: AsteroidBeam.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-asteroidbeam-sit
  install: []
- id: asteroidships10-sit
  label: AsteroidShips10.sit
  description: >-
    Contains the Asteriod Ship! Very hard to spot when fighting but a lot of fun to
    fight. ADDS 9 ships, over 12 outfits, 20 missions (five must be unlocked after
    playing the first fifteen). Compatible with version 1.0.5 of EV.
  download_artifact: plugin-asteroidships10-sit
  install: []
- id: astex2-01-sit
  label: astex2.01.sit
  description: >-
    Adds a new system with two new spaceports, some new weapons, a lot of missions,
    and a new ship (the Astex Hulk).
  download_artifact: plugin-astex2-01-sit
  install: []
- id: avichai1-0-sit
  label: avichai1.0.sit
  description: "Adds several new ships, with new graphics: Asp Heavy Fighter, Starfall Mark IX and the Avichai War Cruiser in addition to a new government. The new ships can be found at some shipyards."
  download_artifact: plugin-avichai1-0-sit
  install: []
- id: azertykeyboard-sit
  label: azertykeyboard.sit
  description: >-
    Lets EV 1.0.2 (or later) support AZERTY keyboards (i.e keyboards used in France
    and, probably, some other French speaking countries). Simply drop it in the EV
    Plug-ins folder.
  download_artifact: plugin-azertykeyboard-sit
  install: []
- id: babylon5
  label: Babylon5
  description: >-
    The B5 plug-in adds 24+ systems, 24 missions, 14 ships, 8 govts, 40 outfits,
    almost 24 sounds to EV, and the ability for several systems to CHANGE GOVERNMENTS
    depending on the outcome of some missions! Core missions begin at the start of
    Babylon 5s 3rd season.
  download_artifact: plugin-babylon5
  install: []
- id: balatic1-01-sit
  label: balatic1.01.sit
  description: >-
    Balatic currently adds 1 ship, 1 system ,1 new government, and 1 mission. I
    hope to soon add weapons, sounds, and missions. So enjoy flying!
  download_artifact: plugin-balatic1-01-sit
  install: []
- id: battle-front-sit
  label: Battle_Front.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-battle-front-sit
  install: []
- id: battleofvalhalla091-sit
  label: BattleOfValhalla091.sit
  description: >-
    With 1.5 MB of graphics 21 ships, 10 systems, 16 stellars, 4 governments, 10
    outfits, 10 weapons and lots more, this plug should keep you busy for decades! Join
    the Blissex Corporation and their allies in the war against the returning
    Aliens, or uncover sec...
  download_artifact: plugin-battleofvalhalla091-sit
  install: []
- id: beefyships1-sea
  label: BeefyShips1.sea
  description: >-
    Welcome to the twenty-third century! Beefy Ships thrusts you into a parallel EV
    universe where the arms trade is booming, and kinder, gentler traders are found
    only in reminiscences of the past. You must try to make a living in the crossfire
    of merchants, m...
  download_artifact: plugin-beefyships1-sea
  install: []
- id: behemothpluginpack2-sit
  label: BehemothPluginPack2.sit
  description: >-
    The Behemoth plug-in package includes six new ships, one new weapon, one new
    outfit and three new systems with five new planets and spaceports. The sequel
    "Valhalla", will soon arrive, with even more ships, etc. and missions!
  download_artifact: plugin-behemothpluginpack2-sit
  install: []
- id: betterramscoop-sit
  label: BetterRamscoop.sit
  description: >-
    941.00 B | By Anonymous A simple plug that will increase fuel regeneration 4
    times over the normal ramscoop. Helpful if you use my style of planetary conquest
    or if you are one to often make your last jump into an uninhabited or unfriendly
    system and dont w...
  download_artifact: plugin-betterramscoop-sit
  install: []
- id: betweentimeandspaceteaser-sit
  label: BetweenTimeAndSpaceTeaser.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-betweentimeandspaceteaser-sit
  install: []
- id: bigevplug-sit
  label: bigevplug.sit
  description: >-
    This package includes four new ships and six new weapons, all very cool. Rebel
    Striker, Strikefighter, Blockade Runner and Sweep Mine Fighter.
  download_artifact: plugin-bigevplug-sit
  install: []
- id: black-pearl-1-0-1-sit
  label: Black_Pearl_1.0.1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-black-pearl-1-0-1-sit
  install: []
- id: blackdrek1-0-sit
  label: blackdrek1.0.sit
  description: >-
    Legend has it that in the core of Blackdrek a very little-known-about community
    of monks live. They brew Qwertz and sit around pondering the meaning of life the
    whole day.
  download_artifact: plugin-blackdrek1-0-sit
  install: []
- id: blackpanel-sit
  label: BlackPanel.sit
  description: "Black Panel sticks to its name: it changes the control panel in EV to be mostly black. Plus, it includes a special edition of E3DT (my 3D target plug) that has them with black backgrounds. Enjoy!"
  download_artifact: plugin-blackpanel-sit
  install: []
- id: blackthornekestrel-sit
  label: BlackthorneKestrel.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-blackthornekestrel-sit
  install: []
- id: bladez-sit
  label: bladez.sit
  description: >-
    A far off group of aliens has come to the Rebellions side to help the fight
    against the evil Confederation scums, join them! It has 3 ships, a few missions, no
    weapons, no graphics and no sound.
  download_artifact: plugin-bladez-sit
  install: []
- id: bmw-sea
  label: BMW.sea
  description: >-
    Battle of Milky Way is a decent plug in by any standards. This has taken me
    over 7 months to make and its my 1st. You have to give me a little credit. This is
    my 1st plug-in ever making. It contains a crapload amount of ships, outfits,
    weapons, governments,...
  download_artifact: plugin-bmw-sea
  install: []
- id: borg1-00-sit
  label: borg1.00.sit
  description: This Plug adds a borg cube and some weapons with it.
  download_artifact: plugin-borg1-00-sit
  install: []
- id: botkupdater1-1-sit
  label: BOTKupdater1.1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-botkupdater1-1-sit
  install: []
- id: bountyplus201-1
  label: bountyplus201-1
  description: >-
    Bounty Plus takes you inside the mysterious organization known as the Bounty
    Hunters Guild, a loose association of soldiers of fortune, hot-shot fighter pilots,
    wealthy, powerful merchant princes, and outlaws looking for a second chance...
    Latest version fi...
  download_artifact: plugin-bountyplus201-1
  install: []
- id: bountyplus201
  label: BountyPlus201
  description: >-
    The Bounty Hunters -- weve seen them, fought them, hacked them to fly
    shuttlecraft. . . Now we can join them. Bounty Plus takes you inside the mysterious
    organization known as the Bounty Hunters Guild, a loose association of soldiers of
    fortune, hot-shot f...
  download_artifact: plugin-bountyplus201
  install: []
- id: bountyplus21-sit
  label: bountyplus21.sit
  description: >-
    0.00 B | By Anonymous Bounty Plus takes you inside the mysterious organization
    known as the Bounty Hunters Guild, a loose association of soldiers of fortune,
    hot-shot fighter pilots, wealthy, powerful merchant princes, and outlaws looking
    for a second chan...
  download_artifact: plugin-bountyplus21-sit
  install: []
- id: bountyplusgraphics2
  label: BountyPlusGraphics2
  description: Contains the graphics for the plug-in "BountyPlus" it is required to play.
  download_artifact: plugin-bountyplusgraphics2
  install: []
- id: brotherhoodofthekestrel-sit
  label: BrotherhoodOfTheKestrel.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-brotherhoodofthekestrel-sit
  install: []
- id: button-changer-1-sit
  label: Button_Changer_1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-button-changer-1-sit
  install: []
- id: bvgraphicsaddon-sit
  label: BVGraphicsAddOn.sit
  description: >-
    BV Graphics add-on allows to see the graphics of Battle Velocity 3.0 when
    actually playing the classic Escape Velocity or other plug-ins.
  download_artifact: plugin-bvgraphicsaddon-sit
  install: []
- id: cascadeindustries-sit
  label: CascadeIndustries.sit
  description: >-
    Pirates a large threat. Help the tiny chemical industry Cascade Industries in
    their struggle against pirates and encounter the mighty S.P.A.F.. Over twenty new
    missions, fifteen of them available throughout a very important plot, and a
    brandnew bar for Scor...
  download_artifact: plugin-cascadeindustries-sit
  install: []
- id: claviusandbeyond32s-sit
  label: ClaviusAndBeyond32s.sit
  description: >-
    Fight against a pro-Confed extremist movement and use new weapons, beat off an
    alien invasion, discover an ancient civilization... Land on Clavius Station, and
    depending on your combat skills, you will be able to pilot the famous Neutronic
    Kestrel Intercept...
  download_artifact: plugin-claviusandbeyond32s-sit
  install: []
- id: claviusandbeyond-333-sit
  label: ClaviusAndBeyond_333.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-claviusandbeyond-333-sit
  install: []
- id: claviusandbeyondhints-sit
  label: ClaviusAndBeyondHints.sit
  description: >-
    Hints on installing and using the "Clavius and Beyond" plugin successfully, as
    well as some mission hints.
  download_artifact: plugin-claviusandbeyondhints-sit
  install: []
- id: cleanrecordv10-sit
  label: CleanRecordv10.sit
  description: >-
    This plugin adds a single outfit, the False Transponder, which is a cheaper
    version of the false ID papers.
  download_artifact: plugin-cleanrecordv10-sit
  install: []
- id: cleanrecordv11-sit
  label: CleanRecordv11.sit
  description: >-
    This is an upgrade for my plug-in, Clean Record v1.0. It has cooler graphics
    and descriptions. For those of you who dont know, Clean Record clears your records
    with every government in the galaxy.
  download_artifact: plugin-cleanrecordv11-sit
  install: []
- id: cleanrecordv13-sit
  label: CleanRecordv13.sit
  description: >-
    This is an upgrade to my great plug-in, Clean Record. It adds an outfit that
    clears all of the bad records you have with every governmnet in the galaxy. This
    edition adds another new outfit, the HGFC, that allows you to make a hyperspace
    jump anyware in the...
  download_artifact: plugin-cleanrecordv13-sit
  install: []
- id: cloakedships-sit
  label: CloakedShips.sit
  description: >-
    This plug-in adds ships that uncloak to the Rebelion and to the Confederation
    fleets.
  download_artifact: plugin-cloakedships-sit
  install: []
- id: colossus1-0-sit
  label: colossus1.0.sit
  description: >-
    Its a very straight-forward plug in it replaces a system and it adds 2 ships
    and a weapon (plus a neat graphic for a wormhole).
  download_artifact: plugin-colossus1-0-sit
  install: []
- id: conex12-sit
  label: ConEx12.sit
  description: >-
    ConEx 1.2 fixes all the bugs in ConEx 1.1, and gives the ship Dart a sleeker
    new look! It is equiped with 10 all new ships for the government called
    Consolidated Express, which is now 11 systems strong. Let the 35 new missions make you
    travel throught the g...
  download_artifact: plugin-conex12-sit
  install: []
- id: conex12u-sit
  label: ConEx12u.sit
  description: >-
    A patch to bring ConEx 1.1 up to ConEx 1.2. If you dont already have ConEx1.1,
    download the complete ConEx 1.2 package, not this update.
  download_artifact: plugin-conex12u-sit
  install: []
- id: confed-crusier-fix
  label: Confed Crusier Fix
  description: >-
    Have you turned down EVs game speed only to discover your Confed Crusier wont
    turn anymore? This will fix it.
  download_artifact: plugin-confed-crusier-fix
  install: []
- id: cool-ships-are-added-ne-sit
  label: Cool_Ships_are_Added__NE.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-cool-ships-are-added-ne-sit
  install: []
- id: coolerbutton101
  label: CoolerButton101
  description: >-
    Cooler EV Buttons replaces the standard interface buttons in Escape Velocity
    with Pontus Ilbrings own version.
  download_artifact: plugin-coolerbutton101
  install: []
- id: corvettebay0-6b3-sit
  label: corvettebay0.6b3.sit
  description: >-
    Star Hackers Industrial Technologies is a new firm that specializes in
    extending existing technologies to unheard of limits and beyond. Runs fine with Velocity
    Plus 2.x, and may be OK with others.
  download_artifact: plugin-corvettebay0-6b3-sit
  install: []
- id: crazy-sit
  label: Crazy!.sit
  description: >-
    Crazy! is not your regular plug-in it adds the most absurd of ships and
    weapons. You can even fly around in a plunger while shooting Kestrels!
  download_artifact: plugin-crazy-sit
  install: []
- id: cssplug-in-sit
  label: CSSPlug-in.sit
  description: >-
    Ever wondered why Confed ships have the U.S.S prefix and not C.S.S.? Ever
    wondered if you were the only person playing EV who wasnt a patriotic American? Well,
    this plugin solves that. It changes the U.S.S. prefix to C.S.S.
  download_artifact: plugin-cssplug-in-sit
  install: []
- id: cydonianmissions-sit
  label: cydonianmissions.sit
  description: >-
    This is a set of six missions that you perform for the Cydonian Government. The
    mission series develops the plot of the Cydonian-Lethean war.
  download_artifact: plugin-cydonianmissions-sit
  install: []
- id: darkstorm1-02-sit
  label: darkstorm1.02.sit
  description: >-
    This creates the galaxy in my own image, with one new system. The systems have
    been better spaced and some of the obvious planet picture to descriptions have
    been made more congruent.
  download_artifact: plugin-darkstorm1-02-sit
  install: []
- id: davonia1-0-2-sit
  label: Davonia1.0.2.sit
  description: >-
    0.00 B | By Anonymous This is my first plug. It has 1 new planet and 1 new
    ship. If it messes up send your complaint (s) to
  download_artifact: plugin-davonia1-0-2-sit
  install: []
- id: deathstar2-9-8-sit
  label: deathstar2.9.8.sit
  description: >-
    Here comes the latest and greatest version on the DeathStar. Due to popular
    demand, here comes DeathStar 2.9.8 with improved everything new ships, worlds,
    missions, sounds and even a few new tricks. May the Force be with you!
  download_artifact: plugin-deathstar2-9-8-sit
  install: []
- id: deathstar20-sit
  label: DeathStar20.sit
  description: >-
    Death Star 2.0 plug package constists of several plugins that all go together.
    In Death Star 2.0 you can fly around in the extreamly strong Death Star wile
    destroying ships by the dozens with your super laser. This plug is well worth the
    download.
  download_artifact: plugin-deathstar20-sit
  install: []
- id: deathstarenv-sit
  label: deathstarenv.sit
  description: >-
    Integrates several plugs with DeathStar 2.0.1, required but not itself
    included. This includes Star Wars, Velocity Plus, raised limits to the DS scale and much
    more...
  download_artifact: plugin-deathstarenv-sit
  install: []
- id: decoy-sit
  label: decoy.sit
  description: >-
    If youre annoyed that the Decoy Flares are difficult to get to work, this
    plug-in adds a missle destroyer. Simply aim, point and click. No more missile!
  download_artifact: plugin-decoy-sit
  install: []
- id: delorean2-3-sit
  label: DeLorean2.3.sit
  description: >-
    This is the DeLorean Plug-In, which adds a few missions to EV, mainly to let
    you use different ships, and to get you a bunch of money. It also adds a new ship,
    called the DeLorean, based on the Back to the Future movies. It was created on a
    68LC040 Macintos...
  download_artifact: plugin-delorean2-3-sit
  install: []
- id: destroyer-bay
  label: Destroyer Bay
  description: "\"Destroyer Bay\" adds a fighter/bay for the Confederation. The Destroyer fighter (available only in bay form) is expensive, but quite powerful. It can only be purchased upon completion of the Confed Alien missions."
  download_artifact: plugin-destroyer-bay
  install: []
- id: destroyerwars
  label: DestroyerWars
  description: >-
    Destroyer Wars is when there are only destroyers and frigate class ships. But
    you had better be careful otherwise you just might get blast away!! Prepare for
    the fight of your life! Made by Alex Wouters. Have Fun!
  download_artifact: plugin-destroyerwars
  install: []
- id: destroyerwars1-1
  label: DestroyerWars1.1
  description: >-
    Destroyer Wars 1.1 is the upgrade from Destroyer Wars. It has a few bug fixes
    like when you take off from Levo you dont get shot down by pirate corvetts.
    Another fix is when you go to Rigel you can go back to Levo in one jump. So dont
    download the other one...
  download_artifact: plugin-destroyerwars1-1
  install: []
- id: destroyerwars2-1
  label: DestroyerWars2.1
  description: >-
    This is the next upgrade to Destroyer Wars. I really didnt have the problem
    with the linking of the systems fixed. A few other things have been added. Have Fun.
    Made By Alex Wouters. Oh and if the program crashes when you hyper into Apollo,
    it only happens...
  download_artifact: plugin-destroyerwars2-1
  install: []
- id: destroyerwars5-1
  label: DestroyerWars5.1
  description: >-
    Destroyer Wars 5.1 is the next upgrade to Destroyer Wars. This makes it
    impossible to win when you demand tribute of any planet. So be careful. And you must
    read the documents to know how to play. Have fun.
  download_artifact: plugin-destroyerwars5-1
  install: []
- id: destroyerwars5-2
  label: DestroyerWars5.2
  description: >-
    This Destroyer Wars is better graphicaly inclined. The systems only send out
    one ship now and the ship is more powerful than it used to be. E-mail me if you
    want me to change something. My E-mail address is . Please at all costs read the
    read me. Have a gre...
  download_artifact: plugin-destroyerwars5-2
  install: []
- id: devins-ev-programs-zip
  label: Devins EV Programs.zip
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-devins-ev-programs-zip
  install: []
- id: dialogexpander1-0-sea
  label: DialogExpander1.0.sea
  description: >-
    Dialog Expander 1.0 by Coraxus - This contains sets of plug-ins that expand the
    windows of dialog boxes when you are viewing the galaxy map, hailing a planet,
    or checking your player info. Each plug-ins are designed to cover various ranges
    from 640x480 to 1...
  download_artifact: plugin-dialogexpander1-0-sea
  install: []
- id: dilithium1-0-sit
  label: dilithium1.0.sit
  description: >-
    This adds 2 phaser weapons, a new power plant, and missions that allow you to
    buy the power plant.
  download_artifact: plugin-dilithium1-0-sit
  install: []
- id: ds9vsdominion-sit
  label: DS9vsDominion.sit
  description: >-
    Have you ever wanted to play a Star Trek plug and just fight? Then this plug is
    for you. Start off in the DS9 system, go to the bar and learn about the Dominion
    invasion comin your way. Take the Defiant along with a few of the Stations
    defences and try to d...
  download_artifact: plugin-ds9vsdominion-sit
  install: []
- id: e1-war-without-end-sea
  label: E1-WAR WITHOUT END.sea
  description: >-
    (version 2.0) This is the plug-in that started it all! As outlined previously,
    it centers on the conflict between the oppressive Terran Star Empire and a ragtag
    group of aliens known as The Weave who have formed a temporary alliance to fight
    off Empire anne...
  download_artifact: plugin-e1-war-without-end-sea
  install: []
- id: e1updater-sit
  label: E1updater.sit
  description: >-
    This is a simple update to E1-WAR WITHOUT END that repairs a problem with the
    Empire storyline that causes a certain fleet not to appear at a pivotal battle. If
    you are having problems with the mission "No Turning Back", simply install this
    update in the pl...
  download_artifact: plugin-e1updater-sit
  install: []
- id: e2-dark-horizons-sit
  label: E2-DARK HORIZONS.sit
  description: >-
    (version 1.0) This is a truly massive sequel to the plug-in EMPIRE, which
    centered on a ragtag group of alien sentients who formed an alliance to fight off
    domination by the evil Terran Star Empire. E2:DH takes place several years following
    an Empire victor...
  download_artifact: plugin-e2-dark-horizons-sit
  install: []
- id: e2updater-sit
  label: E2updater.sit
  description: "This updater performs some minor cosmetic fixes and repairs an out of order mission. To use, simply place the enclosed update in the same Plug-ins folder as EMPIRE 2: DARK HORIZONS. Please remove and trash any previous updater you may be using. At version 1..."
  download_artifact: plugin-e2updater-sit
  install: []
- id: e2updater-sit-2
  label: E2updater.sit
  description: "This updater performs some minor cosmetic fixes and repairs an out of order mission. To use, simply place the enclosed update in the same Plug-ins folder as EMPIRE 2: DARK HORIZONS. Please remove and trash any previous updater you may be using. At version 1..."
  download_artifact: plugin-e2updater-sit-2
  install: []
- id: e3-endgame-sit
  label: E3-ENDGAME.sit
  description: >-
    (version 1.0) In this scenario, the malevolent Terran Star Empire has been
    completely defeated by the Weave Alliance. After literally decades of war the galaxy
    is finally at peace, but all is not what it seems as new regional powers,
    revenge-minded factions...
  download_artifact: plugin-e3-endgame-sit
  install: []
- id: e3updater1-2
  label: E3updater1.2
  description: >-
    This updater is to replace the "cheater" already posted at Ambrosia which
    contains some problems. This fixes a mistake which made certain battlestations and
    sentinels indestructible and also performs some minor cosmetic fixes. To use, simply
    place the enclo...
  download_artifact: plugin-e3updater1-2
  install: []
- id: electronium-1-0-sit
  label: Electronium 1.0.sit
  description: >-
    This plug has no missions yet but it does have 5 new ships, 6 new weapons, and
    a system in the south west part of the map
  download_artifact: plugin-electronium-1-0-sit
  install: []
- id: elitefrontiers-sea
  label: EliteFrontiers.sea
  description: >-
    The Elite Frontiers Galaxy is much larger than the standard Escape Velocity
    one.There are three main govenments, two of which are at war. This war forms the
    core ofthe missions in Elite Frontiers.In addition though, there are several smaller
    independent gov...
  download_artifact: plugin-elitefrontiers-sea
  install: []
- id: elphan-iv1-1-sit
  label: Elphan Iv1.1.sit
  description: >-
    The rumbling of Georges World was discovered to be Alien weapons tests. The
    Confederation and Rebellion, aided by a mercenary captain, banded together and wiped
    out the Alien interlopers on Georges World. Scientists discovered a hyperspace
    fluxpoint off of...
  download_artifact: plugin-elphan-iv1-1-sit
  install: []
- id: empire-sea-sit
  label: Empire.sea.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-empire-sea-sit
  install: []
- id: empireofcrime-sea
  label: EmpireOfCrime.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-empireofcrime-sea
  install: []
- id: empirevsrebellion-sit
  label: EmpireVsRebellion.sit
  description: >-
    This plug-in is about the struggle of the Rebellion agianst the Empire. It
    centers you right in the middle of the conflict because there are no civilian ships.
    You must choose whether you want to join the Rebellion or the Empire. This plug
    replaces ALL of t...
  download_artifact: plugin-empirevsrebellion-sit
  install: []
- id: enhanced-buttons-sit
  label: Enhanced Buttons.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-enhanced-buttons-sit
  install: []
- id: enhancedstuff101
  label: enhancedstuff101
  description: "Enhanced Stuff by James Johnson, 8-29-2000This plug-in adds a few ships that are upgrades of ships already in EV. There are also some new outfits. Soon i hope to have some missions and even more ships and outfits. Web page:"
  download_artifact: plugin-enhancedstuff101
  install: []
- id: enterprise0-96-sit
  label: enterprise0.96.sit
  description: >-
    This plug adds the Starship Enterprise to the game. Now you too can pilot your
    very own Constellation Class Crusier (no TNG, thank you).
  download_artifact: plugin-enterprise0-96-sit
  install: []
- id: epicmissions1-22-sit
  label: epicmissions1.22.sit
  description: >-
    The Epic Missions is a mission set. A hard mission set. And a satisfying
    mission set. Version 1.22 fixes a bug where a mission could not be completed under EV
    1.02.
  download_artifact: plugin-epicmissions1-22-sit
  install: []
- id: era-a-02
  label: Era-A.02
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-era-a-02
  install: []
- id: escortcarrier11-sit
  label: EscortCarrier11.sit
  description: >-
    Escort Carrier 1.1 is an updater to EC 1.0. This version is mainly a fixer for
    all of the mistakes in EC 1 (like a red window on the ship). It also features a
    better target pic. Enjoy!
  download_artifact: plugin-escortcarrier11-sit
  install: []
- id: ev-ai-upgrade-sit
  label: EV AI Upgrade .sit
  description: >-
    This plug-in upgrades the Ship AI to EVO 1.0.2s AI. This makes the game much
    more challenging. The first thing is the fighters behave differently. They will
    pull off a few different manuvers on you and just be a lot smarter in general. Just
    prior to the rel...
  download_artifact: plugin-ev-ai-upgrade-sit
  install: []
- id: ev-expand-v1-0-sit
  label: EV expand v1.0.sit
  description: >-
    EV expand v1.0 is a plug that contains 2 new systems, 3 new stations and
    planets. It also has modified ship data so you can buy confed and rebel ships.Warning
    this plug is for confed haters ONLY!!.There are 3 new fleets some of which are for
    killing confeds...
  download_artifact: plugin-ev-expand-v1-0-sit
  install: []
- id: ev-expand-v1-1-sit
  label: EV expand v1.1.sit
  description: >-
    this is the next version of ev expand, it fixs a bug that stops you from buying
    the new ship thats in the plug it also lets you buy alien ships and weapons but
    at a high price so its not really a cheat. e mail me at if you have anything to
    say about my plug...
  download_artifact: plugin-ev-expand-v1-1-sit
  install: []
- id: ev-game-expander-1-0u2-sit
  label: EV Game Expander 1.0u2.sit
  description: >-
    (EVGE Second Update) Here is an all new scenario that I have put together for
    the game Escape Velocity. It does not completely change the game environment, but
    expands the existing one greatly. The Confeds and Rebels are still there, but
    theyve got company....
  download_artifact: plugin-ev-game-expander-1-0u2-sit
  install: []
- id: ev-graphics-sit
  label: EV GRAPHICS.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-graphics-sit
  install: []
- id: ev-plug-in-asteridian2-0
  label: EV Plug-in Asteridian2.0
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-plug-in-asteridian2-0
  install: []
- id: ev-radicalcolumn-3-0-sit
  label: EV RadicalColumn 3.0 .sit
  description: >-
    2/9/02NebulaEV RadicalColumn 3.0 is a small and fun plugin that replaces the
    games green side column with one made very radical colors!!!
  download_artifact: plugin-ev-radicalcolumn-3-0-sit
  install: []
- id: ev-super-helper-v2-0
  label: EV SUPER Helper v2.0
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-super-helper-v2-0
  install: []
- id: ev-targets-sit
  label: EV Targets.sit
  description: >-
    This plug-in does nothing but change the EV target graphics to a new form. The
    graphics are like those from Alpha Console, but are different in a way. Read the
    readme file included with the download for terms of use. -antihero
  download_artifact: plugin-ev-targets-sit
  install: []
- id: ev-borg-sit
  label: EV-Borg.sit
  description: >-
    This plugin adds two Federation starships (galaxy class, and intrepid class), a
    Borg cube, and several assimilated planets and stations. There are also a few
    missions to stop Borg incursions, and more will be added in the near future.
    Resistance is futile!
  download_artifact: plugin-ev-borg-sit
  install: []
- id: ev-sole-survivor-sit
  label: EV-Sole Survivor.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-sole-survivor-sit
  install: []
- id: ev-target-graphics-sit
  label: ev-target-graphics.sit
  description: "This plug-in replaces those boring, 2D cross-section views of ships with all-new, easy-to-read, full-color 3D views of the ship. A great enhancement. Available for Star Wars 1.0, Star Wars: ANA and Star Wars: ANA SE."
  download_artifact: plugin-ev-target-graphics-sit
  install: []
- id: ev-better-graphics
  label: Ev Better Graphics
  description: >-
    This plug GREATLY increases the quality of EVs horrible graphics. It might work
    with EVO (try it!) I did not do the graphics but merely used someone elses who
    said I could. David
  download_artifact: plugin-ev-better-graphics
  install: []
- id: ev-better-graphics-1-0-1-sit
  label: Ev_Better_Graphics_1.0.1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-better-graphics-1-0-1-sit
  install: []
- id: ev-data-str-fix
  label: EV Data STR Fix
  description: "There is a small bug in the EV Data file. In the STR# Resource #6001 a string - \"Confed\" - is missing between the strings \"UGE\" (#15) and \"Rebel\" (#16, should be #17), causing all strings after \"UGE\" to become out of sync. This patch fixes this.This patch w..."
  download_artifact: plugin-ev-data-str-fix
  install: []
- id: ev-destroyer-1-0-sea
  label: EV Destroyer 1.0.sea
  description: >-
    This plug adds 40 missions, 40 personalities, and 32 dudes. This plug-in is
    only for pilots that beat an alien mission. It does not seem to be much, but it is
    hard to beat. Ships begin to disappear around the Serpens Nebula, and questions
    about aliens came...
  download_artifact: plugin-ev-destroyer-1-0-sea
  install: []
- id: ev-destroyer-1-2-sea
  label: EV_Destroyer_1.2.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-destroyer-1-2-sea
  install: []
- id: ev-destroyer-1-4-sea
  label: EV Destroyer 1.4.sea
  description: >-
    1.4 is a big update on 1.2. It adds more ships, weapons, and a government.
    Expect some new missions by version 2.0.By Destroyer E.
  download_artifact: plugin-ev-destroyer-1-4-sea
  install: []
- id: ev-mods-sea
  label: EV_Mods.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-mods-sea
  install: []
- id: ev-solid-1-0a-sea
  label: EV solid 1.0a.sea
  description: >-
    solids main concept is to provide a more balanced gameplay, mainly by changing
    the universe and not adding to it. the player now has a more free choice between
    the two sides (rebels/confeds), all of their graphics were changed to fit the new
    atmosphere, new...
  download_artifact: plugin-ev-solid-1-0a-sea
  install: []
- id: ev-webboard-governments-s
  label: ev webboard governments.s
  description: >-
    Ever thought that EV just didnt have enough governments? Well geuss what, now
    it doesnt! Escape Velocity Webboard Governments was made by me, Nova6, but
    designed almost entirely by several other people on the Escape Velocity Webboard who
    submitted designs f...
  download_artifact: plugin-ev-webboard-governments-s
  install: []
- id: evadventures10-sit
  label: EVAdventures10.sit
  description: >-
    The idea here is to cram as much adventure and replay value into as little RAM
    as possible. Modified ships, weapons, outfits, dudes, fleets, and 48 new
    missions. None of the modifications unbalance the game because the bad guys get them too.
  download_artifact: plugin-evadventures10-sit
  install: []
- id: evbluecolumn1-sit
  label: EVBlueColumn1.sit
  description: >-
    EV BlueColumn is a very simple plugin for the game Escape Velocity, by Ambrosia
    Software. What EV BlueColumn does is it changes the look of the green and grey
    information column which is found on the right side of the screen during play to
    blue, green and b...
  download_artifact: plugin-evbluecolumn1-sit
  install: []
- id: evboom-sit
  label: EVBoom.sit
  description: >-
    EV Boom! is a simple plugin for Ambrosia Softwares Escape Velocity. It changes
    the explosion graphics in EV from those tiny little sparks to big dramatic
    fireballs. All you have to do is drop it in your "EV Plugs" folder and you ready to go!
    There are no kn...
  download_artifact: plugin-evboom-sit
  install: []
- id: evdeutsch103-sit
  label: EVDeutsch103.sit
  description: >-
    Sensation! EV Deutsch! Mein Plugin, bersetzt EV auf Deutsch - so gut es ein
    Plug eben kann - und beinhaltet zustzlich eine Menge berraschungen. Also dann,
    herunterladen, hinein in den EV plug-in Ordner und ausprobieren.
  download_artifact: plugin-evdeutsch103-sit
  install: []
- id: evenabler101-sit
  label: EVEnabler101.sit
  description: >-
    This plug enables ALL of the ships and outfits present in the stock EV. This
    means you can finally buy all those things only the computer had access to before.
    Each of the new items comes with re-drawn graphics, replacing that unattractive
    green "X", but al...
  download_artifact: plugin-evenabler101-sit
  install: []
- id: evfirebird-sit
  label: evfirebird.sit
  description: >-
    This Gravis Firebird control set is for use with EV. It was put together by a
    Kestrel pilot, and covers most of the keys used in combat. To install it, put the
    file in the "Firebird Sets" folder in your System folder.
  download_artifact: plugin-evfirebird-sit
  install: []
- id: evgeproxhelp-sit
  label: evgeproxhelp.sit
  description: "EVGE Players: Sick of waiting for the Crippled Bird-of-Prey to show up so that you can start the Proxima mission string? The EVGE Proxima Helper is for you. Just drop it into your plug-ins folder with EVGE, and youll be able to start the mission string righ..."
  download_artifact: plugin-evgeproxhelp-sit
  install: []
- id: evgovtfixer1-0-sit
  label: EVGovtFixer1.0.sit
  description: "How often has something like this happened to you: Youre right in the middle of a huge battle. You see several Rebel ships ganging up on a Confed Cruiser, so you move in for the killIts almost gone, when one of youre rockets misses and hits a Rebel Cruiser...."
  download_artifact: plugin-evgovtfixer1-0-sit
  install: []
- id: evmagma10-sit
  label: evmagma10.sit
  description: "EV : MAGMA is the Escape Velocity version of the very popular (read : over 15,000 downloads) EVO : MAGMA. MAGMA (Maximum Audio / Graphical Metamorphosis Addition) breaths new life into the game by re-doing Escape Velocitys graphics and sounds - theyre now r..."
  download_artifact: plugin-evmagma10-sit
  install: []
- id: evmissioncontrol-sea
  label: EVMissionControl.sea
  description: >-
    EV Mission Control allows you to manipulate EVs standard missions in many ways.
    Have you ever wanted to play the same mission twice? Well, with this plug-in you
    can...
  download_artifact: plugin-evmissioncontrol-sea
  install: []
- id: evnewdata-sit
  label: EVNewData.sit
  description: >-
    This is my first plug-in. It alters all ships and weapons in some way, shape
    and form. It also changes the government of some planets and systems, so download
    it and check it out! Newer versions might be on the way.
  download_artifact: plugin-evnewdata-sit
  install: []
- id: evoshipsetc2-0-sit
  label: EVOShipsEtc2.0.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evoshipsetc2-0-sit
  install: []
- id: evpluginpackage-sea
  label: EVPluginPackage.sea
  description: >-
    This package is meant for people who are interested in developing their own
    Escape Velocity plugins. It contains an FAQ, the EV Bible, resource templates, and
    more!
  download_artifact: plugin-evpluginpackage-sea
  install: []
- id: evplugs-sit
  label: evplugs.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evplugs-sit
  install: []
- id: evplus-sit
  label: EVPlus.sit
  description: >-
    EV Plus 1.0 is a plug-in that adds ONE HUNDRED new AI personalities to the
    original Escape Velocity! It is designed for people who want to expand the EV
    universe, without altering the basics. Its easy. Its incredibly cool. And will probably
    work with most o...
  download_artifact: plugin-evplus-sit
  install: []
- id: evtg-sw-sit
  label: evtg-sw.sit
  description: >-
    An EV Target Graphics for the amazingly cool Star Wars 2.0-2.0.5 plug-in. See
    EV Target Graphics for more information.
  download_artifact: plugin-evtg-sw-sit
  install: []
- id: evtg-swana-se-sit
  label: evtg-swana-se.sit
  description: "An EV Target Graphics for the all-new Star Wars: A New Alliance Special Edition ships plug-in. See EV Target Graphics for more information."
  download_artifact: plugin-evtg-swana-se-sit
  install: []
- id: evtg-swana-sit
  label: evtg-swana.sit
  description: "An EV Target Graphics for the all-new Star Wars: A New Alliance plug in. See EV Target Graphics for more information."
  download_artifact: plugin-evtg-swana-sit
  install: []
- id: evulas3dtargets-sit
  label: Evulas3DTargets.sit
  description: >-
    EVulas 3D Targets is, well, a plug with 3D target grafix done by me. I think
    they look pretty cool, as target pics go, but if I thought they looked dumb, I
    wouldnt be posting them on EV.com, now would I?
  download_artifact: plugin-evulas3dtargets-sit
  install: []
- id: evultra
  label: EVUltra
  description: "The Good News: You have managed to survive against the might of an alien warship. You boast all the time you enter a bar about your great battle and people never seem to get tired of you blabbing on. The Bad News: A Confederation Official crashes in on your..."
  download_artifact: plugin-evultra
  install: []
- id: extramissions1-01-sit
  label: extramissions1.01.sit
  description: "Extra Ships adds four unpurchasable ships: the Bulk Freighter, Executive Transport, Luxury Liner, and the Rebel Escort Carrier (availible w/completion of Rebel Alien Mission). It also provides these weapons: Swivel Laser Cannon, Rear Laser Turret, and the F..."
  download_artifact: plugin-extramissions1-01-sit
  install: []
- id: extraships1-2-1-sit
  label: extraships1.2.1.sit
  description: >-
    If you really want to know what I have done, I have made the Bulk Freighter,
    Luxury Liner,Exucutive Transport, and Hawk availible for purchase at many areas.
    Also the Rebel Escort Carrier is availible with the Rebel Alien mission. And now I
    have added some...
  download_artifact: plugin-extraships1-2-1-sit
  install: []
- id: extraships1-3-sit
  label: extraships1.3.sit
  description: >-
    Extra Ships unlocks previously unperchasable ships Bulk Freighter, Luxury
    Liner, Executive Transport, Hawk (now new and improved), and the Rebel Escort Carrier
    (availible after Rebel Alien Missions). Extra Ships also makes certain weapons
    availible Swivel L...
  download_artifact: plugin-extraships1-3-sit
  install: []
- id: eyeoforion13-sit
  label: EyeOfOrion13.sit
  description: >-
    Indiana Jones meets EV! Episodic campaign pack which includes new ships,
    worlds, systems, graphics, sounds, missions etc evil mummies, ancient tombs, mystic
    curses, grave robbers, black marketeers, malign murders, arcane artifacts,
    mysterious suicides, acad...
  download_artifact: plugin-eyeoforion13-sit
  install: []
- id: fair-boy-bin
  label: fair boy.bin
  description: >-
    From the makers of titan A.E comes somthing inkretabul.Basekley this plug
    modafies the hole game so donlwod it made by Joe Mama.
  download_artifact: plugin-fair-boy-bin
  install: []
- id: ffb2-sit
  label: FFB2.sit
  description: >-
    The long-awaited update to the early EV classic. Sixteen new ships, countless
    new outfits, and at least eighty new missions. Expanded Read-Me also. "One of
    these days theyll be your masters theyll do what it takes, theyre full-fledged
    bastards."
  download_artifact: plugin-ffb2-sit
  install: []
- id: fighterbaysgalore-sit
  label: FighterBaysGalore.sit
  description: >-
    This is a series of plugs that makes available ALL the fighter bays of the EV
    universe for purchase as well as adding the Defender Bay. The prices of the
    fighters and bays have been dropped, but not enough, in my opinion, to unbalance the
    game. Just enough...
  download_artifact: plugin-fighterbaysgalore-sit
  install: []
- id: final-battle-sit
  label: Final Battle .sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-final-battle-sit
  install: []
- id: finalbattle41
  label: finalbattle41
  description: "Final Battle expands EV and allows you to see the Rebel-Confed conflict through to its resolution. Be forwarned: when you have crushed the opposition, the battle has only begun."
  download_artifact: plugin-finalbattle41
  install: []
- id: flightacademy-sit
  label: flightacademy.sit
  description: >-
    Welcome to Flight Academy! Flight Academy is an EV Plug-in designed to teach
    new players how to fly, trade, and fight. Flight Academy will take you through a
    series of missions that will instruct you as you progress. You will experience
    using a number of sh...
  download_artifact: plugin-flightacademy-sit
  install: []
- id: floating-fortress-1-2-1-sea
  label: Floating Fortress 1.2.1.sea
  description: >-
    This plug-in adds an invinsible ship, three weapons, fighters, and a system
    with a tribute of 10 Million credits.
  download_artifact: plugin-floating-fortress-1-2-1-sea
  install: []
- id: floating-fortress-1-2-1-sea-2
  label: Floating Fortress 1.2.1.sea
  description: >-
    This is a plug-in a made a couple of years ago, but I never got around to
    posting it.The Floating Fortress plug adds a new ship (which is invincible), which
    looks like the ship in Ambrosias "Maelstrom." It also adds a new system, which has a
    really high tri...
  download_artifact: plugin-floating-fortress-1-2-1-sea-2
  install: []
- id: flying-high-1-5-sit
  label: Flying High 1.5.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-flying-high-1-5-sit
  install: []
- id: flying-high-2-0-sit
  label: Flying High 2.0.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-flying-high-2-0-sit
  install: []
- id: flying-high-2-1-1-sit
  label: Flying High 2.1-1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-flying-high-2-1-1-sit
  install: []
- id: flying-high-3-0-sit
  label: Flying High 3.0.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-flying-high-3-0-sit
  install: []
- id: flyinghigh1-7-sit
  label: FlyingHigh1.7.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-flyinghigh1-7-sit
  install: []
- id: forkliftmissions-sit
  label: ForkliftMissions.sit
  description: It lets you get the Forklift! Alex Wouters made it.
  download_artifact: plugin-forkliftmissions-sit
  install: []
- id: forkliftremover-sit
  label: ForkliftRemover.sit
  description: >-
    This tiny little plugin will allow you to take forklifts away from pilots that
    you spoiled by using the easter egg. (In my opinion, forklifts make the game less
    fun.) Contains both EV and EVO versions.
  download_artifact: plugin-forkliftremover-sit
  install: []
- id: foundationpackage14-sit
  label: FoundationPackage14.sit
  description: >-
    Foundation replaces the entire EV Galaxy. Foundation is set at the time of the
    Great Expansion, when two main groups, the Terrans and Martians, are in a
    struggle to colonize the galaxy through both peaceful means and war.
  download_artifact: plugin-foundationpackage14-sit
  install: []
- id: free-ev-zip
  label: free-ev.zip
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-free-ev-zip
  install: []
- id: freemen1-0-sit
  label: freemen1.0.sit
  description: >-
    Version 1.0 of Maurers Freemen contains 16 new systems, 19 new ports of call, 2
    new weapons, 1 new goverment, 3 new ships(the sprites are modified Ambrosia
    defaults), 12 custom PICTS for new worlds, and all new graphics for all of the
    planets and asteroids!
  download_artifact: plugin-freemen1-0-sit
  install: []
- id: funky-colored-sidebar-sit
  label: Funky-Colored Sidebar.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-funky-colored-sidebar-sit
  install: []
- id: gae-sit
  label: gae.sit
  description: >-
    Welcome back to the Alliance!Three new governments and one modified are
    included in this version. Along with land defenses, destructible planet, two AI pers, 46
    SPOBs, a new sound for the photon torpedoes, and over 70 missions! (You can see,
    my plugs are he...
  download_artifact: plugin-gae-sit
  install: []
- id: gae10b4f-sit
  label: GAE10b4F.sit
  description: >-
    As the Alliance struggles to contain the vicious onslaughts of the increasingly
    powerful NR Imperium, a new and devastating race enters the Galaxy - the
    dreaded, conquering Mechanicals that wiped out an earlier human race millenia before...
    This interim Fix...
  download_artifact: plugin-gae10b4f-sit
  install: []
- id: gae-landscape-sit
  label: GAE Landscape.sit
  description: "This file contains 9 BRAND NEW landscape graphics for Alliance worlds. Developed with the help of David Padula, this file is not essential to gameplay but will add flavor to it. WARNING: It will cost 3-4 megs of alloted EV memory to run."
  download_artifact: plugin-gae-landscape-sit
  install: []
- id: gaenhanced-sit
  label: GAEnhanced.sit
  description: >-
    Welcome back to the Alliance!Three new governments and one modified are
    included in this version. Along with land defenses, destructible planet, seven AI pers,
    81 SPOBs, new sounds for three weapons, and 128 missions, excluding the Mission
    Packs! (You can s...
  download_artifact: plugin-gaenhanced-sit
  install: []
- id: galacticall2-0p8-sit
  label: galacticall2.0p8.sit
  description: "This is my third plug and best of the pack. Note: this is a total rewrite of the 1.x versions. And Please remove all previous versions upon satisfactory of this version. This plug adds five new governments and modified the Merchant government, the Galactic..."
  download_artifact: plugin-galacticall2-0p8-sit
  install: []
- id: galacticexpress1-01-sit
  label: galacticexpress1.01.sit
  description: >-
    The first plugin in the Alliance Trilogy. Galactic Express is a new shipping
    company, it specializes in large shipments of very high priced cargo. But it has a
    secret...
  download_artifact: plugin-galacticexpress1-01-sit
  install: []
- id: galacticinsanity-sit
  label: GalacticInsanity.sit
  description: >-
    Its just not the same anymore, so get ready for the craziest Escape Velocity
    plug-in yet! Galactic Insanity introduces things like your favorite video games
    into the EV universe. Its a totally new game the moment you launch it. GI features
    tons of new ships...
  download_artifact: plugin-galacticinsanity-sit
  install: []
- id: galacticjavind1-0-sit
  label: galacticjavind1.0.sit
  description: >-
    The GJI or Galactic Javeline Industries is a company run haphazardly in the
    THX-something system, which is just above Palshife. They are allied to the rebels
    and research new technologies, most of them blowing up in their faces.At this time
    the plug include...
  download_artifact: plugin-galacticjavind1-0-sit
  install: []
- id: goldilocks10b2-sit
  label: Goldilocks10b2.sit
  description: >-
    This plugin is the first in a series of plugins Im working on, which revolve
    aroung Stegz Research Industries. They will be good to you at first, but theres
    something behind the scenes that will shock you... It has 1 new ship, 4 outfits, 15
    missions (all lo...
  download_artifact: plugin-goldilocks10b2-sit
  install: []
- id: goodshiplp1-31-sit
  label: goodshiplp1.31.sit
  description: >-
    An undocumented Bug has been going around that people cannot impress systems no
    matter how hard they try. Most of these reports i have noticed came from the
    news group comp.sys.mac.games.action. This plug-in will hopefully make up for the
    loss of time and e...
  download_artifact: plugin-goodshiplp1-31-sit
  install: []
- id: gozerla-sit
  label: Gozerla.sit
  description: >-
    Super powerful weapons with a cruiser to match. While the weapons may be strong
    Ive tried not to un-balance the EV universe so all the weapons are large or
    expensive. The Gozerlas great for holding the weapons because of the size of Its
    weapons bays..
  download_artifact: plugin-gozerla-sit
  install: []
- id: great-war-1-1
  label: Great War 1.1
  description: >-
    The Great War is a little different than my Alien Empire add-on. This makes the
    Confeds have the center and the Alien Empire have the outside of the galaxy.
    There arent any Alien Missions. Alex Wouters made this super add-on.
  download_artifact: plugin-great-war-1-1
  install: []
- id: greatcivilwar1-2-sit
  label: greatcivilwar1.2.sit
  description: "Includes three new ships: Confederate Dreadnaught, Rebel Dreadnaught, and Rebel Flounder. Also a few new missions, and a few surprises."
  download_artifact: plugin-greatcivilwar1-2-sit
  install: []
- id: greatwar0-sea
  label: GreatWar0.sea
  description: >-
    They came without warning. A few months after the start of the Great
    War,humanity has been reduced to several core systems and worlds. As a young Navypilot, you
    must lead the attack against the murderous Aliens. Your actions will leadto
    victory... or extinc...
  download_artifact: plugin-greatwar0-sea
  install: []
- id: gs-sit
  label: GS.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-gs-sit
  install: []
- id: gsto145-sit
  label: GSto145.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-gsto145-sit
  install: []
- id: gundrones1-31-sit
  label: gundrones1.31.sit
  description: >-
    Gundrones are small, protable, fully robotic proton turents which let a combat
    pilot drop next to an enemy. Give the order to fire and up to 15 gundrones will
    destroy the target.
  download_artifact: plugin-gundrones1-31-sit
  install: []
- id: heartofdarkness-sit
  label: HeartOfDarkness.sit
  description: "HEART OF DARKNESS (version 1.0) This is the \"grand finale\" to the Empire Trilogy, whose previous episodes included Empire, E2: Dark Horizons, E3: Endgame and Empire of Crime. In this instalment, the oppressive Terran Star Empire controls the galaxy with an..."
  download_artifact: plugin-heartofdarkness-sit
  install: []
- id: helper-ev
  label: Helper EV
  description: >-
    This outfits all ships with more weapons. The confederates and rebels have
    their own turrets. Have Fun!! Alex Wouters
  download_artifact: plugin-helper-ev
  install: []
- id: hft1-1-sit
  label: hft1.1.sit
  description: "A combination of four custom designed weapons, to give you some added firepower above and beyond the stock stuff. 4 brand new weapons for EV: 2 Projectile, 1 Laser, 1 Missle."
  download_artifact: plugin-hft1-1-sit
  install: []
- id: hodupdater1-1-sit
  label: HODupdater1.1.sit
  description: "HEART OF DARKNESS UPDATER: This simple fix updates HoD to version 1.1, repairing a pair of minor NON-FATAL bugs that may cause confusion to the player. Weave long range missiles are now secondary weapons (as opposed to primary) and both the Empire blockade..."
  download_artifact: plugin-hodupdater1-1-sit
  install: []
- id: hornet10-sit
  label: Hornet10.sit
  description: >-
    Hornet 1.0 adds a new race, the Hornets, to the EV Galaxy. The Hornets, using
    converted ConFed and Rebel ships, have allied with the Rebels in their fight for
    independence. Both the Rebels and the ConFeds have also converted enemy ships.
    This plug also intr...
  download_artifact: plugin-hornet10-sit
  install: []
- id: hostiletakeover1-sit
  label: HostileTakeover1.sit
  description: >-
    Hostile Take-over is a largish plug-in for EV 1.0.5. It adds 15 new ships, 9
    new weapons, 6 new outfits, 11 new or modified governments, 6+ modified systems, 2
    modified landing picts, several new or modified dudes and fleets, and a whole
    mess o missions - m...
  download_artifact: plugin-hostiletakeover1-sit
  install: []
- id: hsifados2-4-sit
  label: hsifados2.4.sit
  description: >-
    The system HSif Ados is north of THX-something or other (Which is northwest of
    Satori) It has some neat stuff, but there are no new items, ships, or anything of
    the sort. There are missions, but Im not sure if they work yet. (Too bad, wait
    for the next vers...
  download_artifact: plugin-hsifados2-4-sit
  install: []
- id: hyperionproject12-sit
  label: HyperionProject12.sit
  description: >-
    Hyperion Inc. is a company that makes advanced starfighters and weaponry. This
    plug-in for Escape Velocity will let you test drive some of its products, like
    the Hyperion fighter, an assortment of plasma weapons, and the deadly ion torpedo
    launcher.
  download_artifact: plugin-hyperionproject12-sit
  install: []
- id: inherentgovt1-sit
  label: InherentGovt1.sit
  description: >-
    This small plug changes the inherent governments of the ships in EV. This means
    Confeds wont automaticlly attack ANY Rebel ship. (although they will still
    attack Rebel governed ships). No longer will you have to put up with annoying Confeds
    constantly attac...
  download_artifact: plugin-inherentgovt1-sit
  install: []
- id: interfaceenhancer-sit
  label: InterfaceEnhancer.sit
  description: >-
    This is a plug-in that I havent seen around since the old EV site, but its
    possibly the best interface enhancement I have seen yet besides Magma so I am
    uploading it here.-LoneIgadzra
  download_artifact: plugin-interfaceenhancer-sit
  install: []
- id: jamesfond3-sea
  label: JamesFond3.sea
  description: >-
    Welcome to Her Majestys Sequined Service. Agent 711, your next mission will
    involve... The James Fond Plug-in is designed to combine irreverent humor and satire
    with a series of interesting and challenging missions.
  download_artifact: plugin-jamesfond3-sea
  install: []
- id: janiss-plugin-1-0-sea
  label: Janiss Plugin 1.0 .sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-janiss-plugin-1-0-sea
  install: []
- id: jet092-sit
  label: Jet092.sit
  description: >-
    This is the beginnings of a new plug for Escape Velocity. So far it includes
    only 3 ships (jet plane, bi-plane, space shuttle) and 1 weapon (machine gun).
  download_artifact: plugin-jet092-sit
  install: []
- id: josie-maran-sit
  label: Josie Maran.sit
  description: >-
    2/9/02NebulaThis plugin is a fun plugin for Escape Velocity dedicated to super
    model Josie Maran that I created in my spare time. It introduces 1 new planet, 3
    new ships and some interesting fun in the Levo system!!!
  download_artifact: plugin-josie-maran-sit
  install: []
- id: jumpnow-sea
  label: JumpNow.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-jumpnow-sea
  install: []
- id: kestrelcollection-sea
  label: KestrelCollection.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-kestrelcollection-sea
  install: []
- id: kestrelplus-sit
  label: KestrelPlus.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-kestrelplus-sit
  install: []
- id: kingfisherind1-1-sit
  label: kingfisherind1.1.sit
  description: >-
    KFI focuses on the mega-corporation Kingfisher Industries and its attempts to
    survive and prosper in the dangerous times of civil war. The player can join the
    company and run missions...
  download_artifact: plugin-kingfisherind1-1-sit
  install: []
- id: klingonandfederationwar-sit
  label: KlingonAndFederationWar.sit
  description: >-
    This plug adds about 7 systems, 5 planets, 5 new ships with 2 new sprites, 2
    opposing governments, 2 new outfits 5 sprites(pictures), and most ships and outfits
    are available.
  download_artifact: plugin-klingonandfederationwar-sit
  install: []
- id: ksv-entreprises-sit
  label: KSV Entreprises.sit
  description: >-
    KSV Entreprises fights for resources and food against the SSV Corporation. This
    plug-in adds 3 new ships, 5 new weapons, 16 outfits and two systems.
  download_artifact: plugin-ksv-entreprises-sit
  install: []
- id: kys-sit
  label: Kys.sit
  description: >-
    Ky Industries was desighned to give the people what they want such as powerful
    weapons and sheilds. This is just a small plug no systems or missions, but if you
    really suck at this game youll like this plug.
  download_artifact: plugin-kys-sit
  install: []
- id: laser-beams-and-upgrades-sit
  label: Laser Beams and Upgrades.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-laser-beams-and-upgrades-sit
  install: []
- id: lbe1-1-02-sit
  label: lbe1-1.02.sit
  description: >-
    LBE Enterprises, a new outfitter company, has chaosen to make its mark though
    the release of several new, useful weapons and the Cobra Strike-Bomber. Part 1 of
    the LBE Trilogy.
  download_artifact: plugin-lbe1-1-02-sit
  install: []
- id: legion1-52-sit
  label: legion1.52.sit
  description: >-
    Currently it adds one world (Legion Central), The Legion government, The Legion
    Fleet, 8 missions (including the two from The Legion 1.0), a plot that will
    eventually be thicker than tar, and some other alterations that you will find out
    about through the m...
  download_artifact: plugin-legion1-52-sit
  install: []
- id: levofix-sit
  label: LevoFix.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-levofix-sit
  install: []
- id: light-sit
  label: light.sit
  description: "Arena Plus Light is an upgrade to Arena, a battle simulator. This fixes some bugs and adds some reviews (of ships and outfits). Look for Arena Plus soon!NOTE: The maker of Arena IS NOT the maker of Arena Plus Light! For comments, questions, etc. ASK ME, not..."
  download_artifact: plugin-light-sit
  install: []
- id: lightningjump-sit
  label: lightningjump.sit
  description: >-
    Lightning Jump is a simple utility-type plug-in for either Escape Velocity or
    EV Override. This small but useful goodie physically speeds up your hyperjumps so
    that youre in the next system by the time your finger lifts off the J key. This
    does not affect t...
  download_artifact: plugin-lightningjump-sit
  install: []
- id: lone-cruiser-plug-sit
  label: Lone_Cruiser_plug.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-lone-cruiser-plug-sit
  install: []
- id: lotsobays10-sit
  label: LotsoBays10.sit
  description: >-
    This plugin adds a bay for each type of ship lacking one. It also adds fighters
    to fill the bays. Sorry, it doesnt add bays already incorporated into the game
    (lightning, alien fighter, manta, etc). It does, however, add graphics that I cut
    pasted myself....
  download_artifact: plugin-lotsobays10-sit
  install: []
- id: lovelyhunsruck-sit
  label: LovelyHunsruck.sit
  description: >-
    This is a small plug with funny and difficult missions (dont expect the usual
    go-and-kill). If you manage to solve the riddles, you are rewarded with the
    permission to dock at the Satellite Of Love!! As a special feature you might find some
    evil Windows(TM)...
  download_artifact: plugin-lovelyhunsruck-sit
  install: []
- id: lproof-sea
  label: LProof.sea
  description: >-
    This plugin was an effort to try to end the Confeds vs. Rebels dispute in
    one-on-one combat- I find that Rebels tend to win.~Jormungand
  download_artifact: plugin-lproof-sea
  install: []
- id: mach-sit
  label: Mach.sit
  description: >-
    This is version 1.0 of Mach, a new plug-in for EV. Currently, it adds 6 systems
    that can be found connected to Darven. These systems are filled with Pirates,
    each one harder than the last. Mach 6 even includes 3 new ships- the Pirate
    Fighter, Destroyer, and...
  download_artifact: plugin-mach-sit
  install: []
- id: maelstrom-15-plugin-sit
  label: Maelstrom-15-plugin.sit
  description: Maelstrom for EV adds four ships from Ambrosias classic, Maelstrom.
  download_artifact: plugin-maelstrom-15-plugin-sit
  install: []
- id: maelstrom1-01-sit
  label: maelstrom1.01.sit
  description: "This plug-in adds a new ship, the Maelstrom Mark IV shuttlecraft - borrowed from another Ambrosia game: Maelstrom 1.4.3."
  download_artifact: plugin-maelstrom1-01-sit
  install: []
- id: magma-sit
  label: Magma.sit
  description: This plug-in introduces two new Magma weapons, each highly devestating.
  download_artifact: plugin-magma-sit
  install: []
- id: megaplugin1-1-sit
  label: megaplugin1.1.sit
  description: >-
    The Mega Plug-In is a huge plug-in for EV. It adds 2 systems, 8 missions, 5
    weapons, 4 ships, the kitchen sink, and various other goodies.
  download_artifact: plugin-megaplugin1-1-sit
  install: []
- id: merchant-sea
  label: Merchant.sea
  description: >-
    I created this plug-in called Merchant. This allows you to start off in a light
    freighter instead of a shuttlecraft. Now youll actually have fun being a
    merchant. It also has some really cool stuff to trade.
  download_artifact: plugin-merchant-sea
  install: []
- id: merchantworlds2-sit
  label: MerchantWorlds2.sit
  description: >-
    A major update to Merchant Worlds. In addition to previously mentioned things
    this update adds a lot of new ships, systems and missions. The galaxy is also more
    crowded with ships than in standard EV.
  download_artifact: plugin-merchantworlds2-sit
  install: []
- id: missionimpossible1-4-sit
  label: missionimpossible1.4.sit
  description: >-
    It includes a bunch of new ships, new weapons, worlds to explore, sounds and 14
    new missions. The main missions start on Luna, any pirate planet, and Ma Bell
  download_artifact: plugin-missionimpossible1-4-sit
  install: []
- id: missionpatch10-sit
  label: MissionPatch10.sit
  description: >-
    I have been playing EV for a long time. I had finally achieved my final
    goal...I received the "Investigate Disappearances" mission. It was for the Confeds. I
    went through the cycle of missions, but couldnt destroy the alien cruiser. So, I
    cheated, using my...
  download_artifact: plugin-missionpatch10-sit
  install: []
- id: missionsgalore2-0-sit
  label: missionsgalore2.0.sit
  description: >-
    This is actually not one plug in but a collection of plugins. Each one is a
    seperate string of Missions.At this time there 2 plug-ins, Rebel Spy and Clean Slate.
  download_artifact: plugin-missionsgalore2-0-sit
  install: []
- id: mmbay-sit
  label: mmbay.sit
  description: >-
    This plug in adds a few new ship bays including a Frigate, Destroyer, and
    Rapier bay. It also adds a new ship.
  download_artifact: plugin-mmbay-sit
  install: []
- id: modified-strings-sit
  label: Modified strings.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-modified-strings-sit
  install: []
- id: moreevvoices-sit
  label: moreevvoices.sit
  description: >-
    This Escape Velocity plug-in was hand-crafted by the same person who produced
    the fighter speech for EV. These sounds are simply additional voices to add a
    little variety to the game.
  download_artifact: plugin-moreevvoices-sit
  install: []
- id: mostwanted1-0-sit
  label: mostwanted1.0.sit
  description: >-
    For serious EV players only. It has extremely difficult missions that may take
    you a while to finish (try killing 3 Alien Cruisers on the first try).
  download_artifact: plugin-mostwanted1-0-sit
  install: []
- id: mti1-sit
  label: MTI1.sit
  description: >-
    Weird uniforms, a new company and its problems. Join MTI in their battle
    against nasty pirates and the large S.P.A.F. Pirates are the only guys you can legally
    shoot so... hunt them! Watch out for part II with much more action and battles!
  download_artifact: plugin-mti1-sit
  install: []
- id: mugabi-and-destiny-sit
  label: Mugabi and Destiny.sit
  description: >-
    Mugabi And Destiny begins where the Alien storylines in Escape Velocity
    conclude. Following your fabulous success against the Alien Cruiser menace, you search
    for new adventure, and answers to the mysteries of your past. You will see things
    you have never s...
  download_artifact: plugin-mugabi-and-destiny-sit
  install: []
- id: mugabi-and-destiny-her-sit
  label: Mugabi and Destiny her.sit
  description: >-
    Mugabi And Destiny... for her begins where the Alien storylines in Escape
    Velocity conclude. Following your fabulous success against the Alien Cruiser menace,
    you search for new adventure, and answers to the mysteries of your past. You will
    see things you h...
  download_artifact: plugin-mugabi-and-destiny-her-sit
  install: []
- id: mxplug-sit
  label: mxplug.sit
  description: >-
    The Meowx Plug is a classic old plug - the first Meowx ever made and released -
    that adds a new system, new ships and weapons, some new missions, and more.
  download_artifact: plugin-mxplug-sit
  install: []
- id: mysterious-force-sit
  label: Mysterious_Force.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-mysterious-force-sit
  install: []
- id: navallightnings-sit
  label: NavalLightnings.sit
  description: >-
    Tired of watching your expensive Lightnings dieing? Want to expand your Manta
    bay so you can use something besides Mantas? Naval Lightnings is for you! It isnt
    really a cheat, because pirates and mercenaries will use them too! Prices are
    reasonable, and I k...
  download_artifact: plugin-navallightnings-sit
  install: []
- id: navallightnings102-sit
  label: navallightnings102.sit
  description: >-
    Naval Lightnings, By Far World Software and Brian Whalley Tired of watching
    your lightnings get shredded in combat? They are now stronger, and they have a new
    bay, so that you can just go and upgrade your ship, no worries. They still cost
    lots of money, and...
  download_artifact: plugin-navallightnings102-sit
  install: []
- id: neptron15-sit
  label: Neptron15.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-neptron15-sit
  install: []
- id: nerdsidebar-sit
  label: NerdSidebar.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-nerdsidebar-sit
  install: []
- id: neutronic-kestrel-sea
  label: Neutronic Kestrel.sea
  description: >-
    For those who are hooked on the famous Neutronic Kestrel Clavius Interceptor
    and wish to keep it to play other EV plug-ins, Neutronic Kestrel 1.1 simply adds
    this ship and all its neutronic weapons. So, this shortened plug-in should be
    easily compatible wit...
  download_artifact: plugin-neutronic-kestrel-sea
  install: []
- id: neutronturret-sit
  label: NeutronTurret.sit
  description: >-
    This plug adds the neutron turret, a turreted and improved form of the neutron
    blaster.
  download_artifact: plugin-neutronturret-sit
  install: []
- id: new-letheancydonian-ships-sit
  label: New LetheanCydonian ships.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-new-letheancydonian-ships-sit
  install: []
- id: new-ships-sit
  label: New ships.sit
  description: >-
    Enough of seeing always-the-same, dumb, in open market Lethean and Cydonian
    ships? This plug-in make them use one new ship each, inspired from 2 they were using
    before. Not brand-new graphics, but new graphics already. You cant buy them now
    (but you can eas...
  download_artifact: plugin-new-ships-sit
  install: []
- id: newbars-sit
  label: NewBars.sit
  description: >-
    Put one Plug (Coldbar for example) into your Plug-In´s folder and start
    EV.Because only few Plug´s change the right bars, you should now have a new bar on the
    right.You can only use one new bar at a time.
  download_artifact: plugin-newbars-sit
  install: []
- id: newbrodania101-sit
  label: NewBrodania101.sit
  description: >-
    This plug-in currently adds 1 new ship, 1 new government, 1 new system, 1 new
    planet, 2 new people and a bunch of fleets.
  download_artifact: plugin-newbrodania101-sit
  install: []
- id: newcanada1-00-sit
  label: newcanada1.00.sit
  description: >-
    This plug-in adds a new system, a new planet, a new government, and a few
    little extras. Works with version 1.0.2 and up.
  download_artifact: plugin-newcanada1-00-sit
  install: []
- id: newendorp-sit
  label: Newendorp.sit
  description: >-
    This plugin adds 1 outfit, some dudes, and many systems. You might find a Rebel
    Super Cruiser, but I cant get it to work. This is my first plugin so it is small.
  download_artifact: plugin-newendorp-sit
  install: []
- id: newhorizons-sit
  label: newhorizons.sit
  description: >-
    The galaxy is awaiting- Sociology and technology are never stagnant. Even on a
    galactic scale they change with time. Over the years the civil war has worn down
    the Confederation through isolation and hit and run tactics. The once mighty
    Confederation fleet...
  download_artifact: plugin-newhorizons-sit
  install: []
- id: newmilitia0-b3-sit
  label: newmilitia0.b3.sit
  description: "Adds a solar system at the far top right of the galaxy: New Militia Prime, New Militia Minor, New Militia Major and New Militia Station."
  download_artifact: plugin-newmilitia0-b3-sit
  install: []
- id: newplanets1-03-sit
  label: newplanets1.03.sit
  description: >-
    This is a patch for my "New Planet and Ships" plug-in which fixes a bug which
    would sometimes corrupt the weapons on the Leviathan.
  download_artifact: plugin-newplanets1-03-sit
  install: []
- id: newtechnology1-0-sit
  label: newtechnology1.0.sit
  description: >-
    This plug currently adds 6 ships, 16 weapons, 2 outfits and one system. No new
    graphics were created. This plug-in is compatible with EV 1.04 and up.
  download_artifact: plugin-newtechnology1-0-sit
  install: []
- id: nfsa-1-0-sit
  label: NFSA 1.0.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-nfsa-1-0-sit
  install: []
- id: nfsa-1-2-1-sit
  label: NFSA 1.2.1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-nfsa-1-2-1-sit
  install: []
- id: ninnyman10-sit
  label: ninnyman10.sit
  description: >-
    This plug adds the Ninnymann Fighter to the EV Universe. There are versions for
    both EV and EVO. Submitted by Mouse (Ambrosia SW Webboards).
  download_artifact: plugin-ninnyman10-sit
  install: []
- id: nod1-0b1-sit
  label: nod1.0b1.sit
  description: >-
    Strange new ships have arrived in the Serpens Nebula, plundering everything in
    sight, including each other. Is this a new alien threat...?
  download_artifact: plugin-nod1-0b1-sit
  install: []
- id: nova-bracket-warning-sit
  label: nova_bracket_warning.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-nova-bracket-warning-sit
  install: []
- id: obsidianbuttons-sea
  label: ObsidianButtons.sea
  description: "Obsidian Buttons is a pair of free plugins: one for Escape Velocity called \"EV Obsidian Buttons\" and one for EV Override called \"EV Override Obsidian Buttons.\" They both provide replacement graphical buttons for the respective games you may prefer them to t..."
  download_artifact: plugin-obsidianbuttons-sea
  install: []
- id: omegafaction1-01-sit
  label: omegafaction1.01.sit
  description: >-
    As the war raged on between the Confederacy and the Rebellion, many systems
    soon grew alienated by the constant fighting and pressure to take sides. These
    systems chose to remain as independant, autonomous systems without alleigences to
    either side. However...
  download_artifact: plugin-omegafaction1-01-sit
  install: []
- id: onyx1-01-sit
  label: onyx1.01.sit
  description: >-
    Explore several new star systems and planets, and aid the new Onyxian Alliance
    in its struggle for survival against marauding pirates ...
  download_artifact: plugin-onyx1-01-sit
  install: []
- id: oreste-4-1-sea
  label: Oreste 4.1.sea
  description: >-
    This plug-in has been written for those who have completed the missions of
    Escape Velocity and want to get a new world, with new ships, weapons, graphics and
    missions.Generally you meet tougher opponents when completing the missions, and
    more ships and weap...
  download_artifact: plugin-oreste-4-1-sea
  install: []
- id: otp-sit
  label: otp.sit
  description: >-
    A Plugin with 4 missions, 2 weapions and one new ship, with origanal 3D
    graphics.The new ship, The Orbital turret, is a mostly fixed position ship that packs a
    punch.This plug will, if you "Play your cards right" Make a system change
    goverments.The Missions...
  download_artifact: plugin-otp-sit
  install: []
- id: pale1-9-sit
  label: Pale1.9.sit
  description: >-
    The Civil War continues as a reluctant new political player is thrust into
    intergalactic politics. An independent world determined to maintain its identity
    engages in astro-physical research of areas beyond the Fringe. The Lethe-Cydonian War
    hurtles toward...
  download_artifact: plugin-pale1-9-sit
  install: []
- id: paletitles-sit
  label: paletitles.sit
  description: This plugin adds two new "Title pictures" for the PALE plugin.
  download_artifact: plugin-paletitles-sit
  install: []
- id: paralleluniverse-sit
  label: paralleluniverse.sit
  description: >-
    The third version of the Parallel Universe plugin. It changes the normal EV
    Universe to a parallel one, where the Great War between the humans and aliens never
    ended. It is a mess and doesnt work okay.
  download_artifact: plugin-paralleluniverse-sit
  install: []
- id: parrots-sit
  label: parrots.sit
  description: >-
    This Plugin will make it so you can buy parrots at Merlin and sell it at New
    Columbia to make a huge profit. Both of these planets are in the Tau Ceti system.
  download_artifact: plugin-parrots-sit
  install: []
- id: pdainc503-sit
  label: PDAInc503.sit
  description: >-
    P.D.A. Inc. 5.0.3 adds one new ship, a wide variety of new weapons and outfits,
    new systems and docks, new governments, and a bundle of missions in which you
    help James Bond XIV overcome and destroy the evil Privateers.
  download_artifact: plugin-pdainc503-sit
  install: []
- id: pdaincforward105-sit
  label: PDAIncForward105.sit
  description: >-
    If youve played any previous P.D.A. Inc.s youll notice that this one has all of
    them combined into one plug-in and has improved graphics for most things (also
    includes 3D target graphics all made by me except for the target graphics for the
    original EV ship...
  download_artifact: plugin-pdaincforward105-sit
  install: []
- id: pegasusmissions1-1-sit
  label: pegasusmissions1.1.sit
  description: >-
    The Perseus system is frequently attacked by pirate ships. Trader ships from
    Pegasus is often boarded. While the neighbor Blackthorne station is mysteriously
    growing rich. Is there a connection?"
  download_artifact: plugin-pegasusmissions1-1-sit
  install: []
- id: people1-sit
  label: People1.sit
  description: >-
    People has been released containing over 250 actual people. The names are good
    and the prices are fair when boarding. It also contains extra features such as a
    new side screen.
  download_artifact: plugin-people1-sit
  install: []
- id: personalitiesplug-sit
  label: PersonalitiesPlug.sit
  description: >-
    Has one completely new ship, all the original EV ships are now modified, theres
    also 5 new outfits, including 2 weapons, a shield, a fuel scoop, and galactic
    map, enjoy!
  download_artifact: plugin-personalitiesplug-sit
  install: []
- id: persons-of-evwebboard-sea
  label: Persons of EVwebboard.sea
  description: >-
    Ever wanted to be in the Escape Velocity universe? Well now you can! Persons of
    the EV Webboard adds 20 or so new persons, all based on people who visit the
    Escape Velocity webboards!! The only other thing this plug-in adds is an Ambrosia
    government used on...
  download_artifact: plugin-persons-of-evwebboard-sea
  install: []
- id: phoenixcorvette1-12-sit
  label: phoenixcorvette1.12.sit
  description: >-
    Has one completely new ship, all the original EV ships are now modified, theres
    also 5 new outfits, including 2 weapons, a shield, a fuel scoop, and galactic
    map, enjoy!
  download_artifact: plugin-phoenixcorvette1-12-sit
  install: []
- id: pilots-helper-sit
  label: pilots_helper.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-pilots-helper-sit
  install: []
- id: pilotutilitypack-sit
  label: PilotUtilityPack.sit
  description: "Përsonal Flyer:If you create a pilot while this plug is active, you should be able to use the same pilot in most other plugs. (Note: The plugin doesnt have to stay in your plugin folder all the time, only when making new pilots).Ajax Pilot: This will clear..."
  download_artifact: plugin-pilotutilitypack-sit
  install: []
- id: pinkbeard-the-pirate-sit
  label: Pinkbeard the Pirate.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-pinkbeard-the-pirate-sit
  install: []
- id: planetarydefenses1-2-sit
  label: planetarydefenses1.2.sit
  description: >-
    Planetary Defenses modifies the standard EV planets and stations defense dudes,
    so that there will be ground forces defending their home, as well as the defense
    fleet.
  download_artifact: plugin-planetarydefenses1-2-sit
  install: []
- id: planetsplug
  label: PlanetsPlug
  description: >-
    This is my first plug. It adds more planets and some different system names. I
    hope to add ships and systems and outfits in the future so keep your eyes open
    for updates and i hope you enjoy my plug. thanks!
  download_artifact: plugin-planetsplug
  install: []
- id: planetsplug2000
  label: PlanetsPlug2000
  description: >-
    Sorry about last time...i forgot to add what its about...:) Planets Plug 2000
    adds some more systems, new planets in existing systems, some weapons, and lot o
    ships...some which need the alien missions done...thanx again and look for
    updates!!
  download_artifact: plugin-planetsplug2000
  install: []
- id: q-merch-alphor-1-0-0-sit
  label: Q_Merch__Alphor_1.0.0..sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-q-merch-alphor-1-0-0-sit
  install: []
- id: qumre104-sit
  label: qumre104.sit
  description: >-
    The Quantumire Trilogy is a total conversion for EV. It has been avalible from
    meowx.com and a MacAddict CD for a while now, but never from the Ambosia web
    site. This plug creates all-new ships, weapons, missions - everything. EV is a whole
    new game now. Ve...
  download_artifact: plugin-qumre104-sit
  install: []
- id: qumre104u-sit
  label: qumre104u.sit
  description: >-
    If you have The Quantumire Trilogy v1.0.2 or v1.0.3, you can download this plug
    in to update your copy to version 1.0.4, which is Quantumires final version.
  download_artifact: plugin-qumre104u-sit
  install: []
- id: raiders1-2-sit
  label: raiders1.2.sit
  description: >-
    Avast ye scurvy scum. Come about and prepare to be boarded. The Raiders have
    joined with the pirates. Pirate ships are harder to defeat, but more booty when you
    do. 10 ships, 7 weapons, 2 non weapon outfits, 4 systems, numerous fleets,
    missions etc.
  download_artifact: plugin-raiders1-2-sit
  install: []
- id: rangers1-05-sit
  label: rangers1.05.sit
  description: >-
    The Rangers are the colonization and exploration arm of the Confederation.
    Under-funded and lacking ships, they tend to hire a few free-lancers...
  download_artifact: plugin-rangers1-05-sit
  install: []
- id: raptor1-2-sit
  label: raptor1.2.sit
  description: >-
    New features include a new government, four new ships and two new fleets. No
    longer compatible with 68 plug-ins. This version fixes a number of bugs.
  download_artifact: plugin-raptor1-2-sit
  install: []
- id: realmofprey-sit
  label: RealmOfPrey.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-realmofprey-sit
  install: []
- id: rebel-command
  label: Rebel command
  description: >-
    This is just an litle download tell me if it works I,am not sure i use this
    edit 20.1 the first time and i had a lot off dificulties but for me it works now .It
    alters the rebel Cruiser and destroyer you can buy it also now your self and
    fight with the rebe...
  download_artifact: plugin-rebel-command
  install: []
- id: rebel-upgrade-sit
  label: Rebel Upgrade.sit
  description: >-
    There are many programs that say "do you hate seeing the rebels geting crushed
    by the cofeds" but dont work ". Well this one works and all it does is upgrades
    the rebel cruiser and some weapons and now the rebels dont get crushed by the
    confeds. Download an...
  download_artifact: plugin-rebel-upgrade-sit
  install: []
- id: rebelalliance1-50-sit
  label: rebelalliance1.50.sit
  description: "This plug-in adds substantially to the EV universe. A quick run down of what it adds: 16-misns 12-new dudes 10-modified dudes 50-pers 9-ships 6-outfs 4-weaps 50-systs, 14 which are hidden 38-spobs, 1-new bar in existing spob 6-govts 9-new or modified flets."
  download_artifact: plugin-rebelalliance1-50-sit
  install: []
- id: rebelbattleship-sit
  label: RebelBattleship.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-rebelbattleship-sit
  install: []
- id: rebeldreadnought1-0-sit
  label: rebeldreadnought1.0.sit
  description: A Rebel version of a Confed Cruiser
  download_artifact: plugin-rebeldreadnought1-0-sit
  install: []
- id: rebelescortcarrier-sea
  label: RebelEscortCarrier.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-rebelescortcarrier-sea
  install: []
- id: rebelescortcarrier1-0-1-sea
  label: RebelEscortCarrier1.0.1.sea
  description: >-
    Changes the coloured bands on the Escort Carrier to red, makes the Escort
    Carrier available for purchase after the Rebel Alien mission (if you want if for some
    reason), adds the Escort Carrier to the normal game (2 düdes and 1 flët), adds a
    few përs Escort...
  download_artifact: plugin-rebelescortcarrier1-0-1-sea
  install: []
- id: rebels-plug-1-0-0-sea
  label: Rebels_Plug_1.0.0.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-rebels-plug-1-0-0-sea
  install: []
- id: rebelstriker-sit
  label: rebelstriker.sit
  description: >-
    The Rebel Striker is a bit more powerful than the Gunboat, being based on a
    Rapier. Also includes a fixed version of my Rapier Bay.
  download_artifact: plugin-rebelstriker-sit
  install: []
- id: recession1-0-sit
  label: recession1.0.sit
  description: >-
    A very simple plug that reduces the base prices of goods in EV 1.0.1 so that
    trading is not so lucrative.
  download_artifact: plugin-recession1-0-sit
  install: []
- id: red-nose-ships-sit
  label: Red Nose Ships.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-red-nose-ships-sit
  install: []
- id: reddwarf-sit
  label: RedDwarf.sit
  description: >-
    This is the plug-in of the hit British comedy Red Dwarf. This is a must have
    for all Red Dwarf fans. It has a brand new start up and music, a new ship, the Star
    Bug, new wepons and much more.
  download_artifact: plugin-reddwarf-sit
  install: []
- id: redplug-sit
  label: RedPlug.sit
  description: >-
    A plugin I made years ago but didnt submit because I hadnt yet purchased Ray
    Dream Studio, on which I had made it. It was for someone who would always play EV
    on my friends computer. His name was Eric. Things only happen in Levo
  download_artifact: plugin-redplug-sit
  install: []
- id: return10-sit
  label: return10.sit
  description: "Quantumire: Return is the continuation of the Quantumire series. It takes place after the Trilogy. This is version 1.0."
  download_artifact: plugin-return10-sit
  install: []
- id: ropupdater1-2-sit
  label: ROPupdater1.2.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ropupdater1-2-sit
  install: []
- id: rsf0-99b1-sit
  label: rsf0.99b1.sit
  description: >-
    RSF is a new shipping company, Rebel Shipping Force, located on RSF HQ in the
    system Guron, which is in between Satori and Clotho. Lots of missions.
  download_artifact: plugin-rsf0-99b1-sit
  install: []
- id: rumblingplanet1-1-sit
  label: rumblingplanet1.1.sit
  description: Bug fix and update. Final version
  download_artifact: plugin-rumblingplanet1-1-sit
  install: []
- id: saab10-sit
  label: saab10.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-saab10-sit
  install: []
- id: salvager-sit
  label: Salvager.sit
  description: >-
    This plugin adds one new ship that will allow you to disable freighters and
    steal all their cargo and ammo. The ship has 20,000 tons of cargo space and about
    the same weapon space.
  download_artifact: plugin-salvager-sit
  install: []
- id: sateliteoflove-sit
  label: SateliteOfLove.sit
  description: >-
    The Satelite of Love plug-in provides the much sought after, often discussed
    but never seen (in the game), Satelite of Love. The are two files one with an
    associated mission and the other as a stand alone spob in case of compatability
    restraints due to othe...
  download_artifact: plugin-sateliteoflove-sit
  install: []
- id: satori-station-sit
  label: Satori_Station.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-satori-station-sit
  install: []
- id: satori-station-update-sit
  label: Satori_Station_Update.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-satori-station-update-sit
  install: []
- id: scoutship-sit
  label: Scoutship .sit
  description: >-
    This plug-in is pretty simple. It removes the shuttlecraft from the game,
    replacing it with the scoutship. It also replaces the scoutships spot with a larger
    version of the scoutship, and changes the graphics for the scoutship and defender.
    It also alters s...
  download_artifact: plugin-scoutship-sit
  install: []
- id: seeker1-0-sit
  label: seeker1.0.sit
  description: >-
    The Seekers are a new race (non-human) who have mysteriously appeared in the
    NGC-8724 system. Contains 1 new government, 8 ship outfits, 3 ships w/altered
    graphics, 1 system, 2 stellar objects, 11 missions, and 16 personalities.
  download_artifact: plugin-seeker1-0-sit
  install: []
- id: shipsgalore-1-12-sea
  label: ShipsGalore 1.12.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-shipsgalore-1-12-sea
  install: []
- id: shipsgalore-2-5-sea
  label: ShipsGalore 2.5.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-shipsgalore-2-5-sea
  install: []
- id: shipsgalore-ii-1-0-cpt
  label: ShipsGalore II 1.0.cpt
  description: >-
    Its ready! I have worked overtime on this, but I managed it in the end! This is
    the sequel to ShipsGalore- ShipsGalore II. In this, I have replaced all the
    ships of ShipsGalore with new ones. Weapons stay the same, save for some being
    upgraded. There are 34...
  download_artifact: plugin-shipsgalore-ii-1-0-cpt
  install: []
- id: shuttles-vs-freighters-sit
  label: Shuttles_VS_Freighters.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-shuttles-vs-freighters-sit
  install: []
- id: si-sr2
  label: SI SR2
  description: >-
    The SR2 is a fighter/interceptor with lots of weapons space and almost no cargo
    space. It comes pre-equipped with two Proton Cannons a Javelin Pod and a new
    secondary weapon I call the Plasma Array. It should be available at most shipyards
    for the low, low...
  download_artifact: plugin-si-sr2
  install: []
- id: smv-sit
  label: SMV.sit
  description: >-
    Shawn Michaels is back, and whether you love wrestling or hate it, Shawn
    Michaels Velocity is sure to put a smile on your face. Lovingly crafted with Snapz,
    PICS to SPRITE, Graphic Converter, AppleWorks, Animation Creator, ResEdit, Amadeus
    and a ploretha of...
  download_artifact: plugin-smv-sit
  install: []
- id: spaceballs-ev
  label: Spaceballs EV
  description: "\"Spaceballs EV\" is a small plug-in for fans of the movie \"Spaceballs.\" It adds two planets in a new sytem one jump from Sol. I suggest you shop at the new planets. ) Made with EVO Developers Map by Luke."
  download_artifact: plugin-spaceballs-ev
  install: []
- id: spacerocks1-0-sit
  label: spacerocks1.0.sit
  description: >-
    This plugin replaces all the original planets and asteroids graphics with some
    ones that we created. Developed using Alias and SGI. Check it out!
  download_artifact: plugin-spacerocks1-0-sit
  install: []
- id: spam1-0-sit
  label: spam1.0.sit
  description: >-
    Plug this sucker in for 24 new missions with detailed storylines characters, 3
    new "governments," a new system with custom graphics and sounds, a good dose of
    tongue-in-cheek humor, and lots and lots and LOTS of spam!
  download_artifact: plugin-spam1-0-sit
  install: []
- id: sparklien1-0b-sit
  label: sparklien1.0b.sit
  description: >-
    This plugin adds a new alien fighter ship, as wel as a particle beam outfit
    weapon.
  download_artifact: plugin-sparklien1-0b-sit
  install: []
- id: spartus1-1-sea
  label: Spartus1.1.sea
  description: >-
    Spartus 1.1- A plug in for Escape Velocity that gives all the regular EV ships
    entirely diffrent, including graphics, stats, weapons, and discriptions. It also
    adds new weapons, and even a new person. -Madman-
  download_artifact: plugin-spartus1-1-sea
  install: []
- id: spartus1-2-sit
  label: Spartus1.2.sit
  description: >-
    Spartus 1.2- Spartus 1.2 gives every ship in the original EV universe a new
    look, new stats, weapons, and a whole bunch of other stuff. It also adds other
    things,including goverments, weapons, planets, and a few surprises! Well worth the
    download. -Madman
  download_artifact: plugin-spartus1-2-sit
  install: []
- id: spartus1-3-sit
  label: Spartus1.3.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-spartus1-3-sit
  install: []
- id: spartus1-5-sit
  label: Spartus1.5.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-spartus1-5-sit
  install: []
- id: ssemulator1-0-sit
  label: ssemulator1.0.sit
  description: >-
    This plugin is designed to make Escape Velocity sound better, it replaces many
    of the original sounds with pseudo-surround sounds.
  download_artifact: plugin-ssemulator1-0-sit
  install: []
- id: st-the-plug-in-55b1-sit
  label: ST, The Plug In .55b1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-st-the-plug-in-55b1-sit
  install: []
- id: star-wars-ana-68k
  label: Star Wars-ANA 68k
  description: >-
    68K Version -- A long time ago, in a galaxy far ,far away...It is a time of
    great turmoil in the galaxy . The Emperor is gone, defeated at the Battle of Endor,
    but theImperial Fleet lives on to continue his legacy of tyranny. The Empire,
    massing its mighty...
  download_artifact: plugin-star-wars-ana-68k
  install: []
- id: star-wars-ana-ppc
  label: Star Wars-ANA PPC
  description: >-
    PowerPC Version -- A long time ago, in a galaxy far ,far away...It is a time of
    great turmoil in the galaxy . The Emperor is gone, defeated at the Battle of
    Endor, but theImperial Fleet lives on to continue his legacy of tyranny. The Empire,
    massing its mig...
  download_artifact: plugin-star-wars-ana-ppc
  install: []
- id: starclashcv10
  label: StarclashCv10
  description: "This is a port of the Plug-in I made for Nova. It adds the Protoss Carrier and Terran Battle Cruiser to EV. Expand with StuffIt 6.5.1. contact me:"
  download_artifact: plugin-starclashcv10
  install: []
- id: starfighters-sit
  label: Starfighters.sit
  description: >-
    Starfighters adds five new ships and two new weapons to the EV universe, along
    with dudes and personalities to make them a part of the game. The ships include a
    light, medium, and heavy fighter, a light warship, and a highly upgradable
    merchant cruiser.
  download_artifact: plugin-starfighters-sit
  install: []
- id: starfx1-sea
  label: StarFX1.sea
  description: >-
    StarFX is a new power in the galaxy. Located in the southeast of the galaxy,
    they control more than you may think. Ally with the rebellion, they are a force to
    be reckoned with. 6 ships, 5 systems, 7 stellars, 1 gov. 1 dude, 2 fleets, 2
    pers. and new intro...
  download_artifact: plugin-starfx1-sea
  install: []
- id: starwars2-01-sit
  label: starwars2.01.sit
  description: >-
    This version features 17+ accurately 3D rendered Star Wars ships, weapons,
    sounds, planets, graphics and people and two more new ships.
  download_artifact: plugin-starwars2-01-sit
  install: []
- id: starwars2-05upd-sit
  label: starwars2.05upd.sit
  description: "The hopefully final version: more missions, fixes bugs, adds a TIE Advanced, an E-Wing, and a really cool AT-AT for Captain Hector that walks as it turns."
  download_artifact: plugin-starwars2-05upd-sit
  install: []
- id: starwarsdeathstar-sit
  label: starwarsdeathstar.sit
  description: >-
    This is a very cool, very large graphic to supplement the plugin Star Wars
    along with some cool new X-Wing graphics. However, make sure you have a lot of RAM.
  download_artifact: plugin-starwarsdeathstar-sit
  install: []
- id: starwarsfix1-sit
  label: starwarsfix1.sit
  description: >-
    This is a fix for the plugin "Star Wars!" for those who are having problems
    with the graphics. This fixes the Bulk Freighter, Corvette, Death Star, Mon
    Calamari, and Star Destroyer.
  download_artifact: plugin-starwarsfix1-sit
  install: []
- id: starwarsfix2-sit
  label: starwarsfix2.sit
  description: >-
    This is also a fix for the plugin "Star Wars!" for those who are having
    problems with the graphics. This fixes problems that were not previously known about
    with the Nebulon B Frigate and the Rebel Transport.
  download_artifact: plugin-starwarsfix2-sit
  install: []
- id: stev-sit
  label: STEV.sit
  description: >-
    STEV was made by Troy and Trent. It is the best Star Trek plug-in for EV today.
    It includes all the ships you know and love about the Star Trek universe. Also
    all correct solar systems in the Star Trek universe. This plug-in does not have
    any missions, whic...
  download_artifact: plugin-stev-sit
  install: []
- id: sting-and-hive
  label: Sting and Hive
  description: "\"Sting and Hive\" adds a fragile but swift fighter, the \"Sting\". It is available only in bay form. The bay, nicknamed \"Hive\", can hold up to 16 of the fighters, and the fighters and bay are quite cheap."
  download_artifact: plugin-sting-and-hive
  install: []
- id: strife-teaser-sit
  label: strife-teaser.sit
  description: >-
    This is a teaser version of a full plugin to be released sometime in summer
    1998. It features three new ships with all new graphics, plus two weapons, and one
    fleet. The final version of Strife will be much larger and will feature a richer
    gaming enviroment.
  download_artifact: plugin-strife-teaser-sit
  install: []
- id: super-pirates-sit
  label: Super Pirates.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-super-pirates-sit
  install: []
- id: supershield-1-1-sit
  label: Supershield 1.1.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-supershield-1-1-sit
  install: []
- id: swedishkeyboard-sit
  label: swedishkeyboard.sit
  description: >-
    998.00 B | By Anonymous So, EV 1.0.2 is here. I was asked to UL this plug when
    it was released. It fixes a foreign keyboard bug, but ONLY WITH SWEDISH
    KEYBOARDS. Enjoy.
  download_artifact: plugin-swedishkeyboard-sit
  install: []
- id: switch-sides-1-1-cpt
  label: Switch Sides 1.1.cpt
  description: >-
    Fixes the rebel Switch Sides mission so that it is no longer a dead end by
    adding a Switch Sides II mission. Also adds similar missions for the Confeds.
  download_artifact: plugin-switch-sides-1-1-cpt
  install: []
- id: switchgovt1-00-sit
  label: switchgovt1.00.sit
  description: "Contains two plug-ins: \"Switch to Rebellion\" and \"Switch to Confederation.\" These allow you to change your alliance from Confed to Rebel and vice-versa."
  download_artifact: plugin-switchgovt1-00-sit
  install: []
- id: swoopv1-0-1-sit
  label: Swoopv1.0.1.sit
  description: Swoop is based on the Ambrosia SWs game, Swoop. Only three ships are installed.
  download_artifact: plugin-swoopv1-0-1-sit
  install: []
- id: swtge-sea
  label: swtge.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-swtge-sea
  install: []
- id: t-g-w-sit
  label: T.G.W.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-t-g-w-sit
  install: []
- id: talon11u-sit
  label: Talon11u.sit
  description: >-
    To update Talon to 1.1, copy the Talon  1.1 Update file into your EV Plug-Ins
    folder. Do not remove the Talon plugin file. Also, do not remove the Talon  1.1
    Update unless you want to stop playing Talon, as that will downgrade Talon back to
    1.0. Tal...
  download_artifact: plugin-talon11u-sit
  install: []
- id: talon18-sit
  label: talon18.sit
  description: >-
    As the Civil War rages on, independent small traders and corporations became
    tired of being caught in the middle of battles not their own, and having no
    protection from the ever-increasing pirate attacks. A number of them came together and
    formed an organis...
  download_artifact: plugin-talon18-sit
  install: []
- id: targetenhancer105r2-sit
  label: TargetEnhancer105r2.sit
  description: >-
    An alternative to EV Target Graphics, Target Enhancer 1.05 adds a black lining
    around the targeted ship. It also uses the newest graphics for the Alien ships
    unlike EV Target Graphics.
  download_artifact: plugin-targetenhancer105r2-sit
  install: []
- id: taxi-sit
  label: Taxi.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-taxi-sit
  install: []
- id: terran-rebellion-folder
  label: Terran Rebellion Folder
  description: >-
    This add-on changes the way you look at EV! After I looked at my Alien Empire
    Add-on I decided to change it totally, so I changed it to Terran Rebellion. This
    add-on has better graphics, a whole new map, one new mission, new weapons, and
    cool ships. So down...
  download_artifact: plugin-terran-rebellion-folder
  install: []
- id: terran-rebellion-1-1
  label: Terran Rebellion 1.1
  description: >-
    A whole new add-on and a new way to look at Escape Velocity! A new galaxy to
    explore, new governments, a new mission, new outfits, and new or modified ships.
    This add-on is similar to my Alien Empire except that it is in a whole new galaxy!
    This is only the...
  download_artifact: plugin-terran-rebellion-1-1
  install: []
- id: terran-rebellion-2-0
  label: Terran Rebellion 2.0
  description: >-
    This is truely a whole new way to look at EV. This one can support that
    statement unlike my last Terran Rebellion I put out, because this one has a whole new
    interface and new ships and better governments(you dont get into trouble by one if
    you shoot a non...
  download_artifact: plugin-terran-rebellion-2-0
  install: []
- id: terrannaval1-0-sit
  label: terrannaval1.0.sit
  description: >-
    This little plug-in fixes the bug that prevents you to purchase Confed warships
    after you had done the Confed alien mission. Just go to Luna and click on the
    shipyard and the Confed warships in the shipyard. BUT YOU STILL HAVE TO DO THE
    MISSIONS BEFORE THEY...
  download_artifact: plugin-terrannaval1-0-sit
  install: []
- id: terrawattsg-sit
  label: terrawattsg.sit
  description: >-
    The terrawatt shield generator is an experimental device created by Bonsai
    Industries. It uses matter/antimatter converters to generate power into the terrawatt
    range, considerably boosting shields.
  download_artifact: plugin-terrawattsg-sit
  install: []
- id: thamahawk-37-morpher
  label: thamahawk 37 morpher
  description: >-
    8.00 B | By Anonymous This is a small plug-in and holds only a ship. The ship
    is named thamahawk 37 morpher and can also morph to a smaller ship. Its fast and
    has good cargo and costs only 6mill so why not download now??
  download_artifact: plugin-thamahawk-37-morpher
  install: []
- id: the-year-of-the-rebels-2-0-sea
  label: The Year of the Rebels 2.0.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-the-year-of-the-rebels-2-0-sea
  install: []
- id: the-grinchians1-0-sit
  label: The Grinchians1.0.sit
  description: >-
    This plug is a high-quality plug, meaning that each element thereof has been
    lovingly crafted and carefully thought out (usually in that order). For starters,
    you wont find any silly omissions, such as missing graphics in the shipyard or
    outfit shop. In ad...
  download_artifact: plugin-the-grinchians1-0-sit
  install: []
- id: the-sickness-sea
  label: The Sickness.sea
  description: >-
    This plug-in adds two new stellars, a few new governments, a new mission that
    allows you to buy the other merchant ships, and two systems that you can only get
    to by acomplishing the mission. The mission is hard though. Download if you like
    to die, but ther...
  download_artifact: plugin-the-sickness-sea
  install: []
- id: the-weaponator
  label: The Weaponator
  description: >-
    This makes a weapon thats as cheap as a coke and as heavy as a pepsi and as
    powerful as a machine gun
  download_artifact: plugin-the-weaponator
  install: []
- id: thebladez-sit
  label: TheBladez.sit
  description: >-
    A far off group of aliens has come to the Rebellions side to help the fight
    against the evil Confederation scums, join them! It has 3 ships, a few missions, no
    weapons, no graphics, and no sound.
  download_artifact: plugin-thebladez-sit
  install: []
- id: thecygnusalliance-sit
  label: TheCygnusAlliance.sit
  description: >-
    Welcome to the alternate reality of your own. A place where what happened for
    you doesnt happen here. In this universe, the Confederates are meaner, the
    Alliance is fighting a two sided war, and an enemy from the past is back for more. With
    about 30 new mis...
  download_artifact: plugin-thecygnusalliance-sit
  install: []
- id: thefrontier-sea
  label: TheFrontier.sea
  description: >-
    Confederation was a test and a new alien threat still awaits.... this plugin
    adds a number of new ships, outfits, and missions, centering around "The Frontier"
    border of known and unknown space.
  download_artifact: plugin-thefrontier-sea
  install: []
- id: theprivateers102
  label: theprivateers102
  description: >-
    This plug adds quite a lot of things:-2 new governement (The Privateers and The
    NSF Aliens)-5 new ships (6 if you count a very special one youll encounter only
    once)-3 new weapons-1 new outfits-about eight systems (I think, but only 3 with
    planets or statio...
  download_artifact: plugin-theprivateers102
  install: []
- id: theprivateersreadme-sit
  label: ThePrivateersReadMe.sit
  description: "This is not a new plugin. I was checking ev site to see if there still was a plugin I made a long time ago. And lo, there was one: The Privateers. It fairs not so bad and I checked it again and saw that the read me was not accurate. So here is an update of..."
  download_artifact: plugin-theprivateersreadme-sit
  install: []
- id: thevisitordemo-sit
  label: TheVisitorDemo.sit
  description: >-
    This demo allows you to purchase five new ships from any tech level 3 planet or
    station, at the small price of only 1000 credits! Each ship has its new weapons,
    which can be restocked at any tech level 2 planet or station, at the even
    smaller price of 500 c...
  download_artifact: plugin-thevisitordemo-sit
  install: []
- id: tiefighter1-02-sit
  label: tiefighter1.02.sit
  description: >-
    This is an entirely new ship for EV... it performs like the Defender but better
    in basically every area, and costs a tad more. Its a pirate-operated ship. _Sky_
    did the graphics.
  download_artifact: plugin-tiefighter1-02-sit
  install: []
- id: titan-a-e
  label: Titan .A.E
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-titan-a-e
  install: []
- id: torgo-and-beyond-messed-up
  label: Torgo And Beyond Messed Up
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-torgo-and-beyond-messed-up
  install: []
- id: torgo-and-beyond-spobs
  label: Torgo and Beyond Spobs
  description: >-
    These are the planets, stations, and systems currently in Torgo and Beyond. I
    make no guarantees.
  download_artifact: plugin-torgo-and-beyond-spobs
  install: []
- id: torture-sit
  label: torture.sit
  description: "This plug changes the bounty hunters to the most feared ship in the game. EV: Changes bounty hunter from Kestrel to Alien Cruiser. EVO: Changes bounty hunter from Crescent Warship to Voinian Dreadnought. Dont download unless you have no life! Youve been war..."
  download_artifact: plugin-torture-sit
  install: []
- id: turinvpatch-sit
  label: TurinVPatch.sit
  description: >-
    Turin V Patch By Far World Software / Brian Whalley Turin V Patch is a small
    plugin that fixes one of the more annoying elements of EVs scenario. It will stay
    dormant untill the player finishes the Turin V terraforming missions. At that
    time, the planet wil...
  download_artifact: plugin-turinvpatch-sit
  install: []
- id: tweakit-v1b-sit
  label: TweakIt_v1B.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-tweakit-v1b-sit
  install: []
- id: uglyplug0-5b3-sit
  label: uglyplug0.5b3.sit
  description: >-
    This plugin contains 3 ships and 2 weapons with original graphics. The release
    is soon to come. To get the UgLyHeLL, change the price and availibility with
    ResEdit.
  download_artifact: plugin-uglyplug0-5b3-sit
  install: []
- id: ultimateevdata1-0-sit
  label: UltimateEVData1.0.sit
  description: >-
    This is probably the greatest cheat ever. This alternate EV Data and EV
    Graphics includes 2 new conflicting governments, 3 new ships, 3 new outfitts, and quite
    a few highly upgraded ships. The Confed/Rebel ships have 10,000 armor and 20,00
    shields, and the...
  download_artifact: plugin-ultimateevdata1-0-sit
  install: []
- id: usw-sit
  label: USW.sit
  description: This plugin allows you to purchase ships that are normally unpurchasable.
  download_artifact: plugin-usw-sit
  install: []
- id: utopia0-999-sit
  label: utopia0.999.sit
  description: >-
    Creates 2 new governments, 4 new systems with several planets, 5 new ships plus
    2 weapons, several new personalities and 12 exciting new missions. Now
    Stormbringer compatible!
  download_artifact: plugin-utopia0-999-sit
  install: []
- id: valhalla1-sit
  label: Valhalla1.sit
  description: >-
    The Valhalla Plug-In adds three new ships and twelve new outfits. The ships and
    outfits dont replace any of the original resources from EV v1.0.5. The new ships
    and outfits have there own graphics and no copys of the originals.
  download_artifact: plugin-valhalla1-sit
  install: []
- id: valkyrie1-1-sit
  label: valkyrie1.1.sit
  description: >-
    Valkyrie adds one ship, the Atinoda IVX-4 Valkyrie, to EV. The Valkyrie is
    based on the Kestrel, but is closer to a massively upgraded Clipper in its multirole
    design.
  download_artifact: plugin-valkyrie1-1-sit
  install: []
- id: variety
  label: variety
  description: >-
    You dont keep the outfits that came with your ship, so why should the computer?
    Variety adds more variety amoung ships, so youll run into Kestrels without
    fighter bays and with space bombs, Corvettes with fighter bays, Argosys with Proton
    Bolts, and more as...
  download_artifact: plugin-variety
  install: []
- id: varsis-zip
  label: Varsis.zip
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-varsis-zip
  install: []
- id: velocityplus2-5-sit
  label: velocityplus2.5.sit
  description: >-
    Velocity Plus Plugin is a large plugin that adds a number of new and unique
    ships and weapons to Escape Velocity. A new series of missions are also on the way.
  download_artifact: plugin-velocityplus2-5-sit
  install: []
- id: viper-sit
  label: viper.sit
  description: >-
    After reading the EV bible I decided to design my first ship. The Viper is the
    result. If you like this kind of ship-animation idea, please email me with your
    opinion.
  download_artifact: plugin-viper-sit
  install: []
- id: vorlons-sit
  label: vorlons.sit
  description: >-
    This plug-in is inspired by the Babylon 5 Vorlon alien race. At present it is
    not yet finished but you can download it and try it out.
  download_artifact: plugin-vorlons-sit
  install: []
- id: vpk10-sit
  label: vpk10.sit
  description: >-
    Vorlon Planet-Killer adds the Vorlon ship as seen in the popular TV series
    Babylon 5.
  download_artifact: plugin-vpk10-sit
  install: []
- id: walker0-42b2-sit
  label: walker0.42b2.sit
  description: >-
    Has two systems (one invisible), dudes, two persons, two govts, three missions,
    two outfits, and one weapon. New govt. is Atland, and come from the hidden
    system.
  download_artifact: plugin-walker0-42b2-sit
  install: []
- id: warriors-sit
  label: warriors.sit
  description: >-
    Warriors is very neat plug-in with lots of new weapons and starships. (This
    plug is only compatible with version Escape Velocity 1.0.5!)
  download_artifact: plugin-warriors-sit
  install: []
- id: weapon-plug-sea
  label: Weapon-plug.sea
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-weapon-plug-sea
  install: []
- id: wicked0-b9-sit
  label: wicked0.b9.sit
  description: >-
    This plug adds the following:Destroyer Beam Turret, Tractor Beam Puller, Galaxy
    Map, Impossible Armor, some missions and the planet Kal-El.
  download_artifact: plugin-wicked0-b9-sit
  install: []
- id: williams1-52-sit
  label: williams1.52.sit
  description: >-
    Adds the Williams system (near Perseus), the Mafia, 25 new missions, a new
    Alien race with ships/weapons (purchasable), and some high level pirate missions.
  download_artifact: plugin-williams1-52-sit
  install: []
- id: winthewar-cpt
  label: WinTheWar.cpt
  description: >-
    Put your ultimate combat rating to the ultimate test and Win the War! Run 30
    exciting missions and watch the hated Confederation disappear planet-by-planet
    forever! New ships, new weapons, and new governments complete the picture. Thanks to
    B B shipyard for...
  download_artifact: plugin-winthewar-cpt
  install: []
- id: xenhancments-sit
  label: xenhancments.sit
  description: >-
    XcomputerMissions, XlightningBay, XmassExpansion, XshieldCapcitor and
    XshipUpgrades. Makes standard missions to have a stronger pirate part in them.
  download_artifact: plugin-xenhancments-sit
  install: []
- id: ydwtk1-0-sit
  label: YDWTK1.0.sit
  description: >-
    This plug-in features totally different combat ratings and legal statuses. It
    beefs up all rebel ships. The Demonfire borrows the Kestrels graphic, but can win
    against one Confed Cruiser. There is a totally new "ship" called the Turret Mine,
    that is placed...
  download_artifact: plugin-ydwtk1-0-sit
  install: []
- id: yellow-sit
  label: Yellow.sit
  description: >-
    This is my first plugin. very simple and just for fun, this plug changes the
    rebel cruiser and destroyers colours so they are now yellow instead of red.10/9/02
    Virmor
  download_artifact: plugin-yellow-sit
  install: []
- id: your-private-cruisers-sea-sit
  label: Your_Private_Cruisers.sea.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-your-private-cruisers-sea-sit
  install: []
- id: yster1-00-sit
  label: yster1.00.sit
  description: >-
    Ever since the civil war began, engineers on both sides have wanted to turn a
    profit. Since the founding of Yster, this dream has been realized...
  download_artifact: plugin-yster1-00-sit
  install: []
- id: zen-v-beta-sit
  label: ZEN- v.Beta.sit
  description: "Welcome to ZEN: a small adventure into religion. The adventures in Zen only add a couple of things, such as a government: Zen a system: Buddha two planets: Nirvana and Zazen. Remember, this is still the beta version and I am looking for suggestions and comm..."
  download_artifact: plugin-zen-v-beta-sit
  install: []
- id: zzzpalevisagefix-sit
  label: zzzpalevisagefix.sit
  description: >-
    Original EV plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-zzzpalevisagefix-sit
  install: []
references:
- https://macintoshgarden.org/games/escape-velocity
---

## Source and version

This is the untouched [Escape Velocity 1.0.5 installer](https://assets.systemless.org/catalogue/objects/sha256/5c/5c6b8ed5ee1efb67f3f7cbe62b051f36d26753337f18a238febbdc023ef0a5a2.bin), preserved in its original MacBinary form. Systemless opens the installer and finds the game inside it at launch; the catalogue does not keep a second, repacked copy.

## Plug-ins

The collection below is a doorway into the astonishing amount of work EV players made for one another: new ships, mission arcs, utilities and full conversions. Each download is the original package held by the source archive. Automatic installation is deliberately unavailable until Systemless can understand those packages without rebuilding them.

## Gameplay

![Escape Velocity gameplay](https://assets.systemless.org/catalogue/media/sha256/52/5294854e83df7be34719c02df8ffc3f6f4a9b48f1b6389b4afeb92fa4f39bfd0.png)

## The life of a pilot

You begin with a shuttle, ten thousand credits and no prescribed career. Trade routes offer the gentlest start, but courier work, bounty hunting, piracy and the growing war between the Confederation and Rebellion soon pull the same little ship in very different directions. Bar conversations matter: many of the game's best stories begin as an unassuming job on an ordinary world.

Use the arrow keys to steer, **L** to select and land, **M** for the map, and **J** to jump once you are clear of the system centre.
