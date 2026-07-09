#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
package_dir="$(mktemp -d "${TMPDIR:-/tmp}/boltffi-static-symbols.XXXXXX")"
derived_data="$package_dir/DerivedData"
archive="$package_dir/SymbolConsumer.xcarchive"
executable="$archive/Products/usr/local/bin/SymbolConsumer"
dsym="$archive/dSYMs/SymbolConsumer.dSYM"
dsym_binary="$dsym/Contents/Resources/DWARF/SymbolConsumer"
symbol="boltffi_function_demo_primitives_scalars_add_i32"
build_log="$package_dir/xcodebuild.log"

trap 'rm -rf "$package_dir"' EXIT

mkdir -p "$package_dir/Sources/SymbolConsumer"
ln -s "$script_dir/ffi" "$package_dir/ffi"

cat > "$package_dir/Package.swift" <<'EOF'
// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "SymbolConsumer",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "SymbolConsumer", targets: ["SymbolConsumer"])],
    dependencies: [.package(path: "ffi")],
    targets: [
        .executableTarget(
            name: "SymbolConsumer",
            dependencies: [.product(name: "DemoFFI", package: "ffi")]
        ),
    ]
)
EOF

cat > "$package_dir/Sources/SymbolConsumer/main.swift" <<'EOF'
import DemoFFI

precondition(boltffi_function_demo_primitives_scalars_add_i32(20, 22) == 42)
EOF

if ! (
    cd "$package_dir"
    xcodebuild \
        -scheme SymbolConsumer \
        -configuration Release \
        -destination generic/platform=macOS \
        -archivePath "$archive" \
        -derivedDataPath "$derived_data" \
        DEBUG_INFORMATION_FORMAT=dwarf-with-dsym \
        archive \
        -quiet \
        >"$build_log" 2>&1
); then
    cat "$build_log"
    exit 1
fi

"$executable"

executable_uuid="$(xcrun dwarfdump --uuid "$executable" | awk 'NR == 1 { print $2 }')"
dsym_uuid="$(xcrun dwarfdump --uuid "$dsym" | awk 'NR == 1 { print $2 }')"

test -n "$executable_uuid"
test "$executable_uuid" = "$dsym_uuid"

symbol_address="$(nm "$dsym_binary" | awk -v expected="_$symbol" '$3 == expected { print "0x" $1; exit }')"
test -n "$symbol_address"

symbolicated="$(atos -arch "$(uname -m)" -o "$dsym_binary" "$symbol_address")"
printf '%s\n' "$symbolicated" | grep -Eq '\.rs:[0-9]+\)'
