# PowerPC import gap audit

The PowerPC loader binds every strong import, including symbols that have no
native implementation; such a symbol halts the engine only when the guest first
calls it. This tool measures those gaps statically, without running guest code.

## One archive

```sh
cargo build --locked --profile ci-test --bin systemless
target/ci-test/systemless /path/to/app.sit --ppc-import-audit > report.json
```

The JSON report (`systemless_ppc_import_audit_v1`) lists every PowerPC
fragment in the archive's launch VFS: applications, `'cfrg'` libraries, and
unregistered PEF data forks that plug-in loaders open with `GetDiskFragment`.
Each import is classified with the loader's own binding order:

| Status | Meaning |
|---|---|
| `bundled` | Exported by a CFM library shipped in the archive |
| `fixed_data` | Backed by runtime data storage (`errno`, `_iob`, …) |
| `native` | Implemented by the native dispatcher |
| `weak_unresolved` | Weak import with no implementation; bound to address 0 |
| `unsupported` | Strong import with no implementation; halts when first called |

Bundled DrawSprocketLib and InputSprocketLib copies are omitted because the
launcher replaces them with the native implementations. A gap is a capability
the runtime lacks; whether a title reaches it depends on its control flow and on
which fragments it loads (for example, Carbon variants or optional plug-ins).

## Whole catalogue

```sh
python3 -I .agents/tools/ppc-import-audit/audit.py --out /new/empty/dir [EXTRA_ARCHIVE...]
```

The script fetches each catalogue archive that has a PowerPC architecture by
SHA-256 into `~/.cache/systemless-ppc-import-audit`, writes one report per
title, and writes `summary.md` ranking each missing symbol by the number of
titles importing it. Pass `--no-catalogue` with explicit archives to audit
intake candidates only.
