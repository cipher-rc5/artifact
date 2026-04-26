# ARTIFACT

GPUI-based desktop app for finding and reclaiming disk space. Walks
directories, ranks them by size, and persists results in an embedded redb
database so repeated scans are fast.

## Stack

- **UI** — [gpui](https://crates.io/crates/gpui) (Zed's GPU-accelerated framework)
- **Storage** — [redb](https://crates.io/crates/redb) + [rkyv](https://crates.io/crates/rkyv) zero-copy records
- **Scanning** — [walkdir](https://crates.io/crates/walkdir) on a `num_cpus`-sized thread pool
- **Logging** — [tracing](https://crates.io/crates/tracing) with rolling file output
- **Config** — TOML, loaded from the platform user-config dir via [dirs](https://crates.io/crates/dirs)

See [`Cargo.toml`](./Cargo.toml) for the annotated dependency list.

## Requirements

- Rust 1.85+ (edition 2024)
- macOS, Linux, or Windows

For distribution / cross-compilation:

- [zig](https://ziglang.org/) — `brew install zig`
- [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) — `cargo install --locked cargo-zigbuild`
- [just](https://github.com/casey/just) — `brew install just`

## Development

```bash
just run            # cargo run
just build          # cargo build --release
just check          # cargo check --all-targets
just fmt            # cargo fmt --all
just clippy         # cargo clippy --all-targets -- -D warnings
```

`just` with no args lists every recipe.

## Distribution builds

Release artifacts are produced via [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild),
which uses Zig's bundled C toolchain as the linker. This gives us:

- macOS **universal2** binaries (arm64 + x86_64) from a single recipe
- Linux binaries with a **pinned glibc** (default: 2.17) so they run on
  older distros (RHEL/CentOS 7-era and newer)
- Windows builds without needing a Windows host

One-time setup:

```bash
just setup-targets   # rustup target add for every dist target
```

Per-target builds (artifacts land in `target/dist/`):

```bash
just build-mac           # universal2-apple-darwin
just build-linux-x64     # x86_64-unknown-linux-gnu, glibc 2.17
just build-linux-arm64   # aarch64-unknown-linux-gnu, glibc 2.17
just build-windows       # x86_64-pc-windows-gnu
just build-all           # all of the above
just package             # tar.gz / zip the dist binaries
```

To bump the glibc floor, edit `linux_glibc` at the top of the `justfile`.

The `.cargo/config.toml` wires `cargo-zigbuild` in as the linker for the
cross targets, so plain `cargo build --target=…` produces the same output
as the `just` recipes.

## Project layout

```
src/
  main.rs            # entry point, wires config + logging + GPUI
  app.rs             # ArtifactApp model
  view.rs            # top-level view
  components.rs      # UI components
  scanner.rs         # filesystem traversal
  database.rs        # redb persistence
  config.rs          # TOML config loader
  logging.rs         # tracing-subscriber init
  theme.rs           # colors / typography
  directory_item.rs  # scanned-directory record
  error.rs           # crate-wide error types
  utils.rs
  lib.rs

scripts/build.sh     # thin wrapper around `just`
justfile             # task runner — see recipes above
.cargo/config.toml   # zig linker config for cross targets
```

## License

MIT
