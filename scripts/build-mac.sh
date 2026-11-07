#!/bin/sh
# Bangun CutKit untuk macOS (universal: Apple silicon + Intel) -> .app dan .dmg.
# Butuh: Xcode Command Line Tools, Rust, Node.js.
set -e
cd "$(dirname "$0")/.."
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release -p cutkit-cli --target aarch64-apple-darwin
cargo build --release -p cutkit-cli --target x86_64-apple-darwin
mkdir -p target/universal
lipo -create -output target/universal/cutkit-cli \
  target/aarch64-apple-darwin/release/cutkit-cli \
  target/x86_64-apple-darwin/release/cutkit-cli
npx tauri build --target universal-apple-darwin --bundles app,dmg
ls target/universal-apple-darwin/release/bundle/dmg/
