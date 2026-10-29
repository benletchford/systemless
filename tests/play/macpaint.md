The MacPaint replays use Gryphel's unmodified MacPaint 1.3 gry2 disk image
(SHA-256 `82d534062c5db02f873de7e8003a94159097fe30e2c76dff699cf1130a54ae2b`),
available from https://www.gryphel.com/c/sw/general/macpaint/. The image is
available for non-commercial use; it is not included in this repository.

Build `systemless` with the `ci-test` profile, then run each script against a
fresh disk image, for example:

```sh
SYSTEMLESS_SCREEN_WIDTH=512 SYSTEMLESS_SCREEN_HEIGHT=342 SYSTEMLESS_RAM_SIZE=4194304 \
  target/ci-test/systemless macpaint-1.3-gry2.dsk --headless \
  --addressing-24-bit --screen-depth 1 --play-script tests/play/macpaint-paste.json \
  --play-output /tmp/macpaint-paste
```

The bucket, lasso, copy, paste and FatBits scripts check drawing pixels;
reopened draws, saves, closes and reopens a picture and checks its pixels.
Alert-button checks the
highlight while held and after dragging outside; save-dialog checks that the
menu title is no longer highlighted. Inspect `report.json` for assertion
results and exhausted frames.

On 29 October 2026, a local Chromium 156 preview using the pinned ZIP and a
temporary launch-enabled copy of the entry ran at 59.99 and 60.30 guest ticks
per second in two 20-second samples (minimum 50), with a 512 × 342 canvas and
no console errors. A separate brush click changed document pixels and ran at
59.99 ticks per second. The ZIP was supplied by the local test harness; this
does not verify asset promotion or a deployed route. The tracked entry remains
launch-disabled pending the normal approval and promotion workflow.
