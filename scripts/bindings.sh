#!/bin/sh
# Build todav-ffi for this machine, generate its Kotlin and C# bindings into
# todav/ffi/target/bindings and run the smoke tests against them (they need Gradle with a JDK 21,
# the .NET 8 SDK and msgfmt). The C# generator, uniffi-bindgen-cs, is built into target/tools the
# first time, at the version matching the UniFFI in todav/ffi/Cargo.toml.
#
#     scripts/bindings.sh            # generate and test
#     scripts/bindings.sh --no-test  # generate only
set -eu

CS_TAG="v0.11.1+v0.31.0"

root=$(cd "$(dirname "$0")/.." && pwd)
ffi="$root/todav/ffi"
tools="$root/target/tools"
cd "$ffi"

cargo build --locked
lib="$ffi/target/debug/libtodav_ffi.so"

cargo run --locked -q --features bindgen --bin uniffi-bindgen -- \
  generate --library "$lib" --language kotlin --out-dir target/bindings/kotlin --no-format

if ! "$tools/bin/uniffi-bindgen-cs" --version 2>/dev/null | grep -qF "${CS_TAG#v}"; then
  cargo install uniffi-bindgen-cs --locked --root "$tools" \
    --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag "$CS_TAG"
fi
"$tools/bin/uniffi-bindgen-cs" --library "$lib" --config uniffi.toml \
  --out-dir target/bindings/cs --no-format

[ "${1:-}" = --no-test ] && exit 0

# A catalog for the smoke tests to read.
msgfmt -o target/bindings/pl.mo "$root/po/pl.po"

(cd tests/kotlin && gradle -q run --console=plain)
(cd tests/csharp && dotnet run)
