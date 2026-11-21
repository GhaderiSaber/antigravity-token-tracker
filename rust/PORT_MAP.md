# Port map: antigravity-token-tracker

0 of 3 units have a Rust port.

## Ports

None yet: `rust-anything bridge` starts one.

## Whole app

| Glue | Runs |
| --- | --- |
| `rust/app/src/units/auth_discovery.rs` | Rust, handing some cases to the original |
| `rust/app/src/units/core_storage.rs` | Rust, handing some cases to the original |
| `rust/app/src/units/geo_shield.rs` | Rust, handing some cases to the original |

## Not ported yet

In the order `analyze` suggests.

| Unit | Lines | Exports | Readiness |
| --- | ---: | ---: | ---: |
| `web_server.py` | 341 | 3 | 86 |
| `run_tracker.py` | 10 | 0 | 100 |
| `antigravity_tracker/` | 6119 | 0 | 0 |

Regenerate with `rust-anything map --write`. Commit this file: when the original changes upstream, the next map marks every port whose recording it outdated.
