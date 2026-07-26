# Wolfenstein 3D: First Encounter 1.0.1 compatibility

Status: verified on Systemless 0.6.0 (26 July 2026)

Systemless runs the 68k Macintosh release of Wolfenstein 3D: First Encounter
1.0.1 from launch into state-changing first-person gameplay. The verification
used an independently obtained StuffIt archive with SHA-256
`99f1f2e1fd1a8e2fe53504ea38ec7017c234033cdbc19228778eb07ba3598256`
and selected the `Wolfenstein 3D/Wolfenstein 3D` application.

The archive is not part of Systemless and is not redistributed by this
project.

## Verified flow

The compatibility run covers more than application startup:

1. Open the shareware notice and introductory title sequence.
2. Reach the Wolfenstein 3D: First Encounter title screen.
3. Open **New Game**, choose a difficulty, and enter Floor 1.
4. Move forward from the starting position and observe the first-person view
   advance to the door.
5. Fire the pistol and confirm that the ammunition counter decreases from 16
   to 15.

The same flow was replayed against Mac OS 7 under BasiliskII. Both runtimes
reached controllable Floor 1 gameplay, moved to the starting door, and accepted
weapon fire. Three gameplay checkpoints measured 95.61–97.96% strict
whole-frame pixel parity. Introductory screen timing is deliberately not
treated as frame-deterministic.

## Runtime contract

No title-specific dispatch or compatibility branch is required. The verified
flow exercises these generic Systemless contracts:

- StuffIt extraction and selection of runnable 68k `CODE` resources from a fat
  Macintosh application;
- Resource Manager and File Manager access for application resources and the
  preferences file;
- AppleEvent application-open delivery during startup;
- Memory Manager support for the application's requested partition and
  offscreen drawing allocations;
- Color QuickDraw, palette updates, cursor visibility, and direct screen-backed
  drawing on an 8-bit display;
- Event Manager key events and low-memory key-state synchronization for held
  movement and fire controls;
- TickCount progression during introductory animation and live gameplay.

Future fixes for this game must preserve those generic contracts. Do not add
checks for the application name, creator code, archive filename, executable
path, or game-specific addresses.

## Reproduction

With a legally obtained matching archive:

```sh
cargo run --release -- path/to/Wolfenstein-3D.sit
```

Complete the verified flow above and confirm that movement changes the
first-person view and firing decrements the ammunition counter. For runtime
changes, run the public repository test matrix:

```sh
cargo test --lib
cargo test --lib --features test-support
cargo check --no-default-features
cargo package
```
