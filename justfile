# ARTIFACT — task runner
# Requires: just, rustup, zig, cargo-zigbuild
#   brew install just zig
#   cargo install --locked cargo-zigbuild

set shell := ["bash", "-cu"]

bin := "artifact"
dist := "target/dist"

# Pin glibc to 2.17 so Linux release binaries run on RHEL/CentOS 7-era distros.
linux_glibc := "2.17"

# Default recipe: list available recipes.
default:
    @just --list

# --- Local development ----------------------------------------------------

run *ARGS:
    cargo run {{ARGS}}

build:
    cargo build --release

check:
    cargo check --all-targets

fmt:
    cargo fmt --all

clippy:
    cargo clippy --all-targets -- -D warnings

# --- Distribution builds (cargo-zigbuild) --------------------------------

# Install all release targets and required tooling.
setup-targets:
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
    rustup target add x86_64-pc-windows-gnu

# macOS universal binary (arm64 + x86_64) via zig.
build-mac:
    cargo zigbuild --release --target universal2-apple-darwin
    @mkdir -p {{dist}}
    cp target/universal2-apple-darwin/release/{{bin}} {{dist}}/{{bin}}-macos-universal

# Linux x86_64 with pinned glibc.
build-linux-x64:
    cargo zigbuild --release --target x86_64-unknown-linux-gnu.{{linux_glibc}}
    @mkdir -p {{dist}}
    cp target/x86_64-unknown-linux-gnu/release/{{bin}} {{dist}}/{{bin}}-linux-x86_64

# Linux aarch64 with pinned glibc.
build-linux-arm64:
    cargo zigbuild --release --target aarch64-unknown-linux-gnu.{{linux_glibc}}
    @mkdir -p {{dist}}
    cp target/aarch64-unknown-linux-gnu/release/{{bin}} {{dist}}/{{bin}}-linux-aarch64

# Windows x86_64 (gnu).
build-windows:
    cargo zigbuild --release --target x86_64-pc-windows-gnu
    @mkdir -p {{dist}}
    cp target/x86_64-pc-windows-gnu/release/{{bin}}.exe {{dist}}/{{bin}}-windows-x86_64.exe

# Build every distribution target.
build-all: build-mac build-linux-x64 build-linux-arm64 build-windows

# Package dist binaries into release archives.
package: build-all
    cd {{dist}} && tar -czf {{bin}}-macos-universal.tar.gz {{bin}}-macos-universal
    cd {{dist}} && tar -czf {{bin}}-linux-x86_64.tar.gz {{bin}}-linux-x86_64
    cd {{dist}} && tar -czf {{bin}}-linux-aarch64.tar.gz {{bin}}-linux-aarch64
    cd {{dist}} && zip -q {{bin}}-windows-x86_64.zip {{bin}}-windows-x86_64.exe

# Wipe build outputs.
clean:
    cargo clean
    rm -rf {{dist}}
