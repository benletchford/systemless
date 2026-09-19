---
id: escape-velocity-override
kind: game
title: Escape Velocity Override
summary: >-
  Make a living among the Crescent's rival civilizations, then decide whose
  future you are willing to fight for.
developer: Matt Burch and Peter Cartwright
publisher: Ambrosia Software
year: 1998
architectures:
- 68k
- ppc
default_architecture: 68k
category: Space Trading
aliases:
- /evo
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-14
    tester: Catalogue maintainer
    systemless_version: 0.40.2
    architecture: 68k
    environment: Local website preview in the in-app browser
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/1838
  - date: 2026-09-15
    tester: Catalogue maintainer
    systemless_version: 0.41.2
    architecture: ppc
    environment: Headless systemless-play run from the original MacBinary installer
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/1975
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
    sha256: a51a55c5df6dbb1eab41d1840ffdcb47a9857190db510a37ac1a695277744932
    size_bytes: 7064960
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/escape-velocity-override
    - https://download.macintoshgarden.org/games/EV_Override_Installer_1.0.2.bin
    license: Ambrosia Software shareware license
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      Non-profit distribution of the complete, unmodified software is permitted;
      registration is required after the trial period.
    notes: "Verified the bundled EV Override License.text after installing the independently retrieved original 1.0.2 installer. Retrieved 2026-09-13: 7064960 bytes; SHA-256 a51a55c5df6dbb1eab41d1840ffdcb47a9857190db510a37ac1a695277744932."
- id: plugin-warblade
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/---_Warblade_---
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/---_Warblade_---
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-3-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/3.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/3.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-360repulsor-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/360Repulsor.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/360Repulsor.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-45sprite-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/45Sprite.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/45Sprite.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-4in1plugins-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/4in1plugins.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/4in1plugins.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-a-radar110
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/A-Radar110.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/A-Radar110.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-advance-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/advance.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/advance.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aftermath-preview-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aftermath%20Preview.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aftermath%20Preview.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aliens-1-0-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aliens_1.0.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aliens_1.0.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aliens-1-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aliens_1.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aliens_1.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aliens-1-2-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aliens_1.2.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Aliens_1.2.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ambrosiaclass-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/AmbrosiaClass.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/AmbrosiaClass.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-android-arada-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Android%20Arada.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Android%20Arada.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-anewgalaxy-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ANewGalaxy.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ANewGalaxy.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-any-ship-anywhere-evo-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Any_ship_anywhere_-_EVO.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Any_ship_anywhere_-_EVO.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-aquabuttonsforevo-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/AquaButtonsforEVO.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/AquaButtonsforEVO.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-arach
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Arach
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Arach
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-arach-2
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Arach.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Arach.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-arada
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Arada.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Arada.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-arcturusshippak101-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ArcturusShipPak101.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ArcturusShipPak101.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-b5plug-inforevov2-6-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/B5Plug-inforEVOv2.6.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/B5Plug-inforEVOv2.6.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-babylon5evo201-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Babylon5EVO201.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Babylon5EVO201.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-barbarian1prelude
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Barbarian1prelude.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Barbarian1prelude.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-battleships-evo-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Battleships-EVO.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Battleships-EVO.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-beamfix
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Beamfix.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Beamfix.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bettersmoke-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/bettersmoke.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/bettersmoke.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-beyondthecrescent11-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/BeyondTheCrescent11.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/BeyondTheCrescent11.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-big-map-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Big_Map.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Big_Map.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-big-planets-and-astroids-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Big_Planets_and_Astroids.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Big_Planets_and_Astroids.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bigassplanetplug-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/BigAssPlanetPlug.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/BigAssPlanetPlug.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-black-lotus-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Black%20Lotus.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Black%20Lotus.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-bomber-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/bomber.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/bomber.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-candalas-more-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Candalas__more.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Candalas__more.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-captainhectorprotector-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CaptainHectorProtector.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CaptainHectorProtector.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-captainsoffreeport11-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CaptainsOfFreeport11.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CaptainsOfFreeport11.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cdh-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CdH_-_.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CdH_-_.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cdmassframework-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CDmassframework.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CDmassframework.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-chp-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CHP.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CHP.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-classicships1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ClassicShips1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ClassicShips1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cng-evoverride-pack-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CNG%20EVOverride%20PACK.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CNG%20EVOverride%20PACK.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-coldfusion1-0-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ColdFusion1.0.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ColdFusion1.0.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-colours-in-the-space
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Colours_in_the_space
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Colours_in_the_space
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-complete-incomplete-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Complete_Incomplete.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Complete_Incomplete.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-compuwars-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CompuWars.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/CompuWars.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-conexoverride-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ConExOverride.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ConExOverride.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-coolplug-bin
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/COOLPLUG.bin.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/COOLPLUG.bin.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-coolshipsdemo-cpt
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Coolshipsdemo.cpt.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Coolshipsdemo.cpt.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cozmosevopp
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/cozmosevopp.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/cozmosevopp.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-croak-v1
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Croak%20v1.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Croak%20v1.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-cwov102-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/cwov102.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/cwov102.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-darkstation10
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DarkStation10.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DarkStation10.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-darthkevs-plug-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DarthKevs_Plug_.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DarthKevs_Plug_.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dblade-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DBlade.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DBlade.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dcwarship
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/dcwarship.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/dcwarship.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-death-kestrel-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Death%20Kestrel.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Death%20Kestrel.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-deltacorp202-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/deltacorp202.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/deltacorp202.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-deltafighter-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/deltafighter.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/deltafighter.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dialogexpander1-0-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DialogExpander1.0.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DialogExpander1.0.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-disablesmoketrails-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DisableSmokeTrails.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DisableSmokeTrails.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-disparmorpercent-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DispArmorPercent.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DispArmorPercent.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-disparmorpercents2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DispArmorPercents2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DispArmorPercents2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-display-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Display.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Display.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dominion-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dominion-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion_1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion_1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dominion-2-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion_2.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion_2.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dominion-3-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion_3.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dominion_3.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dr-soda
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dr._Soda
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Dr._Soda
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dreadnotthedrea-c9ught101-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DreadNotTheDrea%25C9ught101.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DreadNotTheDrea%25C9ught101.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-dspi-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DSPI.SIT
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/DSPI.SIT
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-emalghaheavyships1-2-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EmalghaHeavyShips1.2.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EmalghaHeavyShips1.2.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-empire-for-evo
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Empire_for_EVO.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Empire_for_EVO.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-enhanced-azdara-1-0
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Enhanced%20Azdara%201.0.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Enhanced%20Azdara%201.0.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-enhanced-buttons-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Enhanced%20Buttons.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Enhanced%20Buttons.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-enhanced-evo
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Enhanced%20EVO.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Enhanced%20EVO.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-enhancedevo
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EnhancedEVO.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EnhancedEVO.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-escapevelocitytx
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EscapeVelocityTX
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EscapeVelocityTX
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-data-2-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV%20Data%202.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV%20Data%202.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-overlord-suite-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV%20Overlord%20Suite.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV%20Overlord%20Suite.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-tx-1-0-1sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV-TX%201.0.1sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV-TX%201.0.1sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-tx-1-0-2
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV-TX%201.0.2
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV-TX%201.0.2
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-stations-hqx
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV_Stations.hqx.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV_Stations.hqx.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ev-super-pack-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV_Super_Pack.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EV_Super_Pack.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evcenhanced101-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evcenhanced101.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evcenhanced101.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evclassic-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evclassic.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evclassic.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evclassic11-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVClassic11.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVClassic11.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evenhanced100-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evenhanced100.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evenhanced100.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-expand-v1-0-folder-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20expand%20v1.0%20folder.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20expand%20v1.0%20folder.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-fixer-beta-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Fixer%20beta.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Fixer%20beta.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-flashy-orbs-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Flashy%20Orbs.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Flashy%20Orbs.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-personalities
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Personalities.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Personalities.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-sound-makeover-v1-0
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Sound%20Makeover%20v1.0
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Sound%20Makeover%20v1.0
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-spob-expander
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Spob%20Expander.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO%20Spob%20Expander.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-missions-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evo-missions.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evo-missions.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-sizer-1-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO-Sizer%20%201.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO-Sizer%20%201.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo1-0-2enahncementsfixes
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO1.0.2EnahncementsFixes.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO1.0.2EnahncementsFixes.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-fixer-1-4-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO_Fixer_1.4.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO_Fixer_1.4.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-webboard-pers-v3-0-sit-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evo_webboard_pers_v3.0.sit.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evo_webboard_pers_v3.0.sit.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evo-webboard-ships-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO_Webboard_Ships.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVO_Webboard_Ships.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evoboom-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOBoom.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOBoom.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evobuttons-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOButtons.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOButtons.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evoctsimulator
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOCTSimulator.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOCTSimulator.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evoenhancementsfixes1-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOEnhancementsFixes1.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOEnhancementsFixes1.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evofacelift-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFacelift.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFacelift.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evofighterbaycollection-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFighterBayCollection.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFighterBayCollection.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evofixer1-5-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFixer1.5.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFixer1.5.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evofixer2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFixer2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOFixer2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evogovtfixer1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOGovtFixer1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOGovtFixer1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evogovtfixer2-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOGovtFixer2.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOGovtFixer2.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evomenufix-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evomenufix.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evomenufix.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evonerdsidebar-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVONerdSidebar.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVONerdSidebar.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evoplus-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOPlus.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOPlus.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evoquickstart1-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOQuickstart1.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOQuickstart1.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evosi1-0-0-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOSI1.0.0.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOSI1.0.0.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evosipatch1-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOSIPatch1.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOSIPatch1.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evotechupdate-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOTechUpdate.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVOTechUpdate.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evplug-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVPlug.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVPlug.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evso-updater
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVSO%20Updater
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVSO%20Updater
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evstellarobjects-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVStellarObjects.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVStellarObjects.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evtg-pro-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evtg-pro.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/evtg-pro.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-evtgoverride1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVTGOverride1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/EVTGOverride1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extra-missions
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Extra%20Missions.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Extra%20Missions.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extra-outfits1-3-2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/extra_outfits1.3.2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/extra_outfits1.3.2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extra-weapons1-0-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/extra_weapons1.0.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/extra_weapons1.0.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extraescorts
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ExtraEscorts.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ExtraEscorts.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extraoutfits1-2-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ExtraOutfits1.2.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ExtraOutfits1.2.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-extraoutfits1-3-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ExtraOutfits1.3.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ExtraOutfits1.3.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-f25-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/f25.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/f25.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-f25v20-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/F25v20.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/F25v20.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-femmefatale-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FemmeFatale.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FemmeFatale.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-firestorm-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Firestorm_.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Firestorm_.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-fireworksgalore1-0-0-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FireworksGalore1.0.0.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FireworksGalore1.0.0.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flak-cannon-plug1-02-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Flak%20Cannon%20Plug1.02.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Flak%20Cannon%20Plug1.02.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flakplug1-0-3-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/flakplug1.0.3.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/flakplug1.0.3.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-fleets-of-doom-sit-bin
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Fleets%20Of%20Doom.sit.bin.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Fleets%20Of%20Doom.sit.bin.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-floating-fortress-1-2-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Floating%20Fortress%201.2.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Floating%20Fortress%201.2.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flying-pumpkins-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Flying%20Pumpkins.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Flying%20Pumpkins.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-flying-toasters-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Flying%20Toasters%20.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Flying%20Toasters%20.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-forklift
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/forklift
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/forklift
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-forklift-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Forklift.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Forklift.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-fotve1-0-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FOTVE1.0.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FOTVE1.0.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-frozen-heart-the-no
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Frozen%20Heart%20-%20the%20No.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Frozen%20Heart%20-%20the%20No.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-frozenheart104-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FrozenHeart104.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/FrozenHeart104.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-g-t-p-s-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/G.T.P.S..sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/G.T.P.S..sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galaxyclassoverride201-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyClassOverride201.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyClassOverride201.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galaxyclassoverride31-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyClassOverride31.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyClassOverride31.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galaxyclassoverride31-sit-2
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyClassOverride31.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyClassOverride31.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galaxyfighters2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyFighters2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyFighters2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galaxysedge-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxysEdge.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxysEdge.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galaxysedgedatav101-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxysEdgeDatav101.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxysEdgeDatav101.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-galaxyships-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyShips.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GalaxyShips.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gambling-modifier-pack-v2-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Gambling%20Modifier%20Pack%20v2.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Gambling%20Modifier%20Pack%20v2.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gbtv-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GBTV.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/GBTV.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-government-colours-sitx
  role: supplement
  format: sitx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Government_Colours.sitx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Government_Colours.sitx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-gravmines-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/gravmines.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/gravmines.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-harder100-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Harder100.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Harder100.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hellscream
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Hellscream
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Hellscream
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-holyhandgrenadeofantioch1-cpt
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/HolyHandgrenadeOfAntioch1.cpt.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/HolyHandgrenadeOfAntioch1.cpt.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-homing-rocket-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Homing_Rocket.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Homing_Rocket.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-hyperstrictor-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Hyperstrictor.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Hyperstrictor.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ifp-plug-preview-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IFP%20Plug%20preview.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IFP%20Plug%20preview.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-igadzra-beam-evo-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Igadzra%20Beam%20EVO.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Igadzra%20Beam%20EVO.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-igadzrabeam-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IgadzraBeam.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IgadzraBeam.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-igadzrabeamfix-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IgadzraBeamFix.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IgadzraBeamFix.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-igadzracom
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Igadzracom.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Igadzracom.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-imadoofas
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Imadoofas
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Imadoofas
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-improvedgraphics-v1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ImprovedGraphics%20v1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ImprovedGraphics%20v1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-incomplete-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Incomplete.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Incomplete.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-iotheshuttle-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IotheShuttle.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IotheShuttle.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-iotheshuttlebeta-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IotheShuttleBeta.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IotheShuttleBeta.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ix-mo-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ix-Mo.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ix-Mo.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ixmo1-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IxMo1.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/IxMo1.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-journeymf-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/journeymf.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/journeymf.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-kalar-war-i
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Kalar%20War%20I.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Kalar%20War%20I.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-kamikazedrone1-2-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/KamikazeDrone1.2.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/KamikazeDrone1.2.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-kens-voinian-war-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Kens%20Voinian%20War.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Kens%20Voinian%20War.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-killerforklift2-0-cpt
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/KillerForklift2.0.cpt.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/KillerForklift2.0.cpt.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lightningjump-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/lightningjump.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/lightningjump.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lone-ranger-ships-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Lone_Ranger_ships.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Lone_Ranger_ships.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lone-ranger-ships-sit-2
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Lone_Ranger_ships_.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Lone_Ranger_ships_.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lubaria-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Lubaria.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Lubaria.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-luna-fix-cool-music
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Luna%20Fix%20%20Cool%20Music
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Luna%20Fix%20%20Cool%20Music
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-lunafix-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/LunaFix.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/LunaFix.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-magma201-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magma201.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magma201.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-magma201u-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magma201u.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magma201u.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-magma20u-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magma20u.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magma20u.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-magmas101-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magmas101.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magmas101.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-magmasl10-sit-bin
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magmasl10.sit.bin.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/magmasl10.sit.bin.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mantis-assultship-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/mantis%20assultship.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/mantis%20assultship.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-marines-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Marines.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Marines.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mega-booms
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Mega%20Booms
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Mega%20Booms
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mercenary-add-on-pack-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Mercenary%20Add-On%20Pack.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Mercenary%20Add-On%20Pack.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-minstrel-v-2
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Minstrel%20v.2
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Minstrel%20v.2
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-mosiac-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Mosiac.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Mosiac.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-my-plug
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/My%20plug.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/My%20plug.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-neutronicblaze
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NeutronicBlaze.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NeutronicBlaze.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-new-ship
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/New%20ship
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/New%20ship
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newplanetgraphics-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewPlanetGraphics.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewPlanetGraphics.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newships
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewShips.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewShips.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtech2-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech2.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech2.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtech3-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech3.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech3.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtech4-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech4.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech4.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtech5-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech5.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech5.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtech6-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech6.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech6.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtech7-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech7.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech7.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtech8-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech8.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTech8.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-newtitlesongsforevo-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTitleSongsforEVO.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NewTitleSongsforEVO.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-niftybundle-1-0-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NiftyBundle_1.0.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/NiftyBundle_1.0.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ninnyman10-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ninnyman10.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ninnyman10.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-nova-bracket-warning-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/nova_bracket_warning.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/nova_bracket_warning.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-nuke-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Nuke.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Nuke.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-obsidianbuttons-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ObsidianButtons.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ObsidianButtons.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-orksvsimp-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/OrksvsImp.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/OrksvsImp.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-outfit-mods-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Outfit%20MODs.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Outfit%20MODs.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ovara-plug-in-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ovara_Plug-In.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ovara_Plug-In.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-overflow1942-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Overflow1942.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Overflow1942.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-override-wars-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Override_Wars.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Override_Wars.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-override-wars1-1
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Override_Wars1.1.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Override_Wars1.1.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-overrideev
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/OverrideEV.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/OverrideEV.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-overrideev1-5-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/OverrideEV1.5.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/OverrideEV1.5.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-paaren-station-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Paaren_Station.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Paaren_Station.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-paralelluniverse-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ParalellUniverse.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ParalellUniverse.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-perniciouspluginpak-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PerniciousPluginPak.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PerniciousPluginPak.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-persmaker1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PersMaker1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PersMaker1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-persofev3-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/persofev3.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/persofev3.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-personalityships
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PersonalityShips.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PersonalityShips.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-phantominterface-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PhantomInterface.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PhantomInterface.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-phasedbeamfixer-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PhasedBeamFixer.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PhasedBeamFixer.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-planetgraphics
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Planetgraphics%20.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Planetgraphics%20.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-planetgraphics2-0
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Planetgraphics2.0.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Planetgraphics2.0.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-plasma-1-0-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Plasma%201.0.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Plasma%201.0.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-plasma-1-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Plasma%201.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Plasma%201.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-plasmaboltturret-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/plasmaboltturret.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/plasmaboltturret.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-preview-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Preview.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Preview.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-previewpics2-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PreviewPics2.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/PreviewPics2.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-proximaplugin
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ProximaPlugin.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ProximaPlugin.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-quitfe-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/quitfe.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/quitfe.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rapscallius-1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Rapscallius_1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Rapscallius_1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-raven103-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Raven103.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Raven103.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rebel-folder-v-1-0-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Rebel_Folder_v._1.0.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Rebel_Folder_v._1.0.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-red-nose-ships-2-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Red%20Nose%20Ships%202.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Red%20Nose%20Ships%202.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-redeeming1-0-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Redeeming1.0.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Redeeming1.0.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-refundplug-in-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/refundplug-in.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/refundplug-in.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reign-ii-shades-of-evil-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign%20II-%20Shades%20of%20Evil.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign%20II-%20Shades%20of%20Evil.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reign-of-the-voinians-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign%20of%20the%20Voinians.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign%20of%20the%20Voinians.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reign-of-the-ue-1-1
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign_of_the_UE_1.1
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign_of_the_UE_1.1
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reign-of-the-ue-1-1no-music
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign_of_the_UE_1.1No_Music
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Reign_of_the_UE_1.1No_Music
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reignoftheue
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheUE.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheUE.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reignoftheue-fix-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheUE_fix.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheUE_fix.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reignoftheuev12
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheUEv12.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheUEv12.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-reignofthevoinians-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheVoinians.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ReignOfTheVoinians.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-renegade-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/renegade.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/renegade.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-renegadevessels1-0-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/RenegadeVessels1.0.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/RenegadeVessels1.0.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rengade-pocket-warship-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/rengade%20pocket%20warship.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/rengade%20pocket%20warship.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-rotue-invincinought-fix
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ROTUE_InvinciNought_Fix.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ROTUE_InvinciNought_Fix.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-saboteursrealm-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SaboteursRealm.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SaboteursRealm.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-saboteursrealm10b4r3
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SaboteursRealm10b4r3.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SaboteursRealm10b4r3.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sadefixer-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SADEFixer.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SADEFixer.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-seans-wacky-world
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/seans_wacky_world
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/seans_wacky_world
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-secession-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Secession.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Secession.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-secession15-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Secession15.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Secession15.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shifter-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shifter.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shifter.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shifter2-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shifter2.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shifter2.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ships-r-us-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ships%20R%20Us.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ships%20R%20Us.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ships-r-us1-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ships%20R%20Us1.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ships%20R%20Us1.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shortcuts-plug-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shortcuts%20plug.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shortcuts%20plug.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-shuttle-dx-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shuttle_DX.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Shuttle_DX.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-silent-killer-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/silent%20killer.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/silent%20killer.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-solgraphs-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SolGraphs.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SolGraphs.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-soundsgalore-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SoundsGalore.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SoundsGalore.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-space-amoeba-1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Space%20Amoeba%201.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Space%20Amoeba%201.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spaceballs-the-plug-in
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Spaceballs-%20The%20Plug-In.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Spaceballs-%20The%20Plug-In.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-speed-plug-1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Speed%20Plug%201.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Speed%20Plug%201.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-speedchange-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SpeedChange.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SpeedChange.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-speedchange-2-0-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SpeedChange_2.0.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SpeedChange_2.0.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-spodiaships-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SpodiaShips.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SpodiaShips.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-star-wars-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Star%20Wars.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Star%20Wars.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-star-trek-preview-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Star_Trek_Preview.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Star_Trek_Preview.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starclashov10
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StarclashOv10.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StarclashOv10.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starring-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/starring.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/starring.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starterplug1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StarterPlug1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StarterPlug1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-starwars2103-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StarWars2103.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StarWars2103.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-stockerpods-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StockerPods.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StockerPods.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-strandvessel1-0-1-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Strandvessel1.0.1.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Strandvessel1.0.1.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-strandvessels1-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StrandVessels1.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StrandVessels1.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-strangeasteroid-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StrangeAsteroid.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StrangeAsteroid.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-stressrelease-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StressRelease.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/StressRelease.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-striker-plug-in-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Striker%20plug-in.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Striker%20plug-in.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-super-plug-in-pak-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Super%20Plug-In%20pak.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Super%20Plug-In%20pak.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-super-stuff-2-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Super_Stuff_2.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Super_Stuff_2.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-superlazira-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SuperLazira.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SuperLazira.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-superships-1-2-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Superships%201.2.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Superships%201.2.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sw-conf-plug-in-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW%20Conf.%20plug-in.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW%20Conf.%20plug-in.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sw2-2-0-0
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW2%202.0.0.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW2%202.0.0.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sw2-2-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW2%202.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW2%202.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-sw221-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW221.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SW221.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-swextraships-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SWextraShips.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SWextraShips.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-swextrashipsv-2-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SWextraShipsV.2.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/SWextraShipsV.2.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-tbfl-test-version-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TBFL%20test%20version.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TBFL%20test%20version.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-teddy-trouble-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Teddy-Trouble.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Teddy-Trouble.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-the-traders-union-1-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/The%20Traders%20Union%201.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/The%20Traders%20Union%201.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-the-assailant-preview-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/The_Assailant_Preview.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/The_Assailant_Preview.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thealliedpowers-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheAlliedPowers.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheAlliedPowers.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thefourthreich1-0-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheFourthReich1.0.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheFourthReich1.0.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thereaper-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheReaper.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheReaper.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-theundiscoveredcountryteaser-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheUndiscoveredCountryTeaser.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TheUndiscoveredCountryTeaser.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-thorrion1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Thorrion1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Thorrion1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-topsmods2-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TOPsMods2.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TOPsMods2.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-torture-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/torture.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/torture.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-transferic-shipyards-demo
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Transferic%20Shipyards%20Demo..hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Transferic%20Shipyards%20Demo..hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-transferic-shipyards-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Transferic%20Shipyards.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Transferic%20Shipyards.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-traveler10-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/traveler10.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/traveler10.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-travelers-superpack-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Travelers_Superpack.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Travelers_Superpack.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-tripleagent15-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TripleAgent15.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TripleAgent15.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-tripleagent151-cpt
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TripleAgent151.cpt.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/TripleAgent151.cpt.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-true-pers-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/True%20Pers.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/True%20Pers.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ue-dreadnought-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Dreadnought.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Dreadnought.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ue-interceptor
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Interceptor
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Interceptor
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ue-prototype-plug1-0-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Prototype%20Plug1.0.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Prototype%20Plug1.0.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ue-prototypes-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Prototypes.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Prototypes.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ue-ship-and-outfit-expander
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Ship%20and%20Outfit%20Expander.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE%20Ship%20and%20Outfit%20Expander.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ue-phase-ii-1-0-2-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE_Phase_II_1.0.2.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UE_Phase_II_1.0.2.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-uedreadnaught-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEDreadnaught.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEDreadnaught.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-uedreadnought-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEDreadnought.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEDreadnought.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-uenavy1-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UENavy1.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UENavy1.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-uephaseii-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEPhaseII.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEPhaseII.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-uephaseii1-0-1-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEPhaseII1.0.1.sit.Bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UEPhaseII1.0.1.sit.Bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultima-1-0-1-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima%201.0.1.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima%201.0.1.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultima-1-0-2-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima%201.0.2.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima%201.0.2.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultima-1-0b3-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima%201.0b3.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima%201.0b3.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultima-1-0-4-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima_1.0.4.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima_1.0.4.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultima-103-sea
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima_103.sea.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultima_103.sea.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultimate-customize
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultimate%20Customize.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultimate%20Customize.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultimate-armory-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ultimate_armory_1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ultimate_armory_1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-ultimate-fleet-zip
  role: supplement
  format: zip
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultimate_Fleet.zip
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Ultimate_Fleet.zip
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-und112-sea
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UND112.sea
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UND112.sea
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-universenextdoor112-sea
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UniverseNextDoor112.sea.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UniverseNextDoor112.sea.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-upwithue-1-0-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UpWithUE%201.0.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/UpWithUE%201.0.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-variousevotweaks-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/VariousEvoTweaks.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/VariousEvoTweaks.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-varterwarship
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/VarterWarship.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/VarterWarship.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-varterwarship-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/VarterWarship.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/VarterWarship.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-voinain-supercrusier-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinain_SuperCrusier.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinain_SuperCrusier.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-voinian-speed-fighter
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinian%20Speed-Fighter
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinian%20Speed-Fighter
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-voinian
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinian.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinian.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-voinian-quickstart-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinian_Quickstart.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Voinian_Quickstart.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-weapons-fixer-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Weapons_Fixer.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Weapons_Fixer.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-weapons-fixer1-0-1-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Weapons_Fixer1.0.1.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Weapons_Fixer1.0.1.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-weincomplete-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/WEIncomplete.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/WEIncomplete.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-wepons-galor
  role: supplement
  format: opaque
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Wepons%20Galor
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Wepons%20Galor
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-wowser-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Wowser.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Wowser.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-xtreme-tech-1-1-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Xtreme%20Tech%201.1.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Xtreme%20Tech%201.1.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-xtreme-tech-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Xtreme%20Tech.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Xtreme%20Tech.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-xtremetechcompletread-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/XtremeTechcompletread.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/XtremeTechcompletread.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-zachit-missions
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Zachit%20Missions.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Zachit%20Missions.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-zachit-ships-sit
  role: supplement
  format: sit
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Zachit_Ships.sit
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/Zachit_Ships.sit
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-zachitcruiser-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ZachitCruiser.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ZachitCruiser.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-zachitships1-0-sit
  role: supplement
  format: hqx
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ZachitShips1.0.sit.hqx
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ZachitShips1.0.sit.hqx
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: plugin-zachitships14-sit
  role: supplement
  format: bin
  source:
    type: external
    url: >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ZachitShips14.sit.bin
  provenance:
    redistribution: unknown
    sources:
    - https://archive.org/details/EscapeVelocityPluginCollection
    - >-
      https://archive.org/download/EscapeVelocityPluginCollection/EVO.zip/EVO/Plugins/ZachitShips14.sit.bin
    notes: >-
      Original package independently retrieved from the upstream collection and
      inspected. Download remains with the original provider.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c6d8cb51221937000fbed3c98cbc37e5c768fdd49d8671524c58b5475beac183
    size_bytes: 29355
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
      replaying a fresh pilot from the untouched 1.0.2 MacBinary installer. No browser,
      operating-system frame, or catalogue controls are present.
plugins:
- id: warblade
  label: "--- Warblade ---"
  description: >-
    Adds a powerful Warblade ship for sale on low-tech worlds. You need to reach a
    shipyard that sells it before the change is visible.
  download_artifact: plugin-warblade
  install: []
- id: 3-sea
  label: 3.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-3-sea
  install: []
- id: 360repulsor-sit
  label: 360Repulsor.sit
  description: >-
    360 Repulsor has one weapon - the Repulsor shield. Unlike the repulsor beam,
    the repulsor shield works in a full circle around your ship repelling all ships
    within its range away from you. Now you can keep all those Azdaras away from you at
    all times.Requir...
  download_artifact: plugin-360repulsor-sit
  install: []
- id: 45sprite-sea
  label: 45Sprite.sea
  description: >-
    45ºSprites by Coraxus. If youre tired of having the same top down animation,
    why not try looking at ships at a 45º angle? This ought to liven things up for your
    game.
  download_artifact: plugin-45sprite-sea
  install: []
- id: 4in1plugins-sit
  label: 4in1plugins.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-4in1plugins-sit
  install: []
- id: a-radar110
  label: A-Radar110
  description: >-
    The latest version of A-Radar, which replaces the standard EVO radar graphics
    with something a little more suitable. Also adds a easter-egg mission, and an
    outfit, which both should make the game a lot easier.More information at
  download_artifact: plugin-a-radar110
  install: []
- id: advance-sit
  label: advance.sit
  description: >-
    Advance 1.1Tired of all those bership plug-ins and cheats? Good. Advance is the
    plug-in for you.This plug-in is intended to bring some new ships into the game
    to enrichen it rather than to spoil its beautiful balance. First and foremost, the
    additions are...
  download_artifact: plugin-advance-sit
  install: []
- id: aftermath-preview-sit
  label: Aftermath Preview.sit
  description: "This is a sneak-peek preview of a plug that changes everything in EVO, very much like B5 or SW2. It has a great plot: Humanity has found a new enemy: the Nameless. This creatures exist in 7 dimensions, and can be seen only sometimes. They have wiped out the..."
  download_artifact: plugin-aftermath-preview-sit
  install: []
- id: aliens-1-0-sea
  label: Aliens 1.0.sea
  description: >-
    Remember the Aliens from Escape Velocity? Have you ever wanted to pilot the
    Alien Cruiser? Now you can! My plug-in allows you to purchase alien ships and
    weapons, for a price. Or if you like to cheat then you can get it all for free. I also
    altered the ship...
  download_artifact: plugin-aliens-1-0-sea
  install: []
- id: aliens-1-1-sea
  label: Aliens 1.1.sea
  description: >-
    This is just like the originial Aliens!, except that there are a few
    miscellaneous fixes. And now the Alien Cruiser doesnt have 200 jumps (accidental deciaml
    error). Enjoy!send comments, bugs, and suggestions to
  download_artifact: plugin-aliens-1-1-sea
  install: []
- id: aliens-1-2-2-sit
  label: Aliens 1.2.2.sit
  description: >-
    Its been a while since Ive made any updates, so Ive decided to release a final
    version of Aliens. There are tons of minor updates to the plug-in. Dont forget
    that Ive changed my e-mail address and Im no longer recieving e-mail there. Send
    comments, suggesti...
  download_artifact: plugin-aliens-1-2-2-sit
  install: []
- id: ambrosiaclass-sit
  label: AmbrosiaClass.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ambrosiaclass-sit
  install: []
- id: android-arada-sea
  label: Android Arada.sea
  description: >-
    Have you always liked the sturdy arada, but thought it could be better?This
    Plug-in, called Android Arada, expands the capacities of theArada. With this plug,
    the arada has more space and cargo with a tad bit more weapon slots. Just go to a
    shipyard where a...
  download_artifact: plugin-android-arada-sea
  install: []
- id: anewgalaxy-sit
  label: ANewGalaxy.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-anewgalaxy-sit
  install: []
- id: any-ship-anywhere-evo-sit
  label: Any_ship_anywhere_-_EVO.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-any-ship-anywhere-evo-sit
  install: []
- id: aquabuttonsforevo-sit
  label: AquaButtonsforEVO.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-aquabuttonsforevo-sit
  install: []
- id: arach
  label: Arach
  description: >-
    Ever since space travel, the Jarvans and the Arach have always rubbed each
    other the wrong way. At first it started out as petty fighting, but it soon turned
    into all out war. The Jarvans may have an advantage in combat with power, but often
    times the Arach...
  download_artifact: plugin-arach
  install: []
- id: arach-2
  label: Arach
  description: >-
    Ever since space travel, the Jarvans and the Arach have always rubbed each
    other the wrong way. At first it started out as petty fighting, but it soon turned
    into all out war. The Jarvans may have an advantage in combat with power, but often
    times the Arach...
  download_artifact: plugin-arach-2
  install: []
- id: arada
  label: Arada
  description: >-
    Have you ever gotten tired of all those Aradas that fly around that you cant
    buy? I did. I wanted to pruchase them, fly them, love them. Thats what Arada 1.0
    allows you to do. With Arada, you can buy all the different versions of the Arada
    available in the...
  download_artifact: plugin-arada
  install: []
- id: arcturusshippak101-sea
  label: ArcturusShipPak101.sea
  description: >-
    This plugin updates the eight ships and related outfits that were introduced in
    the original EV Arcturus Trading Company scenario and makes them available in EV
    Override. An Arcturus Outpost station and several trading fleets are also
    included. The plugin c...
  download_artifact: plugin-arcturusshippak101-sea
  install: []
- id: b5plug-inforevov2-6-sea
  label: B5Plug-inforEVOv2.6.sea
  description: >-
    Babylon 5 plug-in version 2.6. Graphics all rendered in 3d including sprites,
    tactical displays and shipyard, including new ships. over 200 systems, almost 100
    missions, over 40 ships, over 50 oufits and around 30 sounds. This plug-in starts
    off somewhere d...
  download_artifact: plugin-b5plug-inforevov2-6-sea
  install: []
- id: babylon5evo201-sit
  label: Babylon5EVO201.sit
  description: >-
    Babylon 5 has been described by TV Guide as "TVs most complex and compelling
    sci-fi series," and has already won two Hugo awards for best dramatic presentation.
    This plug-in starts off somewhere during the year 2259, but by the time you
    accumulate enough mo...
  download_artifact: plugin-babylon5evo201-sit
  install: []
- id: barbarian1prelude
  label: Barbarian1prelude
  description: >-
    Barbarian1 Prelude is a teaser plug showcasing some graphics work for my work
    in progress, Barbarian1, a full universe plug replacing everything, right down to
    the button interface. This plug includes new music, splash screen, intro
    text(setting the stage f...
  download_artifact: plugin-barbarian1prelude
  install: []
- id: battleships-evo-sit
  label: Battleships-EVO.sit
  description: >-
    BattleShips, by Captain Will. plug-in I made a while ago that tries to simulate
    WW2 ship combat in a future setting. 12 ships have been included, plus 10
    weapons including Torpedoes and Monitor Guns. 6 systems have also been added to allow
    you to see these...
  download_artifact: plugin-battleships-evo-sit
  install: []
- id: beamfix
  label: Beamfix
  description: >-
    Fixes the bug that the phased beam on some computers does not fire into the
    correct direction and does not damage other ships.v1.0by Andreas Sandner© 10. August
    2001 Starlight Development
  download_artifact: plugin-beamfix
  install: []
- id: bettersmoke-sit
  label: bettersmoke.sit
  description: >-
    Replaces EVO's smoke-trail graphics. It is only noticeable when damaged ships
    are leaving smoke in flight or combat.
  download_artifact: plugin-bettersmoke-sit
  install: []
- id: beyondthecrescent11-sit
  label: BeyondTheCrescent11.sit
  description: >-
    A set of 23 intermediate-to-advanced missions following the UE mission line
    which are geared more towards the mission-oriented types of EV players. This plug-in
    investigates some of the mysteries offered by EV Override, and picks up where
    the UE-Voinian mis...
  download_artifact: plugin-beyondthecrescent11-sit
  install: []
- id: big-map-sit
  label: Big Map.sit
  description: >-
    Big Map is a plug-in which makes the map screen forOverride bigger  It now has
    a huge 500x500 pixel viewingarea (thats up from 300x300). A simple plug-in, but
    usefulfor those who get a feeling of claustrophobia when lookingat their tiny map
    or otherwise si...
  download_artifact: plugin-big-map-sit
  install: []
- id: big-planets-and-astroids-sit
  label: Big_Planets_and_Astroids.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-big-planets-and-astroids-sit
  install: []
- id: bigassplanetplug-sit
  label: BigAssPlanetPlug.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-bigassplanetplug-sit
  install: []
- id: black-lotus-sea
  label: Black Lotus.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-black-lotus-sea
  install: []
- id: bomber-zip
  label: bomber.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-bomber-zip
  install: []
- id: candalas-more-sit
  label: Candalas__more.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-candalas-more-sit
  install: []
- id: captainhectorprotector-sea
  label: CaptainHectorProtector.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-captainhectorprotector-sea
  install: []
- id: captainsoffreeport11-sit
  label: CaptainsOfFreeport11.sit
  description: >-
    The Captains of Freeport adds several new ships and missions, and extends the
    Freeport series of missions. Anybody with large anti-UE feelings should try this
    plug-in.
  download_artifact: plugin-captainsoffreeport11-sit
  install: []
- id: cdh-sit
  label: CdH_-_.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-cdh-sit
  install: []
- id: cdmassframework-sit
  label: CDmassframework.sit
  description: >-
    This is a guide to filling in the mass portion of the Ship resource. Developed
    by Corsair Developers, the Corsair Developers mass framework is an attempt to
    make life easier for developers. Enjoy! -Diddlysquat the Corsair Developers team
  download_artifact: plugin-cdmassframework-sit
  install: []
- id: chp-sea
  label: CHP.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-chp-sea
  install: []
- id: classicships1-0-sit
  label: ClassicShips1.0.sit
  description: >-
    Classic Ships 1.0 introduces 16 legendary ships from the original Escape
    Velocity (ranging from the humble Courier to the mighty fighting vessels of the
    Rebellion and Confederacy) to the universe of EVO. Along with these classic ships is an
    array of weaponr...
  download_artifact: plugin-classicships1-0-sit
  install: []
- id: cng-evoverride-pack-sit
  label: CNG EVOverride PACK.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-cng-evoverride-pack-sit
  install: []
- id: coldfusion1-0-0-sit
  label: ColdFusion1.0.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-coldfusion1-0-0-sit
  install: []
- id: colours-in-the-space
  label: Colours in the space
  description: This plug adds 7 new ships! Which ships... i dont tell you here! Feedback to
  download_artifact: plugin-colours-in-the-space
  install: []
- id: complete-incomplete-sit
  label: Complete Incomplete.sit
  description: >-
    This is an update to the original Incomplete plug-in. It has more graphics,
    more ships, more of just about everything, including a beginning area which is
    finished enough to play for at least a few hours. Eventually you will run into
    incomplete areas, but t...
  download_artifact: plugin-complete-incomplete-sit
  install: []
- id: compuwars-sit
  label: CompuWars.sit
  description: "CompuWars is the humorous, almost editorialistic, story of the everlasting battle between Apple and Microsoft to gain market share...and the universe, to boot. Sure, theyre in peace now, but just look ahead 100 years! Thats exactly what CompuWars does: jump..."
  download_artifact: plugin-compuwars-sit
  install: []
- id: conexoverride-sit
  label: ConExOverride.sit
  description: >-
    ConEx Override is a major change since the original. I started all over again
    to make it better and to be compatible with ConEx Override. It has 12 new
    background pictures (when you land) rendered in Bryce, 10 brand spanken19 new ships, lots
    and lots of mis...
  download_artifact: plugin-conexoverride-sit
  install: []
- id: coolplug-bin
  label: COOLPLUG.bin
  description: >-
    This Plug adds 4 ships. The Zatu Mobile Suit, The Mobile Suit Carrier, The
    Gunboat and the Sleeker.The stuff is all purchased on Mira.The Fights look a LOT
    better if you get MAGMA as well.For bugs/mistakes, contact me at (ask For Taylor)
  download_artifact: plugin-coolplug-bin
  install: []
- id: coolshipsdemo-cpt
  label: Coolshipsdemo.cpt
  description: >-
    This is a demo version.I will expand it beyond imagination(Well maybe not that
    far) to include Enterprise-D wit actual clips from the movie. Hope you enjoy the
    demo. The final product will be released after Thanksgiving. Please send
    sugestions or comments t...
  download_artifact: plugin-coolshipsdemo-cpt
  install: []
- id: cozmosevopp
  label: cozmosevopp
  description: >-
    This is a Plug-In Pack with 3 plugins:EV Shuttle - makes the shuttle to the EV
    shuttle in stead.Cheat EV Shuttle - does the same as EV Shuttle, but makes the
    ships stats a lot better!GreenBlaze 2.0 - makes the blaze shots green.
  download_artifact: plugin-cozmosevopp
  install: []
- id: croak-v1
  label: Croak v1
  description: >-
    by Tom Smith, plugin is set during the regular EVO timespan. A new renegade
    group known only as Croak is challenging all of the major powers of the Galactic
    South. Croak is rumored to have a secret project that may give them an edge in
    battle. Three campaig...
  download_artifact: plugin-croak-v1
  install: []
- id: cwov102-sit
  label: cwov102.sit
  description: >-
    CompuWars is the humorous, almost editorialistic, story of the everlasting
    battle between Apple and Microsoft to gain market share...and the universe, to boot.
    100 years from now, political power is in the hands of both forces CompuWars
    documents how they a...
  download_artifact: plugin-cwov102-sit
  install: []
- id: darkstation10
  label: DarkStation10
  description: >-
    Dark Station adds one new space station, in South Tip Renegade space. Whats so
    special about that? Ill tell you. This is no ordinary space station were talking
    about here. The ST Renegades use Dark Station as an outlet for all rarer, more
    powerful (and high...
  download_artifact: plugin-darkstation10
  install: []
- id: darthkevs-plug-sit
  label: DarthKevs Plug .sit
  description: >-
    This is my first plug for EVOverride, though not my first plug. Ive made a few
    plugs for Ares and rescently made one for EVNova. Basically, this plug adds a new
    government named DarthKevs Mercenaries. They own and run a station in te Sol
    system orbiting Ear...
  download_artifact: plugin-darthkevs-plug-sit
  install: []
- id: dblade-sea
  label: DBlade.sea
  description: This Plug-in adds 1 system, 3 ships and many outfits and weapons.
  download_artifact: plugin-dblade-sea
  install: []
- id: dcwarship
  label: dcwarship
  description: >-
    Dark Crescent Warship is an addition to EVO Magma. No, it was not made by Meowx
    nor anyone affiliated with it. It just adds a kicka ship that you get after
    doing the Zachit Missions. Though you should try to work up an abhorrent amount of
    credits before cre...
  download_artifact: plugin-dcwarship
  install: []
- id: death-kestrel-sit
  label: Death Kestrel.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-death-kestrel-sit
  install: []
- id: deltacorp202-sea
  label: deltacorp202.sea
  description: Adds 2 new ships.
  download_artifact: plugin-deltacorp202-sea
  install: []
- id: deltafighter-sit
  label: deltafighter.sit
  description: >-
    This was an experiment in ResEdit shipmaking and 3D modelling. It adds one
    ship, the Delta Fighter. Enjoy!
  download_artifact: plugin-deltafighter-sit
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
- id: disablesmoketrails-sit
  label: DisableSmokeTrails.sit
  description: >-
    The Disable Smoke Trails Plug gives you the option to disable the new smoke
    trail feature in EVO 1.02. This Plug is compatible with ALL other plugs. Use this if
    you are on an older computer and the smoke trails slows the game down, or you
    just want the opti...
  download_artifact: plugin-disablesmoketrails-sit
  install: []
- id: disparmorpercent-sit
  label: DispArmorPercent.sit
  description: >-
    Display Armor Percentages tells players how much armor a ship has left as it
    gets pummeled (instead of simply saying "Shields Down") -- excellent when fighting
    Voinian ships. Now youll have a general idea of when that ship will finally blow
    up!
  download_artifact: plugin-disparmorpercent-sit
  install: []
- id: disparmorpercents2-sit
  label: DispArmorPercents2.sit
  description: >-
    This file is the same as Display Armor Percentages 1.0, except that the armor
    of the Voinian Dreadnought is not displayed. This allows Admiral McPhersons claim
    that one does not know when the Dreadnought will be destroyed to stand true.
  download_artifact: plugin-disparmorpercents2-sit
  install: []
- id: display-sit
  label: Display.sit
  description: >-
    Display for EVO 1.0.2 by IonStorm improves the display and armor graphic. It is
    very small, universally compatible, and should help your game play--so why not
    get it.
  download_artifact: plugin-display-sit
  install: []
- id: dominion-sit
  label: Dominion.sit
  description: "This is a complete rewrite of the EVO unverse, based upon the Dominion War from Star Trek: Deep Space Nine. It has massive amounts of ships, systems, and missions, and is a must have for any Star Trek fan. As opposed to downloading it from this server, it c..."
  download_artifact: plugin-dominion-sit
  install: []
- id: dominion-1-sea
  label: Dominion 1.sea
  description: >-
    Here it is, folks. Finally, Dominion is downloadable from Ambrosias website. I
    apologize for those of you who have downloaded the placeholder (all 600+ of you).
    I was not clear enough that the file was only available at the external URL.
    Anyway, enjoy EVO:...
  download_artifact: plugin-dominion-1-sea
  install: []
- id: dominion-2-sea
  label: Dominion 2.sea
  description: >-
    Here it is, folks. Finally, Dominion is downloadable from Ambrosias website. I
    apologize for those of you who have downloaded the placeholder (all 600+ of you).
    I was not clear enough that the file was only available at the external URL.
    Anyway, enjoy EVO:...
  download_artifact: plugin-dominion-2-sea
  install: []
- id: dominion-3-sea
  label: Dominion 3.sea
  description: >-
    Here it is, folks. Finally, Dominion is downloadable from Ambrosias website. I
    apologize for those of you who have downloaded the placeholder (all 600+ of you).
    I was not clear enough that the file was only available at the external URL.
    Anyway, enjoy EVO:...
  download_artifact: plugin-dominion-3-sea
  install: []
- id: dr-soda
  label: Dr._Soda
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-dr-soda
  install: []
- id: dreadnotthedrea-c9ught101-sit
  label: DreadNotTheDrea%C9ught101.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-dreadnotthedrea-c9ught101-sit
  install: []
- id: dspi-sit
  label: DSPI.SIT
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-dspi-sit
  install: []
- id: emalghaheavyships1-2-sea
  label: EmalghaHeavyShips1.2.sea
  description: >-
    EmalghaHeavyShips1.2 - By Coraxus. The latest updated version of these vessels.
    This set comes with special bonus plug-ins
  download_artifact: plugin-emalghaheavyships1-2-sea
  install: []
- id: empire-for-evo
  label: Empire for EVO
  description: "Empire: War Without End for EVO - This is a port of the classic EV plug-in Empire: War Without End to EVO. This was originally released in 1997 by Tim Isles and started the increasingly inaccurately named Empire Trilogy. In the process of converting the plu..."
  download_artifact: plugin-empire-for-evo
  install: []
- id: enhanced-azdara-1-0
  label: Enhanced Azdara 1.0
  description: >-
    Ever wanted to buy that awsome Enhanced Azdara that you helped test? Well now
    you can. This plug allows you to buy the Enhanced Azdara Bay and the Enhanced
    Azdara to the game as soon as you complete the appropriate missions.
  download_artifact: plugin-enhanced-azdara-1-0
  install: []
- id: enhanced-buttons-sit
  label: Enhanced Buttons.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-enhanced-buttons-sit
  install: []
- id: enhanced-evo
  label: Enhanced EVO
  description: >-
    This Add On enhances the ships by giving them more weapons. Please rate all add
    ons you down load!!!
  download_artifact: plugin-enhanced-evo
  install: []
- id: enhancedevo
  label: EnhancedEVO
  description: >-
    This is better then the first one I made. It makes UE stronger and they only
    use blaze weapons and hunter missiles. Have Fun!
  download_artifact: plugin-enhancedevo
  install: []
- id: escapevelocitytx
  label: EscapeVelocityTX
  description: >-
    EV-TX is an expansion to EV, with all the possibilities from EVO 1.0.2.Many new
    ships, outfits (some new graphics), weapons and other stuff awaits you.You will
    start in one of two systems and must do 5 missions (really simple) to be free. If
    youve done th...
  download_artifact: plugin-escapevelocitytx
  install: []
- id: ev-data-2-sit
  label: EV Data 2.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-data-2-sit
  install: []
- id: ev-overlord-suite-sit
  label: EV Overlord Suite.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-overlord-suite-sit
  install: []
- id: ev-tx-1-0-1sit
  label: EV-TX 1.0.1sit
  description: >-
    EV-TX is an expansion to EV, with all the possibilities from EVO.Many new ships
    (no new graphics), weapons and other stuff awaits you.You will start in one of
    two systems and must do 5 missions to be free (really simple). As you do this
    missions you come in...
  download_artifact: plugin-ev-tx-1-0-1sit
  install: []
- id: ev-tx-1-0-2
  label: EV-TX 1.0.2
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-tx-1-0-2
  install: []
- id: ev-stations-hqx
  label: EV Stations.hqx
  description: >-
    EV Stations 1.0 is a graphics-enhancement plug-in for Escape Velocity Override.
    EV Stations replaces the standard EVO-style space stations graphics with the
    ones from the original Escape Velocity. Its a great piece of nostalgia from the old
    days of EV!
  download_artifact: plugin-ev-stations-hqx
  install: []
- id: ev-super-pack-sit
  label: EV_Super_Pack.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ev-super-pack-sit
  install: []
- id: evcenhanced101-sit
  label: evcenhanced101.sit
  description: >-
    EVC Enhanced for Override is two things. It is a port of Matt Burchs Escape
    Velocity universe to an EVO plug-in file. It also has another file, which enhances
    the port for features found in Override 1.0.2. Plug-in is version 1.0.1. Written
    by Gavin Dow, . "...
  download_artifact: plugin-evcenhanced101-sit
  install: []
- id: evclassic-sit
  label: evclassic.sit
  description: >-
    This plugin turns EV Override into EV classic -- so you get to play the old
    game on the new engine! To use EVC simply place "EVC Data" "EVC Graphics" "EVC
    Music" "EVC Sounds" and "EVC Titles" in the "EV Plug-Ins" folder in the EV Override
    folder.It is recom...
  download_artifact: plugin-evclassic-sit
  install: []
- id: evclassic11-sit
  label: EVClassic11.sit
  description: >-
    Escape Velocity Classic is a direct port of the original EV universe (by Matt
    Burch), designed to run in EV Override. For all those familiar with the EV
    universe and its ships and planets we have all come to know and love, this plug is for
    you. It was desig...
  download_artifact: plugin-evclassic11-sit
  install: []
- id: evenhanced100-sit
  label: evenhanced100.sit
  description: >-
    EV Enhanced for Override is two things. It is a port of Matt Burchs Escape
    Velocity universe to an EVO plug-in file. It also has another file, which enhances
    the port for features found in Override 1.0.2. Plug-in is version 1.0.0. Written by
    Gavin Dow, . "S...
  download_artifact: plugin-evenhanced100-sit
  install: []
- id: evo-expand-v1-0-folder-sit
  label: EVO expand v1.0 folder.sit
  description: "EVO expand is a plug that gives access to ships and new weapons for you but for a price so its not really a cheat.made by (AK)clownhunter e mail me at: if you have questons or comments."
  download_artifact: plugin-evo-expand-v1-0-folder-sit
  install: []
- id: evo-fixer-beta-sit
  label: EVO Fixer beta.sit
  description: >-
    I was usong about ten plugs in EVO. After a while, they stopped working, so now
    I took parts of some plugs, and some of my own to make my version of EVO
    Fixer(beta). It includes weapons, targeters, better looking somke, and more. Remember
    its a BETA. Email...
  download_artifact: plugin-evo-fixer-beta-sit
  install: []
- id: evo-flashy-orbs-sit
  label: EVO Flashy Orbs.sit
  description: >-
    A small plugin that changes the graphics of the main screens orbs so that they
    flash. Pointless, but it looks kinda cool.
  download_artifact: plugin-evo-flashy-orbs-sit
  install: []
- id: evo-personalities
  label: EVO Personalities
  description: >-
    EVO Personalities adds 40 personalities to Override. They are mostly U.E.
    Warships but there are some Dreadnaughts and Cruisers. I also gave the Emalgha some
    ships since they had none. If you want money, attack the ships with the I.C.V.
    prefix(Hehehehe).
  download_artifact: plugin-evo-personalities
  install: []
- id: evo-sound-makeover-v1-0
  label: EVO Sound Makeover v1.0
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evo-sound-makeover-v1-0
  install: []
- id: evo-spob-expander
  label: EVO Spob Expander
  description: >-
    Adds extra planets, systems, and jump routes. The title screen is unchanged;
    check the map and hyperspace routes after starting a pilot.
  download_artifact: plugin-evo-spob-expander
  install: []
- id: evo-missions-sit
  label: evo-missions.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evo-missions-sit
  install: []
- id: evo-sizer-1-2-sit
  label: EVO-Sizer 1.2.sit
  description: >-
    Do you think that some fighters are not to scale? Do you think the plasma
    siphon could use a better graphic? If so this plug is for you. EVO-Sizer 1.2 makes
    fighters closer to scale gives you a better sprites for planets. And makes the game
    more real. v. 1....
  download_artifact: plugin-evo-sizer-1-2-sit
  install: []
- id: evo1-0-2enahncementsfixes
  label: EVO1.0.2EnahncementsFixes
  description: >-
    This package contains two plug-ins for EVO. The first, 1.0.2 Enhancements,
    provides a number of modifications to take advantage of the new features introduced
    in Override version 1.0.2. The second, 1.0.2 Fixes, provides a number of fixes to
    some minor probl...
  download_artifact: plugin-evo1-0-2enahncementsfixes
  install: []
- id: evo-fixer-1-4-1-sit
  label: EVO Fixer 1.4.1.sit
  description: >-
    EVO fixer is a simple plug for EVO that changes some things in EVO that I
    thought need changing/fixing. It lets you buy a UE Fighter Bay, Zidager Fighter Bay
    and Zidager expansion thingy as well as letting you keep the UE Cloak when you
    change ships, it als...
  download_artifact: plugin-evo-fixer-1-4-1-sit
  install: []
- id: evo-webboard-pers-v3-0-sit-zip
  label: evo_webboard_pers_v3.0.sit.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evo-webboard-pers-v3-0-sit-zip
  install: []
- id: evo-webboard-ships-sea
  label: EVO Webboard Ships.sea
  description: >-
    Ever wanted to buy your own ship? Well guess what, now you can do just that! In
    EVO Webboard Ships, several people from the EVO webboard have submitted designs
    for new ships, and YOU get to pilot them! EVO Webboard Ships adds a few systems,
    several fleets,...
  download_artifact: plugin-evo-webboard-ships-sea
  install: []
- id: evoboom-sit
  label: EVOBoom.sit
  description: >-
    EVO Boom! is a simple plugin for Ambrosia Software19s EV Override. It changes
    the explosion graphics in EVO from those tiny little sparks to big dramatic
    fireballs. All you have to do is drop it in your "EV Plugs" folder and you ready to go!
    There are no kn...
  download_artifact: plugin-evoboom-sit
  install: []
- id: evobuttons-sit
  label: EVOButtons.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evobuttons-sit
  install: []
- id: evoctsimulator
  label: EVOCTSimulator
  description: >-
    This is the test version of my upcoming plug-in.This test features twelve
    unique ships, eight missions, and a new station.The full version will have 35+ unique
    ships (26 are done).
  download_artifact: plugin-evoctsimulator
  install: []
- id: evoenhancementsfixes1-1
  label: EVOEnhancementsFixes1.1
  description: "This package provides a number of fixes to some minor \"problems\" in Override and also a number of modifications to take advantage of the new features introduced in Override version 1.0.2. New to version 1.1: Missiles and rockets will do greater damage at sh..."
  download_artifact: plugin-evoenhancementsfixes1-1
  install: []
- id: evofacelift-sit
  label: EVOFacelift.sit
  description: >-
    This plug-in is a back port of Tim Morgans EVN-EVO Facelift, a plug-in for EV
    Nova. It is a graphical facelift for EVO, featuring awesome 3D ship and weapon
    sprites, redesigned high-quality explosion graphics, new smoke trails, and
    Nova-style sidebar target...
  download_artifact: plugin-evofacelift-sit
  install: []
- id: evofighterbaycollection-sit
  label: EVOFighterBayCollection.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evofighterbaycollection-sit
  install: []
- id: evofixer1-5-sit
  label: EVOFixer1.5.sit
  description: >-
    EVO Fixer is a simple plug for EVO that changes some things in EVO that I
    thought need changing/fixing. It lets you buy a UE Fighter Bay, Zidager Fighter Bay
    and Zidager expansion thingy as well as letting you keep the UE Cloak when you
    change ships, it als...
  download_artifact: plugin-evofixer1-5-sit
  install: []
- id: evofixer2-sit
  label: EVOFixer2.sit
  description: >-
    EV Override Fixer 2.0Better Graphics, Improved Weapons, Additional Effects,
    Everything needed to enhance the game further.
  download_artifact: plugin-evofixer2-sit
  install: []
- id: evogovtfixer1-0-sit
  label: EVOGovtFixer1.0.sit
  description: >-
    Ever get really mad when youre in a big battle, and then you acidentlly hit one
    of youre allies and suddenly all of youre allies turn aginst you? Well, this
    plug changes that.This is a set of plugs that will make it so youre weapons cant hit
    the ships of th...
  download_artifact: plugin-evogovtfixer1-0-sit
  install: []
- id: evogovtfixer2-0-sit
  label: EVOGovtFixer2.0.sit
  description: >-
    This is version 2.0 of EVO Govt Fixer. A few bugs have been fixed, thanks to
    user bug reports. The human Renegade plug now works, support formerchants in now in
    a seperate plug compatable with all the rest, the Huron Rebels wont attack you
    while theyre pret...
  download_artifact: plugin-evogovtfixer2-0-sit
  install: []
- id: evomenufix-sit
  label: evomenufix.sit
  description: >-
    This fixes the grainy menu in EVO. In other words it replaces the old with a
    new, non-grainy one. By LoneIgadzra -
  download_artifact: plugin-evomenufix-sit
  install: []
- id: evonerdsidebar-sit
  label: EVONerdSidebar.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evonerdsidebar-sit
  install: []
- id: evoplus-sit
  label: EVOPlus.sit
  description: >-
    The must-have plug-in for EV Override. EVO Plus extends almost every element of
    this wonderful game. From over 40 new missions to several new ships and almost a
    dozen all-new systems as well as two dozen modified ones, this game is a must
    have. And by optim...
  download_artifact: plugin-evoplus-sit
  install: []
- id: evoquickstart1-2-sit
  label: EVOQuickstart1.2.sit
  description: >-
    This plug is for those of you who dont have 20 hours a day to play EVO. It
    makes a few changes so you dont have to spend as much time just getting started.
    First, you dont start out with that crappy shuttle, you get a freight-courior
    instead. Aslo, you are...
  download_artifact: plugin-evoquickstart1-2-sit
  install: []
- id: evosi1-0-0-sea
  label: EVOSI1.0.0.sea
  description: "EVOSI1.0.0 -by Coraxus. After almost a year in development, EVOSI (Escape Velocity: Override System Interface) is finally here. This plug-in will revamp graphics and sounds you will find in any interface items throughout the game. The main feature is the ta..."
  download_artifact: plugin-evosi1-0-0-sea
  install: []
- id: evosipatch1-0-sit
  label: EVOSIPatch1.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evosipatch1-0-sit
  install: []
- id: evotechupdate-sit
  label: EVOTechUpdate.sit
  description: >-
    EVO Tech Update 1.0.0 is an enhancment plug-in that gives EVO a complete
    makeover new splash screens, new dialogue buttons, new intro music, the works. The main
    feature is new target PICTs for all the standard EVO ships, but the end result
    is a completely d...
  download_artifact: plugin-evotechupdate-sit
  install: []
- id: evplug-sit
  label: EVPlug.sit
  description: >-
    EV on EVO? There is a large community of people who prefer the original Escape
    Velocity to Escape Velocity despite a better engine. Well for those people, along
    with EVO users who want to see what its like but dont want to pay the additional
    registration fe...
  download_artifact: plugin-evplug-sit
  install: []
- id: evso-updater
  label: EVSO Updater
  description: This is the updater for EVO Stellar Objects.
  download_artifact: plugin-evso-updater
  install: []
- id: evstellarobjects-sit
  label: EVStellarObjects.sit
  description: >-
    This Plug will change all those primitive graphics reused from older
    games/versions, and replaces them with brand-new, professional looking 3D graphics. Every
    stellar object from asteroids to planets and their caption pictures have been
    redone.
  download_artifact: plugin-evstellarobjects-sit
  install: []
- id: evtg-pro-sit
  label: evtg-pro.sit
  description: >-
    Thank you for downloading EV Target Graphics Pro! We hope you enjoy the
    enhancements this plug-in makes. We believe it provides a welcome improvement to EV
    Override, and eye candy is always loved. =] Fortunately, this is one Read Me you wont
    have to read i...
  download_artifact: plugin-evtg-pro-sit
  install: []
- id: evtgoverride1-sit
  label: EVTGOverride1.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-evtgoverride1-sit
  install: []
- id: extra-missions
  label: Extra Missions
  description: >-
    Adds 12 twelve extra missions, 3 dangerous deliveries, 3 dangerous rush
    deliveries, 3 very rushed deliveries, and 3 dangerous very rushed deliveries. All
    missions are UE.
  download_artifact: plugin-extra-missions
  install: []
- id: extra-outfits1-3-2-sit
  label: extra outfits1.3.2.sit
  description: >-
    Extra Outfits adds 13 new, balanced, outfits to the EVO universe. It includes
    an Auto-Eject System, (like that found in the original EV) 3 types of Armor, a
    slightly bigger regional map, and many other useful things. (youll have to download
    it to find out w...
  download_artifact: plugin-extra-outfits1-3-2-sit
  install: []
- id: extra-weapons1-0-0-sit
  label: extra weapons1.0.0.sit
  description: >-
    Extra Weapons, the sister plugin of Extra Outfits, adds 9 new weapons to the
    EVO universe, ranging from stationary mines to powerful energy weapons. In addition
    to Extra Weapons, you also get "Blink Fix", which fixes the mysterysous SAD/SAE
    "blinking", "Hom...
  download_artifact: plugin-extra-weapons1-0-0-sit
  install: []
- id: extraescorts
  label: ExtraEscorts
  description: "Extra Escorts allows you to have more than six escorts in EVO by way of purchasing outfits and accepting missions. There are four ships this plug-in allows you to get as escorts: Crescent Fighter, Arada, Crescent Warship and Lazira. For each there are two o..."
  download_artifact: plugin-extraescorts
  install: []
- id: extraoutfits1-2-sit
  label: ExtraOutfits1.2.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-extraoutfits1-2-sit
  install: []
- id: extraoutfits1-3-sit
  label: ExtraOutfits1.3.sit
  description: "Extra Outfits 1.3 adds 13, new, and much needed, outfits (12 of which have new graphics) to EVO, including, a cargopod, an auto-ejection system, and 3 different types of armor, not to mention the other 8 outfits. enjoy!Author: Martin Hardinge-mail:"
  download_artifact: plugin-extraoutfits1-3-sit
  install: []
- id: f25-sit
  label: f25.sit
  description: "F-25: New Galaxy v1.4 -- Its been two months since you last ventured deep into the dark depths of the Proxima and Ji Nebulas. Two months since you heard news of colonization within these dark frontiers. Now, that silence has been broken. You are sitting in..."
  download_artifact: plugin-f25-sit
  install: []
- id: f25v20-sit
  label: F25v20.sit
  description: "F-25 v.2.0: Completely re-written and re-done from the bottom up, F-25: The Last Empire follows the same basic story of exploration into the Ji Nebula, but thats where the similarities with the previous versions end. With this new scenario come 235 complete..."
  download_artifact: plugin-f25v20-sit
  install: []
- id: femmefatale-sea
  label: FemmeFatale.sea
  description: >-
    Jasta Hela - briefly encountered in the hugely popular Frozen Heart for Escape
    Velocity Override - is back with her own adventure. Discovered helpless and
    drifting in a human death-trap, Jasta Hela recovers only to learn that sinister forces
    have wiped her...
  download_artifact: plugin-femmefatale-sea
  install: []
- id: firestorm-sit
  label: Firestorm_.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-firestorm-sit
  install: []
- id: fireworksgalore1-0-0-zip
  label: FireworksGalore1.0.0.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-fireworksgalore1-0-0-zip
  install: []
- id: flak-cannon-plug1-02-sit
  label: Flak Cannon Plug1.02.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-flak-cannon-plug1-02-sit
  install: []
- id: flakplug1-0-3-sea
  label: flakplug1.0.3.sea
  description: >-
    This is (hopefully) the final release of the Flak Plug. For those who dont
    know, Flak Plug is a pointless little plug-in my Dave which adds flak weapons to EVO.
    Flak Plug is comatible with most non-TC plugins. The plug adds 2 new weapons,
    the Flak Turret an...
  download_artifact: plugin-flakplug1-0-3-sea
  install: []
- id: fleets-of-doom-sit-bin
  label: Fleets Of Doom.sit.bin
  description: >-
    This is a plugin that adds about 9 "Fleets Of Doom". Fleets of doom are huge
    fgleets of powerful ships. Try and defeat them....If you dare!
  download_artifact: plugin-fleets-of-doom-sit-bin
  install: []
- id: floating-fortress-1-2-1-sea
  label: Floating Fortress 1.2.1.sea
  description: >-
    This plug-in adds an invinsible ship, three weapons, fighters, and a system
    with a tribute of 10 Million credits. This is the same file that is at the EV
    add-ons page. Both an EV and an EVO plug-in are in this file. Read the documentation
    for details.
  download_artifact: plugin-floating-fortress-1-2-1-sea
  install: []
- id: flying-pumpkins-sit
  label: Flying Pumpkins.sit
  description: >-
    This is something I whipped up partly for the heck of it and partly to see if I
    could get something to home right in EVO 1.0.2. Actually, the main reason is my
    dad wants me to do a vegetable total conversion. While I didnt actually do a TC,
    you may get a fe...
  download_artifact: plugin-flying-pumpkins-sit
  install: []
- id: flying-toasters-sit
  label: Flying Toasters .sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-flying-toasters-sit
  install: []
- id: forklift
  label: forklift
  description: >-
    Adds the Forklift ship to the Freeport shipyard. It is a prototype plugin, so
    the change is localized and may have rough edges.
  download_artifact: plugin-forklift
  install: []
- id: forklift-sit
  label: Forklift.sit
  description: >-
    Ever given a pilot a forklift- and then regretted it? Forklifts take a lot of
    the fun out of the game, in my opinion, and theyre addictive and impossible to get
    rid of. Now, with this "disposable" plugin, you can take forklifts away from
    your pilots.
  download_artifact: plugin-forklift-sit
  install: []
- id: fotve1-0-1-sea
  label: FOTVE1.0.1.sea
  description: >-
    FOTVE1.0.1 - by Coraxus. After re-hauling this plug-in for so long, it is now
    finally here, Fall of the Voinian Empire version 1.0.1. Thanks to all who
    participated in beta-testing this plug-in! For those of you who dont know, Fall of the
    Voinian Empire is...
  download_artifact: plugin-fotve1-0-1-sea
  install: []
- id: frozen-heart-the-no
  label: Frozen Heart - the No
  description: >-
    BASK in the setting suns and sleazy bars of New Venus! ENJOY a cold cup of
    coffee paid for by a princess! EXPLORE a fifty thousand year old cryogenic maze! For
    the first time on the internet in unencrypted form, this is the novel on which
    Frozen Heart was b...
  download_artifact: plugin-frozen-heart-the-no
  install: []
- id: frozenheart104-sit
  label: FrozenHeart104.sit
  description: >-
    Written as one of the most ambitious plug-ins for Escape Velocity or Override
    to date, and set in a totally new, totally coherent science-fiction universe, the
    Frozen Heart was 14 months in development and features all new ships, systems,
    planets, governmen...
  download_artifact: plugin-frozenheart104-sit
  install: []
- id: g-t-p-s-sit
  label: G.T.P.S..sit
  description: >-
    My first plug-in for EVO, G.T.P.S (Glaive Turret Propulsion System) is neither
    a cheat weapon, nor a tactical disadvantage. Glaives are human-made rocket
    turrets for the renegade-squashing traders in all of us! Proven to kill a Helian in 3
    hits!Made by Will...
  download_artifact: plugin-g-t-p-s-sit
  install: []
- id: galaxyclassoverride201-sit
  label: GalaxyClassOverride201.sit
  description: >-
    Galaxy Class Override is a plug-in for Star Trek:TNG fans. It adds one base,
    Utopia Planitia, over Mars in Sol which makes available the Galaxy Class starship,
    as well as a new UE fighter model for use with the galaxy class. It also adds the
    Galaxy Class to...
  download_artifact: plugin-galaxyclassoverride201-sit
  install: []
- id: galaxyclassoverride31-sit
  label: GalaxyClassOverride31.sit
  description: "Galaxy Class: Override is a plug-in that adds the availability of a ship similar to the Galaxy Class Starship from Star Trek: TNG. This new version provides improvements in graphics, better ship physics, new weapons, use of the Galaxy by AIs, a storyline fo..."
  download_artifact: plugin-galaxyclassoverride31-sit
  install: []
- id: galaxyclassoverride31-sit-2
  label: GalaxyClassOverride31.sit
  description: "Galaxy Class: Override is a plug-in that adds the availability of a ship similar to the Galaxy Class Starship from Star Trek: TNG. This new version provides improvements in graphics, better ship physics, new weapons, use of the Galaxy by AIs, a storyline fo..."
  download_artifact: plugin-galaxyclassoverride31-sit-2
  install: []
- id: galaxyfighters2-sit
  label: GalaxyFighters2.sit
  description: >-
    Galaxy Ships2 is just an upgrade og the first one i made. I fixed lots of the
    spelling mistakes and added something that when you target someone in stellar, it
    shows brackets around them and tells youif their disabled or not. Other than that
    it is still jus...
  download_artifact: plugin-galaxyfighters2-sit
  install: []
- id: galaxysedge-sit
  label: GalaxysEdge.sit
  description: "Taking place within our solar system, Galaxys Edge is the most thoroughly researched and astronomically accurate Escape Velocity: Override plugin ever. Galaxys Edge has a detailed plot, featuring all-new governments, ships, outfits, planets and missions, as..."
  download_artifact: plugin-galaxysedge-sit
  install: []
- id: galaxysedgedatav101-sit
  label: GalaxysEdgeDatav101.sit
  description: >-
    A small update to Galaxys Edge. Version 1.0.1 of the data file fixes a few
    mission bugs, including the Von Neumann mission and several convoy missions.
  download_artifact: plugin-galaxysedgedatav101-sit
  install: []
- id: galaxyships-sit
  label: GalaxyShips.sit
  description: >-
    Galax ships is a collection of ships. Some od them i made some i didnt. The
    ships are placed all through the galaxy. There is nothing but 11 new ships. Created
    by Daniel Torgerson, Torg Enterprises.
  download_artifact: plugin-galaxyships-sit
  install: []
- id: gambling-modifier-pack-v2-0-sit
  label: Gambling Modifier Pack v2.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-gambling-modifier-pack-v2-0-sit
  install: []
- id: gbtv-sit
  label: GBTV.sit
  description: >-
    This plug-in makes the Voinian ships stronger, and allows you to buy a new type
    of ship, at the same time as the Frigate. Made by Thimoty (),Enjoy!
  download_artifact: plugin-gbtv-sit
  install: []
- id: government-colours-sitx
  label: Government_Colours.sitx
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-government-colours-sitx
  install: []
- id: gravmines-sit
  label: gravmines.sit
  description: >-
    Adds the Grav Mine outfit. Grav Mines are set in place by the player and, when
    told to attack a ship, stick it in place. They dont do any damage, but allow the
    player to attack or flee while the enemy ship is stuck there.
  download_artifact: plugin-gravmines-sit
  install: []
- id: harder100-sit
  label: Harder100.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-harder100-sit
  install: []
- id: hellscream
  label: Hellscream
  description: >-
    Adds a Voinian ship. The original notes say it lacks the companion name data,
    so the ship may appear unnamed or oddly named in-game.
  download_artifact: plugin-hellscream
  install: []
- id: holyhandgrenadeofantioch1-cpt
  label: HolyHandgrenadeOfAntioch1.cpt
  description: >-
    Are you familiar with the term "Monty Python." Veterans know what it means to
    perform this manuever. Here is an absurd weapon for use by those of you who enjoy
    both the manuever and the comedy troupe.
  download_artifact: plugin-holyhandgrenadeofantioch1-cpt
  install: []
- id: homing-rocket-sit
  label: Homing_Rocket.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-homing-rocket-sit
  install: []
- id: hyperstrictor-sit
  label: Hyperstrictor.sit
  description: >-
    Hyperstrictor features 1 new ship, 2 new systems that developed it, 2 new
    outfits bundled with it, and 1 new mission that leads you to Arajagar system. Good
    luck. This plug-in uses original EV-graphics. In the future there might be a new
    ship with new graph...
  download_artifact: plugin-hyperstrictor-sit
  install: []
- id: ifp-plug-preview-sea
  label: IFP Plug preview.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ifp-plug-preview-sea
  install: []
- id: igadzra-beam-evo-sit
  label: Igadzra Beam EVO.sit
  description: "This plug was originally developed by . This version fixes the glitch that made it be in EV only format. I repeat that this is not my plug, but it is my first attempt at using ResEdit to manipulate one. If you have any problems, send to . Note: I did say th..."
  download_artifact: plugin-igadzra-beam-evo-sit
  install: []
- id: igadzrabeam-sit
  label: IgadzraBeam.sit
  description: >-
    Have you ever wanted to use the Igadzra beam that you fought against while
    fighting for the Zidagar? Well, this plugin allows you to, after completing the
    respective Igadzra missions. Made by Lord Gwydion. Report any bugs, problems etc. to ,
    or try and find...
  download_artifact: plugin-igadzrabeam-sit
  install: []
- id: igadzrabeamfix-sit
  label: IgadzraBeamFix.sit
  description: >-
    Have you ever wanted to use the Igadzra beam that you fought against while
    fighting for the Zidagar? Well, this plugin allows you to, after completing the
    respective Igadzra missions. Made by Lord Gwydion. Report any bugs, problems et cetera
    to , or try and...
  download_artifact: plugin-igadzrabeamfix-sit
  install: []
- id: igadzracom
  label: Igadzracom
  description: >-
    This plug was created after a long debate on the EVO web bord, itfixes the
    missing Igadzra comm pict bug.
  download_artifact: plugin-igadzracom
  install: []
- id: imadoofas
  label: Imadoofas
  description: >-
    Makes fighter bays and fighters free, and changes some UE traders so they fight
    back like warships. Look for the effect in outfitter and combat situations.
  download_artifact: plugin-imadoofas
  install: []
- id: improvedgraphics-v1-0-sit
  label: ImprovedGraphics v1.0.sit
  description: >-
    For all you people who are tired of plain regular EVC EVO ships, this colorful
    collection is just for you. Right off the bat  THIS IS NOT A PLUGIN! THIS IS
    COLLECTION OF "IMPROVED" EVO EVC GRAPHICS!!! gasp Whew, unfortunately, you will need
    to use ResEdit...
  download_artifact: plugin-improvedgraphics-v1-0-sit
  install: []
- id: incomplete-sit
  label: Incomplete.sit
  description: "Incomplete is my aborted attempt at an Escape Velocity Override plug-in. If its not done, you may ask, then why in the world am I uploading it? Well, for two reasons: first, as a challenge to anyone who wants to try and complete it, and second as a source o..."
  download_artifact: plugin-incomplete-sit
  install: []
- id: iotheshuttle-sit
  label: IotheShuttle.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-iotheshuttle-sit
  install: []
- id: iotheshuttlebeta-sit
  label: IotheShuttleBeta.sit
  description: >-
    Youve all heard the "It adds one new ship" line a million times before. This
    one is a little different, because I doubt that any of you will use it. It adds the
    Iothe Shuttle, a much more powerful shuttle which is, in all respects, far
    better than the norma...
  download_artifact: plugin-iotheshuttlebeta-sit
  install: []
- id: ix-mo-sit
  label: Ix-Mo.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ix-mo-sit
  install: []
- id: ixmo1-sit
  label: IxMo1.sit
  description: >-
    This Plug-In contains 3 new ships and 4 new weapons with new graphics, feel
    free to use them in your own plug-ins. There is also a new government and a new
    system with two planets.
  download_artifact: plugin-ixmo1-sit
  install: []
- id: journeymf-sit
  label: journeymf.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-journeymf-sit
  install: []
- id: kalar-war-i
  label: Kalar War I
  description: >-
    by Tom Smith, 50 years after Escape Velocity Override, this plugin will take
    you beyond the Ji Nebula. Most people dont enjoy what they find there, though. The
    incredibly powerful and warlike Kalarians are taking on the entire civilized
    galaxy, and it looks...
  download_artifact: plugin-kalar-war-i
  install: []
- id: kamikazedrone1-2-1-sea
  label: KamikazeDrone1.2.1.sea
  description: >-
    KamikazeDrone1.2.1 -by Coraxus. A newer version of this plug includes many
    corrections of these suicidal drones. There are also bonus plug-ins to go along with
    it like the Exploding Bunnies and EVOSI compatible KD. If this is your first
    time, its worth chec...
  download_artifact: plugin-kamikazedrone1-2-1-sea
  install: []
- id: kens-voinian-war-sit
  label: Kens Voinian War.sit
  description: >-
    Adds 7 ships, several outfits and makes hidden outfits available everywhere
    outfits are sold. Features instant warp, a larger map viewing window, improved
    interface graphics like better buttons and targeting displays, and eliminates special
    missions incompa...
  download_artifact: plugin-kens-voinian-war-sit
  install: []
- id: killerforklift2-0-cpt
  label: KillerForklift2.0.cpt
  description: >-
    My name is Ryan Betzer, Age 14, And a lot of people are complaining about the
    lack of forklifts on EVO. So, How about the next best thing? I have now converted
    the Shuttle Into a FORKLIFT!
  download_artifact: plugin-killerforklift2-0-cpt
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
- id: lone-ranger-ships-sit
  label: Lone_Ranger_ships.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-lone-ranger-ships-sit
  install: []
- id: lone-ranger-ships-sit-2
  label: Lone_Ranger_ships_.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-lone-ranger-ships-sit-2
  install: []
- id: lubaria-sit
  label: Lubaria.sit
  description: >-
    I never had time to finish Lubaria if you want to continue the work then please
    do so, just read the Read Me! It was going to add many new ships and planets,
    and I got a fair bit done - it already adds about 20 ships, so I consider it a job
    well done!Any co...
  download_artifact: plugin-lubaria-sit
  install: []
- id: luna-fix-cool-music
  label: Luna Fix Cool Music
  description: >-
    Same as before but with Luna Fixer and with longer, Better, Intro Music.Its
    cool cuz now its not so boring wating for EVO to start-up 2000-2001 "Burning Ice"
    Software, inc. Thankyou....!!!!
  download_artifact: plugin-luna-fix-cool-music
  install: []
- id: lunafix-sit
  label: LunaFix.sit
  description: >-
    This plugin uses the correct custom pict of Luna from the Override data (the
    one with Earth in the background). It is compatible with most plugins that use the
    EVO galaxy.By Beau and Dallas HesterbergStrdrArgrn@aol.com6/21/20001.0
  download_artifact: plugin-lunafix-sit
  install: []
- id: magma201-sit
  label: magma201.sit
  description: "Meowx Design Studios EVO : MAGMA is the critically acclaimed EVO plug in that now has over 5000 downloads. MAGMA completely replaces the sounds and graphics from Escape Velocity Override with brand new, realistic ones. Not much is new in this, version 2.0.1..."
  download_artifact: plugin-magma201-sit
  install: []
- id: magma201u-sit
  label: magma201u.sit
  description: "Meowx Design Studios EVO : MAGMA is the critically acclaimed EVO plug in that now has over 5000 downloads. MAGMA completely replaces the sounds and graphics from Escape Velocity Override with brand new, realistic ones. Not much is new in this, version 2.0.1..."
  download_artifact: plugin-magma201u-sit
  install: []
- id: magma20u-sit
  label: magma20u.sit
  description: "If you have EVO : MAGMA version 1.0, then you can save yourself some downloading time and get this plug-in. It will update your copy of MAGMA to version 2.0."
  download_artifact: plugin-magma20u-sit
  install: []
- id: magmas101-sit
  label: magmas101.sit
  description: "The EVO : MAGMA Subwoofer pumps up the bass for the new sounds in EVO : MAGMA. Provided, of course, you have a good sound system. Subwoofer is compatible with all versions of MAGMA and may even work with it. (I havent tested it without MAGMA, though, so I d..."
  download_artifact: plugin-magmas101-sit
  install: []
- id: magmasl10-sit-bin
  label: magmasl10.sit.bin
  description: >-
    MAGMA Ship Lightener is great if you have a dark screen. If MAGMAs ships look
    too dark on your screen, simply download this plug and put it in your folder with
    MAGMA. This is version 1.0.
  download_artifact: plugin-magmasl10-sit-bin
  install: []
- id: mantis-assultship-zip
  label: mantis assultship.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-mantis-assultship-zip
  install: []
- id: marines-sit
  label: Marines.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-marines-sit
  install: []
- id: mega-booms
  label: Mega Booms
  description: >-
    This plug greatly enhances the size of the explosions in EV Overide so instead
    of little fire crackers there are great overwhelming balls of fire. 2000-2001
    "Burning Ice" Software, inc. Thankyou....!!!!
  download_artifact: plugin-mega-booms
  install: []
- id: mercenary-add-on-pack-sit
  label: Mercenary Add-On Pack.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-mercenary-add-on-pack-sit
  install: []
- id: minstrel-v-2
  label: Minstrel v.2
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-minstrel-v-2
  install: []
- id: mosiac-sit
  label: Mosiac.sit
  description: >-
    The war seems to have no end but thanks to new technologys United Earth may be
    ready to win the war. But the Vinonians have a new weapon that may foil their
    plans.I like feed back so feel free to e-mail me with comments at , and tell me
    about any bugs you f...
  download_artifact: plugin-mosiac-sit
  install: []
- id: my-plug
  label: My plug
  description: >-
    384.00 B | By Anonymous This is a Demo version of MyPlug. It only has two t
    additions. One is a mission in which you must take an Emalgha to Emalghia. Another
    is a new goverment Grahamware which will have its own solar system and its own
    planet graphic. Cre...
  download_artifact: plugin-my-plug
  install: []
- id: neutronicblaze
  label: NeutronicBlaze
  description: >-
    Ever wondered what the UE did with the Voinian technologies they aquired?
    Apparently, not much. Paaren Station seems to be little more than a retailer for a
    couple of oversized, slow-firing weapons. This plug-in, however, changes all that.
    It adds a short s...
  download_artifact: plugin-neutronicblaze
  install: []
- id: new-ship
  label: New ship
  description: >-
    This plug is a good one and has no known incompatabilities, so if you like the
    show "Tenchi Muyo" this plug adds the Ryo-oh(No new graphics yet. Sorry.)
  download_artifact: plugin-new-ship
  install: []
- id: newplanetgraphics-sit
  label: NewPlanetGraphics.sit
  description: >-
    New Planet Graphics is a plug-in which replaces the planet, and station
    graphics, it also replaces the veiws that you see when you land on a planet or station.
  download_artifact: plugin-newplanetgraphics-sit
  install: []
- id: newships
  label: NewShips
  description: >-
    This is my second plug it adds three new ship one almost avalible everywhere
    and two avalible in UE space
  download_artifact: plugin-newships
  install: []
- id: newtech2-sit
  label: NewTech2.sit
  description: >-
    Hello from Italy! Im Zampaman! My plug-in, NewTech2 is ready! I fixed all the
    bugs and now it works better. Ive added many new outfits, a new ship (the UE
    Warship), and two new systems. DOWNLOAD IT NOW!!!
  download_artifact: plugin-newtech2-sit
  install: []
- id: newtech3-sit
  label: NewTech3.sit
  description: >-
    Hi! Im Zampaman again! My New plug-in, NewTech4, is now ready! Ive added THREE
    NEW SHIPS and now the antimatter missiles cost less. Download NOW!!!
  download_artifact: plugin-newtech3-sit
  install: []
- id: newtech4-sit
  label: NewTech4.sit
  description: >-
    Hi! Im Zampaman! This is NewTech4! This time Ive added SIX NEW SHIPS, THREE NEW
    SYSTEMS and many outfits. DOWNLOAD IT!!!
  download_artifact: plugin-newtech4-sit
  install: []
- id: newtech5-sit
  label: NewTech5.sit
  description: >-
    Hi! Im Zampaman! Here is my new plug-in, with EIGHT NEW SHIPS, THREE NEW
    SISTEMS and many cool outfits that can wipe a ship in a shot! DOWNLOAD IT!!
  download_artifact: plugin-newtech5-sit
  install: []
- id: newtech6-sit
  label: NewTech6.sit
  description: >-
    This is NewTech6! It adds ONE NEW GOVERNMENT, MANY NEW SHIPS, MANY NEW SYSTEMS
    and some COOL GRAPHICS. This is a must-have plug-in. Created by Zampaman -)
  download_artifact: plugin-newtech6-sit
  install: []
- id: newtech7-sit
  label: NewTech7.sit
  description: >-
    This is NewTech7!!! Ive added MANY NEW SYSTEMS, MANY NEW OUTFITS, WEAPONS,
    SHIPS and COOL GRAPHICS!!! DOWNLOAD IT NOW!!! Zampaman Plugs
  download_artifact: plugin-newtech7-sit
  install: []
- id: newtech8-sit
  label: NewTech8.sit
  description: >-
    This is NewTech8!!! Ive added MANY NEW WEAPONS, OUTFITS, SHIPS, COOL GRAPHICS
    and now YOU CAN SEE THE VOINIAN SHIPS ARMOR LEVEL!!! Zampaman Plugs-)
  download_artifact: plugin-newtech8-sit
  install: []
- id: newtitlesongsforevo-sit
  label: NewTitleSongsforEVO.sit
  description: >-
    Is the EVO title music getting to your head?Need some thing else for a
    change?Well, Ive recorded (an composed) 2 songs.One is a rock version of the classical
    EV, the other is a completely original piece by me [hehe (!)]Hope you enjoy
    it!CarlakaBlack Beard
  download_artifact: plugin-newtitlesongsforevo-sit
  install: []
- id: niftybundle-1-0-0-sit
  label: NiftyBundle 1.0.0.sit
  description: >-
    This plug-in was created as a bundle of 17 different plug-ins, and a guide.
    They are all useful plug-ins that were very annoying to look for, so I decided to
    bundle them here. Ive also included a plug-in that is a combination of all of the
    other plug-ins. s...
  download_artifact: plugin-niftybundle-1-0-0-sit
  install: []
- id: ninnyman10-sit
  label: ninnyman10.sit
  description: >-
    This plug adds the Ninnyman fighter to the EV universe. There are plugs for
    both EV and EVO. Posted by Mouse (Ambrosia SW Webboards).
  download_artifact: plugin-ninnyman10-sit
  install: []
- id: nova-bracket-warning-sit
  label: nova_bracket_warning.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-nova-bracket-warning-sit
  install: []
- id: nuke-sit
  label: Nuke.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-nuke-sit
  install: []
- id: obsidianbuttons-sea
  label: ObsidianButtons.sea
  description: "Obsidian Buttons is a pair of free plugins: one for Escape Velocity called \"EV Obsidian Buttons\" and one for EV Override called \"EV Override Obsidian Buttons.\" They both provide replacement graphical buttons for the respective games you may prefer them to t..."
  download_artifact: plugin-obsidianbuttons-sea
  install: []
- id: orksvsimp-sit
  label: OrksvsImp.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-orksvsimp-sit
  install: []
- id: outfit-mods-sea
  label: Outfit MODs.sea
  description: >-
    Outfit MODs changes the way turrets and guns are handled, allows you to buy and
    sell any fighter bay (as long as youve done the appropriate missions), allows
    you to keep or rebuy special outfits such as the cloak, zidagar systems enhancement
    and the experim...
  download_artifact: plugin-outfit-mods-sea
  install: []
- id: ovara-plug-in-sit
  label: Ovara_Plug-In.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ovara-plug-in-sit
  install: []
- id: overflow1942-sit
  label: Overflow1942.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-overflow1942-sit
  install: []
- id: override-wars-sit
  label: Override_Wars.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-override-wars-sit
  install: []
- id: override-wars1-1
  label: Override Wars1.1
  description: >-
    This is a slight update for Override Wars as I needed to get my new email out
    and so I could get my new site out. This add-on features more missions, different
    Terran ships, and better overall performance. I will add more soon, so keep an
    eye out for anothe...
  download_artifact: plugin-override-wars1-1
  install: []
- id: overrideev
  label: OverrideEV
  description: >-
    OverrideEV is a Plug-In that I made that adds some of the Original EV ships and
    weapons to EVO. The EVO enviroment and game play are exactly the same except you
    can know play as your favorite Original EV ships!
  download_artifact: plugin-overrideev
  install: []
- id: overrideev1-5-1-sit
  label: OverrideEV1.5.1.sit
  description: >-
    Version 1.5.1 of my Plug-In OverrideEV. Ever want to play as your old Escape
    Velocity ships again, but you dont want to have to go back to the old, small, and
    boring original EV universe? Well look no further, this Plug-In is for you! It
    adds multiple ships...
  download_artifact: plugin-overrideev1-5-1-sit
  install: []
- id: paaren-station-sit
  label: Paaren Station.sit
  description: >-
    Ever thought that the research on Paaren Station is really never finished?
    Well, now you can buy all Voinian Technogoly (including ships) at Paaren Station! If
    you work for the UE, and like the Voinains ships, then download this plug-in!
  download_artifact: plugin-paaren-station-sit
  install: []
- id: paralelluniverse-zip
  label: ParalellUniverse.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-paralelluniverse-zip
  install: []
- id: perniciouspluginpak-sit
  label: PerniciousPluginPak.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-perniciouspluginpak-sit
  install: []
- id: persmaker1-sit
  label: PersMaker1.sit
  description: >-
    PersMaker 1.0 allows EVO plugin developers to automatically generate plugins
    containing pers resources based on pre-existing EVO pilot files. If the pilot in
    question has dominated any spobs, a novel govt is generated and the dominated spob
    govt is modified...
  download_artifact: plugin-persmaker1-sit
  install: []
- id: persofev3-sit
  label: persofev3.sit
  description: "Persons of #ev3, Version 1.0, First ReleaseWell here it is...This plug-in for EV Override adds the personages of many volunteers from the popular channel #ev3 on irc.ambrosia.net. Approximately 30 new people are added to the game.~Arada Pilot"
  download_artifact: plugin-persofev3-sit
  install: []
- id: personalityships
  label: PersonalityShips
  description: >-
    Ever tried to capture a personality ship like the U.E.S. Incontrovertible or
    the Z.S.S. Disruption? You may have been disappointed to find that all extra
    weapons and shields the ship has are lost upon capture. Download this plug-in to allow
    seven of the mor...
  download_artifact: plugin-personalityships
  install: []
- id: phantominterface-sit
  label: PhantomInterface.sit
  description: >-
    Tired of the same old EV interface? This plug-in changes all the buttons, the
    ship panel display, and various other graphics. This interface is a nice change,
    adding cool green graphics to planet dialogs, giving EV a new "sci-fi" look . An
    alternate ship pa...
  download_artifact: plugin-phantominterface-sit
  install: []
- id: phasedbeamfixer-sit
  label: PhasedBeamFixer.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-phasedbeamfixer-sit
  install: []
- id: planetgraphics
  label: Planetgraphics
  description: >-
    Planetgraphics changes all of the planet graphics in EVO. Planet Graphics is a
    plug which repleaces all of the planet graphics in EVO with new ones which are
    far larger. There appear to be no bugs, although some of the planets may not be the
    right size.
  download_artifact: plugin-planetgraphics
  install: []
- id: planetgraphics2-0
  label: Planetgraphics2.0
  description: >-
    Planetgraphics 2.0 changes the dull grainy planets of EVO and replaces all the
    odd angular asteroids with strikingly gorgeous new graphics created in Strata
    Studio Pro. This plugin is an absolute must have for all players of as it provides a
    much richer gra...
  download_artifact: plugin-planetgraphics2-0
  install: []
- id: plasma-1-0-1-sit
  label: Plasma 1.0.1.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-plasma-1-0-1-sit
  install: []
- id: plasma-1-0-sit
  label: Plasma 1.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-plasma-1-0-sit
  install: []
- id: plasmaboltturret-sit
  label: plasmaboltturret.sit
  description: >-
    Plasma Bolt Turret is based off of the Ishiman Carriers turret weapon from Ares
    and is very cool. In fact, I got the idea and the sprite for it from the Ishiman
    Carrier, and it shoots the same way. Its a very cool weapon in my opinion.
  download_artifact: plugin-plasmaboltturret-sit
  install: []
- id: preview-sit
  label: Preview.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-preview-sit
  install: []
- id: previewpics2-sit
  label: PreviewPics2.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-previewpics2-sit
  install: []
- id: proximaplugin
  label: ProximaPlugin
  description: >-
    This plugin only adds 2 ships and a few weaps. Its missing a few picts, feel
    free to fix this. It also needs a full mission string, but i am notoriously bad at
    that, so feel free to fix that too. Thanks to whoever made the Game Expander for
    the original EV
  download_artifact: plugin-proximaplugin
  install: []
- id: quitfe-sit
  label: quitfe.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-quitfe-sit
  install: []
- id: rapscallius-1-0-sit
  label: Rapscallius 1.0.sit
  description: >-
    The UE Frontier has participated in the war. As it seems, the tides are
    beginning to safely stay in the hands of the UE, and the universe becoming human-safe.
    In this Plug-In, seven new ships with non-stock graphics invade your local EVO
    1.0.X application....
  download_artifact: plugin-rapscallius-1-0-sit
  install: []
- id: raven103-sit
  label: Raven103.sit
  description: >-
    This plug-in contains the F7 Raven fighter w/ the Laserbolt Cannon. It also
    comes with 9 Systems w/ great trade, and 4 heavily priced trading items.
  download_artifact: plugin-raven103-sit
  install: []
- id: rebel-folder-v-1-0-0-sit
  label: Rebel_Folder_v._1.0.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-rebel-folder-v-1-0-0-sit
  install: []
- id: red-nose-ships-2-0-sit
  label: Red Nose Ships 2.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-red-nose-ships-2-0-sit
  install: []
- id: redeeming1-0-sea
  label: Redeeming1.0.sea
  description: >-
    Redeem Plug-in 1.0 by Coraxus - People hate it when they attack the bad guys
    for a particular government only to find out that there is this one system that
    goes anal for no reason at all and its hard to remove the bad record off. This
    plug-in lets you do t...
  download_artifact: plugin-redeeming1-0-sea
  install: []
- id: refundplug-in-sea
  label: refundplug-in.sea
  description: >-
    Refund Plug-In 1.0.0 makes unsellable, expensive items which takes up alot of
    mass, or cargo space available to be sold back to corresponding outfitting shops.
    This plug-in also lets you sell back cargo pods and mass expansions too. (sorry
    if someone had th...
  download_artifact: plugin-refundplug-in-sea
  install: []
- id: reign-ii-shades-of-evil-sit
  label: Reign II- Shades of Evil.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-reign-ii-shades-of-evil-sit
  install: []
- id: reign-of-the-voinians-sit
  label: Reign of the Voinians.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-reign-of-the-voinians-sit
  install: []
- id: reign-of-the-ue-1-1
  label: Reign_of_the_UE_1.1
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-reign-of-the-ue-1-1
  install: []
- id: reign-of-the-ue-1-1no-music
  label: Reign_of_the_UE_1.1No_Music
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-reign-of-the-ue-1-1no-music
  install: []
- id: reignoftheue
  label: ReignOfTheUE
  description: >-
    With the Emalgha/UE alliance, the destruction of the Dreadnought and the
    liberation of the Hinwar, you have begun the destruction of the Voinian Empire... Now,
    its time to finish the job. Reign of the UE picks up where the central UE
    objective left off, con...
  download_artifact: plugin-reignoftheue
  install: []
- id: reignoftheue-fix-sea
  label: ReignOfTheUE fix.sea
  description: "Reign of the UE 1.0 Fix is for UE Patriots plug-in Reign of the UE. Reign of the UE Fix corrects two small bugs with Reign of the UE 1.0 that concerned the new intro PICT. NOTE: ROTUE 1.0 Fix is not an update to Reign of the UE. You must have Reign of the U..."
  download_artifact: plugin-reignoftheue-fix-sea
  install: []
- id: reignoftheuev12
  label: ReignOfTheUEv12
  description: >-
    With the Emalgha/UE alliance, the destruction of the Dreadnought and the
    liberation of the Hinwar, you have begun the destruction of the Voinian Empire... Now,
    its time to finish the job. Reign of the UE picks up where the central UE
    objective left off, con...
  download_artifact: plugin-reignoftheuev12
  install: []
- id: reignofthevoinians-sit
  label: ReignOfTheVoinians.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-reignofthevoinians-sit
  install: []
- id: renegade-sit
  label: renegade.sit
  description: >-
    This plugin is for renegades. It adds one ship, The Renegade Destroyer. It can
    take down a UE Destroyer fairly easily. I think I got most of the bugs out so Im
    posting it. Expect more soon.
  download_artifact: plugin-renegade-sit
  install: []
- id: renegadevessels1-0-1-sea
  label: RenegadeVessels1.0.1.sea
  description: >-
    RenegadeVessels1.0.1 -by Coraxus. This new version with a set of three new
    ships for the independent government and the human renegades comes with 2 more
    compatible plug-ins for EVOSI and 45ºSprite.
  download_artifact: plugin-renegadevessels1-0-1-sea
  install: []
- id: rengade-pocket-warship-zip
  label: rengade pocket warship.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-rengade-pocket-warship-zip
  install: []
- id: rotue-invincinought-fix
  label: ROTUE InvinciNought Fix
  description: >-
    Ever wonder why, while playing "Reign of the UE," and you must destroy the
    InvinciNought, it flies off to the side of the screen just after you disabled it? It
    was an oversight by the designer (Mea culpa), but this plug fixes it! Compatible
    with any version...
  download_artifact: plugin-rotue-invincinought-fix
  install: []
- id: saboteursrealm-sea
  label: SaboteursRealm.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-saboteursrealm-sea
  install: []
- id: saboteursrealm10b4r3
  label: SaboteursRealm10b4r3
  description: >-
    The story of an Earth exploration mission gone wrong, which evolves in to a
    huge epic of war, betrayal, sacrifice, and redemption. With 50 missions of plot
    twists, mission forks, and main characters, as well as tons of custom ships,
    weaponry, and graphics,...
  download_artifact: plugin-saboteursrealm10b4r3
  install: []
- id: sadefixer-sit
  label: SADEFixer.sit
  description: "768.00 B | By Anonymous Although EVO v1.0.2 brought a lot of cool features, it also carried a bizarre bug: SADs and SAEs would flicker occasionally while in flight. After a little work in ResEdit, I created this plug to fix that problem. Send in all comment..."
  download_artifact: plugin-sadefixer-sit
  install: []
- id: seans-wacky-world
  label: seans wacky world
  description: >-
    Adds a large, intentionally chaotic set of changes. Expect the differences to
    show up after starting a pilot and exploring altered content.
  download_artifact: plugin-seans-wacky-world
  install: []
- id: secession-sit
  label: Secession.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-secession-sit
  install: []
- id: secession15-sit
  label: Secession15.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-secession15-sit
  install: []
- id: shifter-sit
  label: Shifter.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-shifter-sit
  install: []
- id: shifter2-0-sit
  label: Shifter2.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-shifter2-0-sit
  install: []
- id: ships-r-us-sit
  label: Ships R Us.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ships-r-us-sit
  install: []
- id: ships-r-us1-1-sit
  label: Ships R Us1.1.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ships-r-us1-1-sit
  install: []
- id: shortcuts-plug-sit
  label: Shortcuts plug.sit
  description: >-
    This plug in adds a lot of shortcuts very helpful in most of the missions.Its
    not very compatible, so I recomend you to use it alone.Hope you enjoy it.
  download_artifact: plugin-shortcuts-plug-sit
  install: []
- id: shuttle-dx-sit
  label: Shuttle_DX.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-shuttle-dx-sit
  install: []
- id: silent-killer-sit
  label: silent killer.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-silent-killer-sit
  install: []
- id: solgraphs-sit
  label: SolGraphs.sit
  description: "This plug-in adds new graphics to the Sol system, made with real photos and others created artificially. If you have any problems with my plug, e-mail me at: you enjoy it."
  download_artifact: plugin-solgraphs-sit
  install: []
- id: soundsgalore-sea
  label: SoundsGalore.sea
  description: >-
    SoundsGalore1.0.0 -by Coraxus. While I have not submitted anything for a long
    while, here is something I made long ago but didnt release it until now. Its
    basically a plug-in that replaces the general sound effects, mostly explosions and
    weapons fire. I hop...
  download_artifact: plugin-soundsgalore-sea
  install: []
- id: space-amoeba-1-0-sit
  label: Space Amoeba 1.0.sit
  description: >-
    Where did they come from? Why are they here? What do they hope to achieve? No
    one knows. They are the Space Amoebas, and they lurk in the most desolate corners
    of the Galaxy. Their deadly Crystal Rays are feared by all. This plug-in adds one
    ship, (or shoul...
  download_artifact: plugin-space-amoeba-1-0-sit
  install: []
- id: spaceballs-the-plug-in
  label: Spaceballs- The Plug-In
  description: >-
    Adds two Spaceballs-themed planets in a new system one jump from Sol, plus
    related outfits and local trade opportunities.
  download_artifact: plugin-spaceballs-the-plug-in
  install: []
- id: speed-plug-1-0-sit
  label: Speed Plug 1.0.sit
  description: >-
    WARNING! This plug is for expert pilots only! May cause newbies to melt upon
    startup! It speeds all the ships up making it an extra challenge to those whove
    mastered the game.
  download_artifact: plugin-speed-plug-1-0-sit
  install: []
- id: speedchange-sea
  label: SpeedChange.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-speedchange-sea
  install: []
- id: speedchange-2-0-sea
  label: SpeedChange_2.0.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-speedchange-2-0-sea
  install: []
- id: spodiaships-sit
  label: SpodiaShips.sit
  description: >-
    Spodia Ships- v1.0.0by Nick KasprakThank you for downloading the Spodia Ships
    plugin. It adds 6 new ships and a few weapons (with original graphics) to the EVO
    universe. I am not good at storylines or descriptions or systems or anything like
    that, so I have...
  download_artifact: plugin-spodiaships-sit
  install: []
- id: star-wars-sea
  label: Star Wars.sea
  description: >-
    This is a fix for the infamous circle of lasers and torpedoes spinning in a
    halo around the player. The problem was caused by too many separate plugs in the
    Star Wars set. I combined the separate plugs into one plug and it now works fine.
    Problem solved. Th...
  download_artifact: plugin-star-wars-sea
  install: []
- id: star-trek-preview-sit
  label: Star Trek Preview.sit
  description: >-
    This plug-in is a preview for a plug that could contain an entire Star Trek
    galaxy, if its well received. Adds the Galaxy, Sovereign, Miranda, Intrepid, and
    Defiant class starships as well as Phaser Banks, Arrays, Photon, and Quantum Torps.
    Feedback is appr...
  download_artifact: plugin-star-trek-preview-sit
  install: []
- id: starclashov10
  label: StarclashOv10
  description: >-
    This plug-in adds the Protoss carrier and Terran Battlecruiser to Override. Its
    sort of a port of the one I made for Nova. You Stuffit Expander 6.5.1 to
    unstuff. Contact me -David "Kings Knight" Freeman
  download_artifact: plugin-starclashov10
  install: []
- id: starring-zip
  label: starring.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-starring-zip
  install: []
- id: starterplug1-sit
  label: StarterPlug1.sit
  description: >-
    The Starter Plug adds a few missions (with high rewards) tied into the Destroy
    Voinian Dreadnought mission series, and makes a few modifications (e.g. to the
    Miranu Heavy Freighter), all to aid players who don19t have 20 hours a day free to
    play EV Override.
  download_artifact: plugin-starterplug1-sit
  install: []
- id: starwars2103-sit
  label: StarWars2103.sit
  description: This is a port of the EV scenario Star Wars 2.01 to EV Override.
  download_artifact: plugin-starwars2103-sit
  install: []
- id: stockerpods-sit
  label: StockerPods.sit
  description: >-
    This is my first plug-in ever so I hope it works the plug adds one new ship the
    X-Caper and a weapon that you get with the ship the amazign Stocker Pods an
    extremling intelligent homing device. sorry about the graphics I copy the all from
    other ships and we...
  download_artifact: plugin-stockerpods-sit
  install: []
- id: strandvessel1-0-1-sea
  label: Strandvessel1.0.1.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-strandvessel1-0-1-sea
  install: []
- id: strandvessels1-1-sea
  label: StrandVessels1.1.sea
  description: "StrandVessels1.1 -by Coraxus. This plug-in that contains a number of new ships for each strands including Zachit, now has improved more than ever. Other than correcting mistakes and configurating one ship, Ive also added bonus plug-ins as well. Important: R..."
  download_artifact: plugin-strandvessels1-1-sea
  install: []
- id: strangeasteroid-sit
  label: StrangeAsteroid.sit
  description: >-
    This mission replaces the placeholder graphics for the Strange Asteroid in the
    Miranu/Igadzra mission sequences to make the mission, as EV Override designer
    Peter Cartwright put it, "make sense at last (or at least...make as little sense as
    it was meant to...
  download_artifact: plugin-strangeasteroid-sit
  install: []
- id: stressrelease-sit
  label: StressRelease.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-stressrelease-sit
  install: []
- id: striker-plug-in-sit
  label: Striker plug-in.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-striker-plug-in-sit
  install: []
- id: super-plug-in-pak-sit
  label: Super Plug-In pak.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-super-plug-in-pak-sit
  install: []
- id: super-stuff-2-0-sit
  label: Super Stuff 2.0.sit
  description: >-
    Super Stuff 2.0Hi! Sorry about the bugs in the first version, hopefully they
    have been eliminated.
  download_artifact: plugin-super-stuff-2-0-sit
  install: []
- id: superlazira-sit
  label: SuperLazira.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-superlazira-sit
  install: []
- id: superships-1-2-sea
  label: Superships 1.2.sea
  description: "This plug will make it so you you can see Voinian dreadnoughts and UE Cruisers flying around, attacking outposts,etc. It will also give the Hinwar dreadnoughts. WARNING: this plug may not function correctly unless you have done the following: completed the..."
  download_artifact: plugin-superships-1-2-sea
  install: []
- id: sw-conf-plug-in-sit
  label: SW Conf. plug-in.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-sw-conf-plug-in-sit
  install: []
- id: sw2-2-0-0
  label: SW2 2.0.0
  description: >-
    SW2 2.0! This is a fixed(hopefully) version of Chris Hausers SW2. It fixes
    those annoying things and broken things and should work better than Version 1.0.3.
    Please email bug reports to
  download_artifact: plugin-sw2-2-0-0
  install: []
- id: sw2-2-1-sit
  label: SW2 2.1.sit
  description: >-
    Now, hopefully this loads right. I have tried about two times to put this on
    this site. Maybe third times the charm! Alright. This is an update for SW2 1.0.3.
    IT fixes a few things, blah blah blah...
  download_artifact: plugin-sw2-2-1-sit
  install: []
- id: sw221-sit
  label: SW221.sit
  description: >-
    Alright, hopefully this will upload correctly. SW2 2.1 is a major bug fix for
    SW2 1.0. You do not need SW2 1.0.0-3. You need EVO 1.0.2 to run this though. This
    should fix many of the bugs that were either hindering or annoying.Reeves Software
  download_artifact: plugin-sw221-sit
  install: []
- id: swextraships-sit
  label: SWextraShips.sit
  description: >-
    Adds TIE Advance x1, TIE Defender, Corellian Transport/freighter to SW2, This
    is just a plug to spice up your gameplay(need target Pics though, this is not
    finished) Will add more ships, outfits, fleets, persons, dudes, in later versions if
    you like this. P...
  download_artifact: plugin-swextraships-sit
  install: []
- id: swextrashipsv-2-sit
  label: SWextraShipsV.2.sit
  description: >-
    Adds Target Pics,TIE Defender,TIE Advance,Medium Transport,corellian freighter,
    and Y-wing to SW2.Questions or Comments to , my other e-mail was down. Looking
    for someone to make some new SW ships, respond if interested.(All ships added were
    created by Kris...
  download_artifact: plugin-swextrashipsv-2-sit
  install: []
- id: tbfl-test-version-sit
  label: TBFL test version.sit
  description: >-
    This is The Better Fork-Lift Testing Version. It is also my first submission. I
    really want to spend more timne designing than testing so any one who wants to
    help, please do.
  download_artifact: plugin-tbfl-test-version-sit
  install: []
- id: teddy-trouble-sit
  label: Teddy-Trouble.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-teddy-trouble-sit
  install: []
- id: the-traders-union-1-0-sit
  label: The Traders Union 1.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-the-traders-union-1-0-sit
  install: []
- id: the-assailant-preview-sit
  label: The_Assailant_Preview.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-the-assailant-preview-sit
  install: []
- id: thealliedpowers-sit
  label: TheAlliedPowers.sit
  description: >-
    This is my first shot at plug ins for EVO. This plug-in evens the score, so
    that UE doesnt dominate the space anymore. This plug-in allies Emalgha, renegades,
    and Voinians, vs the UE. I also upgraded a lot of the ships in the game. Any
    comments can go to ,...
  download_artifact: plugin-thealliedpowers-sit
  install: []
- id: thefourthreich1-0-sea
  label: TheFourthReich1.0.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-thefourthreich1-0-sea
  install: []
- id: thereaper-zip
  label: TheReaper.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-thereaper-zip
  install: []
- id: theundiscoveredcountryteaser-sit
  label: TheUndiscoveredCountryTeaser.sit
  description: "Here is a teaser of my upcoming plugin, The Undiscovered Country. Included: 2 new outfits, some weapons, and modifications to the Voinian Frigate and the UE Destroyer to make for some interesting battles. There is also a preview of the new interface. Check..."
  download_artifact: plugin-theundiscoveredcountryteaser-sit
  install: []
- id: thorrion1-sit
  label: Thorrion1.sit
  description: >-
    Thorrion 1.0 adds a new system and a new mission. The mission plays a MooV on
    completion. For those Monty Python afficiandos, it is a take off the Spam Sketch.
  download_artifact: plugin-thorrion1-sit
  install: []
- id: topsmods2-sit
  label: TOPsMods2.sit
  description: >-
    T.O.P.19s Mods 2.0 is a pair of background plugs for EV Override. They modify
    the game to provide a richer, more challenging playing environment for the
    experienced player. Version 2.0 is compatible with "Beyond The Crescent" and other
    current plugs for EVO...
  download_artifact: plugin-topsmods2-sit
  install: []
- id: torture-sit
  label: torture.sit
  description: "This plug changes the bounty hunters to the most feared ship in the game. EV: Changes bounty hunter from Kestrel to Alien Cruiser. EVO: Changes bounty hunter from Crescent Warship to Voinian Dreadnought. Dont download unless you have no life! Youve been war..."
  download_artifact: plugin-torture-sit
  install: []
- id: transferic-shipyards-demo
  label: Transferic Shipyards Demo.
  description: "Yeah, im new in the web, but ive made millions of plugins before, only they didnt get to the web. Anyway, im making a saga of a race called: Akilae, and my first part of it will be soon in the air. It will be called \"Transferic Shipyards\". Well, i was makin..."
  download_artifact: plugin-transferic-shipyards-demo
  install: []
- id: transferic-shipyards-sit
  label: Transferic Shipyards.sit
  description: "THE FULL VERSION OF TRANSFERIC SHIPYARDS IS HERE!!!! IF YOU ARE INTERESTED IN IT, DOWNLOAD IT!!!!IF YOU HAVE TROUBLE, EMAIL ME TO:"
  download_artifact: plugin-transferic-shipyards-sit
  install: []
- id: traveler10-sea
  label: traveler10.sea
  description: >-
    The latest plugin from the maker of Delta Corp. (formerly Delta Fighter), this
    adds a ship, a weapon, and some more programming experience.. :)
  download_artifact: plugin-traveler10-sea
  install: []
- id: travelers-superpack-sea
  label: Travelers_Superpack.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-travelers-superpack-sea
  install: []
- id: tripleagent15-sit
  label: TripleAgent15.sit
  description: >-
    Do you want to play more than one side of the Strand war? This plug-in will
    allow you to do it! Just find and accept the mission and you are off. Despite what I
    said I released a new version, but it ONLY contains new contact information plus
    some editing of...
  download_artifact: plugin-tripleagent15-sit
  install: []
- id: tripleagent151-cpt
  label: TripleAgent151.cpt
  description: >-
    Do you want to play more than one side of the Strand war? This plug-in will
    allow you to do it! Just find and accept the mission and you are off. Despite what I
    said I released a new version, but it ONLY contains new contact information plus
    some editing of...
  download_artifact: plugin-tripleagent151-cpt
  install: []
- id: true-pers-sea
  label: True Pers.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-true-pers-sea
  install: []
- id: ue-dreadnought-sit
  label: UE Dreadnought.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ue-dreadnought-sit
  install: []
- id: ue-interceptor
  label: UE Interceptor
  description: "This plug adds a new ship to every shipyard of the UE.Its also compatible with \"Voinian Speed-Fighter\".If you want to mail me:"
  download_artifact: plugin-ue-interceptor
  install: []
- id: ue-prototype-plug1-0-1-sit
  label: UE Prototype Plug1.0.1.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ue-prototype-plug1-0-1-sit
  install: []
- id: ue-prototypes-sit
  label: UE Prototypes.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ue-prototypes-sit
  install: []
- id: ue-ship-and-outfit-expander
  label: UE Ship and Outfit Expander
  description: >-
    Updates to Version 1.5 Fixes the small bugs that plauged the previous release.
    UE Ship and Outfit Expander Version 1.5 adds the much needed variety of ships and
    outfits to the UE in EVO. Within this Plug you will find many of the first games
    beloved ships,...
  download_artifact: plugin-ue-ship-and-outfit-expander
  install: []
- id: ue-phase-ii-1-0-2-sit
  label: UE Phase II 1.0.2.sit
  description: >-
    The last update to UE Phase II! I made 6 new ships. 4 of them are upgrades, 2
    of them are new. Now the UE Carrier has a better chance of defeating a Voinian
    Crusier. Watch them in action. Also, a bonus bay for the new ship. In this version I
    added 1 new shi...
  download_artifact: plugin-ue-phase-ii-1-0-2-sit
  install: []
- id: uedreadnaught-sit
  label: UEDreadnaught.sit
  description: >-
    This is my first plugin for EV/EVO/EV3 (hopefully). It adds a new ship, the UE
    Dreadnaught.(I apologize if anyone else took this ship title. Sorry!). This ship
    is superb, but you must get all the weapons yourself. This will be changed in a
    upcoming upgrade,...
  download_artifact: plugin-uedreadnaught-sit
  install: []
- id: uedreadnought-sit
  label: UEDreadnought.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-uedreadnought-sit
  install: []
- id: uenavy1-1-sea
  label: UENavy1.1.sea
  description: >-
    UENavy1.1 -by Coraxus. This is the updated version of the UE Navy.
    Reconfiguration of both the UE battleship and the UE interceptor were dramatically improved.
    As usual with new updates, the UE Navy also has extra goodies.
  download_artifact: plugin-uenavy1-1-sea
  install: []
- id: uephaseii-sit
  label: UEPhaseII.sit
  description: >-
    Ever wanted to make some UE ships better? Now they are. I made 5 new ships. 4
    of them are upgrades, 1 of them is new. Now the UE Carrier has a better chance of
    defeating a Voinian Crusier. Watch them in action. Also, a bonus bay for the new
    ship.
  download_artifact: plugin-uephaseii-sit
  install: []
- id: uephaseii1-0-1-sit
  label: UEPhaseII1.0.1.sit
  description: >-
    Ever wanted to make some UE ships better? Now they are. I made 6 new ships. 5
    of them are upgrades, 1 of them is new. Now the UE Carrier has a better chance of
    defeating a Voinian Crusier. Watch them in action. Also, a bonus bay for the new
    ship. In this ve...
  download_artifact: plugin-uephaseii1-0-1-sit
  install: []
- id: ultima-1-0-1-sea
  label: Ultima 1.0.1.sea
  description: "Ultima is a medium-small plug-in which throws you into the heart of a struggle between a budding military government, the Ultima, and the renegades who plague their systems. You can help Ultima in many ways: as a trader, set up a collection of tourist plane..."
  download_artifact: plugin-ultima-1-0-1-sea
  install: []
- id: ultima-1-0-2-sea
  label: Ultima 1.0.2.sea
  description: "Ultima is a medium-sized plug-in which throws you into the heart of a struggle between a budding military government, the Ultima, and the renegades who plague their systems. You can help Ultima in many ways: as a trader, set up a collection of tourist plane..."
  download_artifact: plugin-ultima-1-0-2-sea
  install: []
- id: ultima-1-0b3-sea
  label: Ultima 1.0b3.sea
  description: >-
    Ultima is a medium-small plug-in which adds eight new ships, twelve new
    outfits, six new systems (ruled by one highly aggressive new government), and more. Much
    more has been added since beta2, and all known bugs have been exterminated.
    While Ultima will ne...
  download_artifact: plugin-ultima-1-0b3-sea
  install: []
- id: ultima-1-0-4-sit
  label: Ultima 1.0.4.sit
  description: "Ultima is a medium-sized plug-in which throws you into the heart of a struggle between a budding military government, the Ultima, and the renegades who plague their systems. You can help Ultima in many ways: as a trader, set up a collection of tourist plane..."
  download_artifact: plugin-ultima-1-0-4-sit
  install: []
- id: ultima-103-sea
  label: Ultima 103.sea
  description: "Ultima is a medium-sized plug-in which throws you into the heart of a struggle between a budding military government, the Ultima, and the renegades who plague their systems. You can help Ultima in many ways: as a trader, set up a collection of tourist plane..."
  download_artifact: plugin-ultima-103-sea
  install: []
- id: ultimate-customize
  label: Ultimate Customize
  description: >-
    Ultimate customize is for players who love to make perfect ships, but hate to
    fly around the galaxy picking up outfits everytime they get a new ship. It adds
    one new system with 4 planets, 2 new dudes, a few links between faraway systems
    starting from the n...
  download_artifact: plugin-ultimate-customize
  install: []
- id: ultimate-armory-1-sit
  label: ultimate_armory_1.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ultimate-armory-1-sit
  install: []
- id: ultimate-fleet-zip
  label: Ultimate_Fleet.zip
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-ultimate-fleet-zip
  install: []
- id: und112-sea
  label: UND112.sea
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-und112-sea
  install: []
- id: universenextdoor112-sea
  label: UniverseNextDoor112.sea
  description: >-
    The strands discover the true nature of the council, and unite to destory it.
    Travel is allowed beyond the rim of the crescent. What lurks past those desolate
    worlds on the rim? Only you, the enteprising captain who set the fall of the
    council into motion,...
  download_artifact: plugin-universenextdoor112-sea
  install: []
- id: upwithue-1-0-sit
  label: UpWithUE 1.0.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-upwithue-1-0-sit
  install: []
- id: variousevotweaks-sit
  label: VariousEvoTweaks.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-variousevotweaks-sit
  install: []
- id: varterwarship
  label: VarterWarship
  description: >-
    This plug adds one new ship to the UE system. This fix some minor bugs. Version
    1.1 Comments can be send to
  download_artifact: plugin-varterwarship
  install: []
- id: varterwarship-sit
  label: VarterWarship.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-varterwarship-sit
  install: []
- id: voinain-supercrusier-sit
  label: Voinain SuperCrusier.sit
  description: >-
    Ever thought that there was never a good ship for the Voinains? Well, now there
    is! I added one new ship, with totally new graphics! Download it to have a
    challenge with the UE, or the crush with the Voinains!
  download_artifact: plugin-voinain-supercrusier-sit
  install: []
- id: voinian-speed-fighter
  label: Voinian Speed-Fighter
  description: "This is a simple plug for EVO.You can buy a new ship at every shipyard of the Voinians.If someone interested eMail me:"
  download_artifact: plugin-voinian-speed-fighter
  install: []
- id: voinian
  label: Voinian
  description: >-
    This plug combines Kens Voinian War with other plugs and elements of other
    plugs to make a single plug with no bugs. Now you can use Meowx Design Studios MAGMA
    with a bigger map window, the weapons fix plug, DispArmorPercent, and have some
    of the better shi...
  download_artifact: plugin-voinian
  install: []
- id: voinian-quickstart-sit
  label: Voinian_Quickstart.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-voinian-quickstart-sit
  install: []
- id: weapons-fixer-sit
  label: Weapons_Fixer.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-weapons-fixer-sit
  install: []
- id: weapons-fixer1-0-1-sit
  label: Weapons_Fixer1.0.1.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-weapons-fixer1-0-1-sit
  install: []
- id: weincomplete-sit
  label: WEIncomplete.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-weincomplete-sit
  install: []
- id: wepons-galor
  label: Wepons Galor
  description: >-
    this plug gives you more weponsand this is NOT by the same person who did
    armorygalor this is by corey heffernan
  download_artifact: plugin-wepons-galor
  install: []
- id: wowser-sit
  label: Wowser.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-wowser-sit
  install: []
- id: xtreme-tech-1-1-sit
  label: Xtreme Tech 1.1.sit
  description: >-
    This is the second version of my plug, "Xtreme Tech".It adds a new title song,
    new sounds, outfits n weapons.This file contains a complete read-me.Hope you
    enjoy it.Sirio
  download_artifact: plugin-xtreme-tech-1-1-sit
  install: []
- id: xtreme-tech-sit
  label: Xtreme Tech.sit
  description: >-
    This plug-in adds a lot of new outfits like Xentronium armor and Torpedo
    turret, etc. a very cool new ship, the Extreme Warship and other features.I hope you
    enjoy it.
  download_artifact: plugin-xtreme-tech-sit
  install: []
- id: xtremetechcompletread-sit
  label: XtremeTechcompletread.sit
  description: >-
    This is the complete read-me file of Xtreme Tech.If you downloaded this plug,
    download this too.Sorry, I put a resumed read-me in the plug-in folder, but this
    is the complete one.Enjoy my plug.
  download_artifact: plugin-xtremetechcompletread-sit
  install: []
- id: zachit-missions
  label: Zachit Missions
  description: >-
    Did you ever feel that the Zachit mission string kinda leaves you hanging?
    Well, if you answered yes, then this is the plug for you! Zachit Missions adds 14 new
    missions to the Zachit string. The graphics in this plug-in were created by Ian
    Raymond, and I t...
  download_artifact: plugin-zachit-missions
  install: []
- id: zachit-ships-sit
  label: Zachit_Ships.sit
  description: >-
    Original EVO plugin package from the Archive.org Escape Velocity Plugin
    Collection.
  download_artifact: plugin-zachit-ships-sit
  install: []
- id: zachitcruiser-sit
  label: ZachitCruiser.sit
  description: >-
    This plugin adds a new ship, the Zachit Cruiser, available on New Mira. The
    Zachit Cruiser has two disco machine guns, a zachit fighter bay with 10 zachit
    fighters, a zachit cloak, a zachit marine upgrade, and a zachit ECM system. Fuel is
    set to regenerate...
  download_artifact: plugin-zachitcruiser-sit
  install: []
- id: zachitships1-0-sit
  label: ZachitShips1.0.sit
  description: >-
    Zachit Ships 1.0 adds Zachit Ships so you can buy their ships after doing a few
    of the Zachit Missions. Version 1.0 -By Taylor (AKA Onii7)
  download_artifact: plugin-zachitships1-0-sit
  install: []
- id: zachitships14-sit
  label: ZachitShips14.sit
  description: >-
    As with my other plugs this is the latest version and it fixes some bugs and
    compatability issues. This plug just adds six new ships and a couple outfits (no
    new graphics, yet) available at Zachit Outpost after the completion of the Miranu
    Gunship missions....
  download_artifact: plugin-zachitships14-sit
  install: []
references:
- https://macintoshgarden.org/games/escape-velocity-override
---

## Source and version

This is the untouched [Escape Velocity Override 1.0.2 installer](https://assets.systemless.org/catalogue/objects/sha256/a5/a51a55c5df6dbb1eab41d1840ffdcb47a9857190db510a37ac1a695277744932.bin), preserved in its original MacBinary form. Systemless opens the installer and finds the game inside it at launch; nothing has been pre-installed or repacked for the web.

## Plug-ins

Override inspired an enormous library of ships, stories and total conversions. The collection below points to those original packages, exactly as their authors distributed them. Automatic installation is deliberately unavailable until Systemless can understand the packages without turning their contents into catalogue-made files.

## Gameplay

![Escape Velocity Override gameplay](https://assets.systemless.org/catalogue/media/sha256/c6/c6d8cb51221937000fbed3c98cbc37e5c768fdd49d8671524c58b5475beac183.png)

## Life in the Crescent

The Crescent is not simply a larger map for the first game. Human expansion has reached the Miranu and their neighbours while the long UE–Voinian war shapes the frontier. Trading and small jobs still pay for your first upgrades, but the loyalties you build through missions decide which ships, outfits and corners of the story open to you.

Use the arrow keys to steer, **L** to select and land, **M** for the map, and **J** to jump once clear of the system centre.
