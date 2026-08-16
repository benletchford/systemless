These PEF inventory snapshots are test fixtures for `ppc-inspect`; the source
application archives are intentionally kept outside the repository.

From the monorepo root, regenerate after intentional `ppc-inspect` JSON schema
changes:

```sh
: "${PPC_PEF_APP:?set PPC_PEF_APP to a representative PEF application}"
: "${PPC_PEF_LOW_MEMORY_APP:?set PPC_PEF_LOW_MEMORY_APP to a low-memory PEF application}"
cargo run --quiet --manifest-path systemless/Cargo.toml -p ppc --bin ppc-inspect \
  -- --no-path "$PPC_PEF_APP" \
  > systemless/crates/ppc/tests/fixtures/pef/default.json
cargo run --quiet --manifest-path systemless/Cargo.toml -p ppc --bin ppc-inspect \
  -- --no-path "$PPC_PEF_LOW_MEMORY_APP" \
  > systemless/crates/ppc/tests/fixtures/pef/low-memory.json
```
