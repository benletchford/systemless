# Native pasteboard service access

Production source: `7ec422c2`, checked on 2026-10-11.

SHA-256 provenance:

- `src/bin/gpui_demo_clipboard.rs`: `eae2ea7f62301935413e7dacc8ea793544d20e78fbd11f95489e9d4c35d03557`
- Test executable: `2f2cd69a1cb7e21007e7bfcfae796a34630bd6f04b8f582d46c69c69d43914a0`

The full frontend regression run under restricted host access failed in
`named_native_pasteboard_preserves_hidden_formats_on_rejected_export` because
`+[NSPasteboard pasteboardWithUniqueName]` returned NULL. A targeted rerun of
the unchanged test with native macOS service access completed successfully:
1 passed, 0 failed, 182 filtered out, in 1.11 seconds.

The test uses a unique private pasteboard rather than the general clipboard.
It verifies text-only export eligibility and preservation of RTF, HTML, PNG,
file URL, Zed metadata and unknown native payloads when export is rejected.
The terminal rerun log is `native-service-test.log`.

This establishes native pasteboard inventory behavior with service access.
It does not qualify physical application suspend/resume, user clipboard
interaction, GPUI callbacks, or the full frontend regression suite. The full
suite was still running when this targeted result was recorded. Do not count
its restricted-service failure as a passing result or skip the native test.
