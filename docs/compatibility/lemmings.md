# Lemmings 1.5.2 compatibility

Status: verified on Systemless 0.6.0 (26 July 2026)

Systemless runs the 68k Macintosh release of Lemmings 1.5.2 from launch into
live gameplay, including selecting and assigning a Builder. The verification
used an independently obtained StuffIt archive with SHA-256
`c3e51e74044cdc15d82b5467d700dcdddae1d147ea9cc3dfb194d58fe82caa07`
and selected the `Lemmings/Lemmings` application.

The archive is not part of Systemless and is not redistributed by this
project.

## Verified flow

The compatibility run covers a state-changing gameplay action:

1. Open the Psygnosis title and dismiss the introductory notices.
2. Reach the main menu and choose **Let's Go!**.
3. Open Level 1, **Just dig!**, and start the level.
4. Observe lemmings emerge and walk across the live level.
5. Select the Builder skill and assign it to a walking lemming.
6. Confirm that the available Builder count decreases and a staircase begins.

The same flow was replayed against Mac OS 7 under BasiliskII. Both runtimes
reached the live level and accepted the Builder assignment. Spawn timing,
walker count, cursor position, and the exact staircase pixels are deliberately
not treated as frame-deterministic.

## Runtime contract

No title-specific dispatch or compatibility branch is required. The verified
flow exercises these generic Systemless contracts:

- StuffIt extraction and VFS-relative access to the application and companion
  graphics, level, and music files;
- Resource Manager loading from the application resource fork;
- Event Manager mouse movement, button state, and polling during menus and
  live gameplay;
- QuickDraw, Color QuickDraw, palette, region, text, and cursor rendering on
  the game surface;
- TickCount progression and generic acceleration of recognized guest
  spin-wait instruction patterns.

Source comments may identify Lemmings as a witness for a generic behavior, but
the behavior itself must remain application-independent. Future fixes must not
check the application name, creator code, archive filename, executable path,
or game-specific addresses.

## Reproduction

With a legally obtained matching archive:

```sh
cargo run --release -- path/to/Lemmings.sit
```

Complete the verified flow above and confirm that assigning a Builder changes
both the skill count and live level state. For runtime changes, run the public
repository test matrix:

```sh
cargo test --lib
cargo test --lib --features test-support
cargo check --no-default-features
cargo package
```
