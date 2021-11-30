#!/bin/bash
set -e
# This requires `ockam_ffi/include/vault.h` to be
# renamed to `ockam_ffi/include/ockam/vault.h`,
# to match what erlang is including. IMO that
# seems reasonable anyway.
#
# Also, note that clang (and apple) call 64-bit
# arm "arm64" and rust (and ARM) call it "aarch64".
# (It's a bit like x86_64 vs amd64)
#
# Finally, note that I don't remember how bash works,
# and you could definitely clean this up.
#
# Possible issues:
# - always building as release.
# - use of `cc` over e.g. `xcrun cc`
# - not passing `xcrun --show-sdk-path --sdk macosx`
# - no explicit MACOSX_DEPLOYMENT_TARGET / min macos version
# - not allowing custom "$CFLAGS", etc.
# - produces a .dylib when the elixir wants a .so. it's better for us
#   I think if we passed `-o libblah.so` then
# - no support for "$CARGO_TARGET_DIR"
# - doesn't detect that the user needs to
#   `rustup target add x86_64-apple-darwin aarch64-apple-darwin`
# - assumes that `ockam_vault_software/_build/dev/native/tmp` is
#   an okay place to build
# - always builds everything every time...

ERLANG_INCLUDE_DIR=$(erl -eval 'io:format("~s", [lists:concat([code:root_dir(), "/erts-", erlang:system_info(version), "/include"])])' -s init stop -noshell)

OCKAM_ROOT=$(git rev-parse --show-toplevel)

OCKAM_VAULT_SOFTWARE_DIR="$OCKAM_ROOT/implementations/elixir/ockam/ockam_vault_software"
# NOT SURE THAT THIS
BUILD_DIR="$OCKAM_VAULT_SOFTWARE_DIR/_build/dev/native/tmp"

OCKAM_FFI_DIR="$OCKAM_ROOT/implementations/rust/ockam/ockam_ffi"
NIF_SOURCE_DIR="$OCKAM_VAULT_SOFTWARE_DIR/native/vault/software"

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/macos-x86_64" "$BUILD_DIR/macos-arm64" "$BUILD_DIR/macos-universal"

# cargo build for x86_64
pushd "$OCKAM_FFI_DIR"
cargo build --release --target=x86_64-apple-darwin
popd

# build for x86_64, needs: `-arch x86_64 -m64` flags
# the x86_64-apple-darwin rust lib as an input
# and to be placed in the right output
cc \
    -I "$NIF_SOURCE_DIR" -I "$OCKAM_FFI_DIR/include" -I "$ERLANG_INCLUDE_DIR" \
    -arch x86_64 -m64 "$OCKAM_ROOT/target/x86_64-apple-darwin/release/libockam_ffi.a" \
    "$NIF_SOURCE_DIR/common.c" "$NIF_SOURCE_DIR/nifs.c" "$NIF_SOURCE_DIR/vault.c" \
    -O3 -fPIC -shared -Wl,-undefined,dynamic_lookup \
    -o "$BUILD_DIR/macos-x86_64/libockam_elixir_ffi.dylib"

echo "### Building rust code for for aarch64"

# build for aarch64, needs: `-arch arm64` (note: not aarch64!)
# the aarch64-apple-darwin rust lib as an input
# and to be placed in the right output
pushd "$OCKAM_FFI_DIR"
cargo build --release --target=aarch64-apple-darwin
popd

cc \
    -I "$NIF_SOURCE_DIR" \
    -I "$OCKAM_FFI_DIR/include" \
    -I "$ERLANG_INCLUDE_DIR" \
    -arch arm64 "$OCKAM_ROOT/target/aarch64-apple-darwin/release/libockam_ffi.a" \
    "$NIF_SOURCE_DIR/common.c" "$NIF_SOURCE_DIR/nifs.c" "$NIF_SOURCE_DIR/vault.c" \
    -O3 -fPIC -shared -Wl,-undefined,dynamic_lookup \
    -o "$BUILD_DIR/macos-arm64/libockam_elixir_ffi.dylib"

echo "### Producing universal binary"
# Create a universal binary
lipo -create \
    -output "$BUILD_DIR/macos-universal/libockam_elixir_ffi.dylib" \
    "$BUILD_DIR/macos-arm64/libockam_elixir_ffi.dylib" \
    "$BUILD_DIR/macos-x86_64/libockam_elixir_ffi.dylib"

cp "$BUILD_DIR/macos-universal/libockam_elixir_ffi.dylib" "$OCKAM_VAULT_SOFTWARE_DIR/priv/darwin_universal/native/libockam_elixir_ffi.so"

