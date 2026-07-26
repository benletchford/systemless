# The Oregon Trail 1.3 compatibility

Status: verified on Systemless 0.6.0 (26 July 2026)

Systemless runs the 68k Macintosh release of The Oregon Trail 1.3 through
party setup, purchasing, departure, and live trail travel. The verification
used an independently obtained StuffIt archive with SHA-256
`654a7a74a2bf4922638baa07096b744b49d24f5c6583e4bdffe5f37a65059357`
and selected the `Oregon Trail/Oregon Trail` application.

The archive is not part of Systemless and is not redistributed by this
project.

## Verified flow

The compatibility run covers more than application startup:

1. Open the title screen and choose **Travel the Trail**.
2. Enter the party leader's name and select an occupation.
3. Purchase oxen, clothing, ammunition, spare parts, and food.
4. Select an April departure.
5. Reach Independence with the live status, map, and wagon controls visible.
6. Continue travel and observe the date, distance, wagon position, and event
   log advance.

The same flow was replayed against Mac OS 7 under BasiliskII. Both runtimes
reached the live trail and accepted the state-changing Continue action. Travel
dates, distances, and random event text are deliberately not treated as
pixel-deterministic.

## Runtime contract

No title-specific dispatch or compatibility branch is required. The verified
flow exercises these generic Systemless contracts:

- StuffIt extraction and VFS-relative access to the application and companion
  color data;
- Resource Manager loading across the application and companion resource
  forks;
- Dialog Manager, Control Manager, TextEdit, and Event Manager input during
  party, store, and date setup;
- Color QuickDraw drawing, palette handling, regions, text, and cursor
  updates on the 512×342 game surface;
- mouse polling and tick progression during live travel.

Future fixes for this game must preserve those generic contracts. Do not add
checks for the application name, creator code, archive filename, or executable
path.

## Reproduction

With a legally obtained matching archive:

```sh
cargo run --release -- path/to/Oregon-Trail.sit
```

Complete the verified flow above and confirm that Continue advances the live
trail state. For runtime changes, run the public repository test matrix:

```sh
cargo test --lib
cargo test --lib --features test-support
cargo check --no-default-features
cargo package
```
