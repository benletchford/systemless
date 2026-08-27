# Runtime route inventory

The Rust `runtime_route_inventory` example records reproducible source-level signals
from the legacy 68K trap and native PowerPC HLE dispatchers:

```sh
cargo run --example runtime_route_inventory
cargo run --example runtime_route_inventory -- --check audits/runtime-route-baseline.json
cargo test --example runtime_route_inventory
```

The tool discovers the production 68K first-match chain, PowerPC library/symbol
mapper and disabled guard helpers from source structure. The report identifies
trap claims in actual dispatcher order, duplicate or unreachable canonical
claims, raw A-line words routed to claimed slots, PowerPC import mapper and
target counts, and selected fallback signals. It hashes every scanned source
file so a retained report can be tied to the exact implementation it describes.

These are implementation inventory signals only. They are not an API catalog,
semantic coverage percentage, profile availability proof, or evidence that a
route is correct. Aliases, selector routines, callback forms, native exports,
components, and dependencies require independent profile-backed registry rows.
