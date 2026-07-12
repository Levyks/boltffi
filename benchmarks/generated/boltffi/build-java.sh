#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
DEMO_DIR="$ROOT_DIR/examples/demo"
BENCH_OVERLAY="$DEMO_DIR/boltffi.benchmark.toml"
GENERATOR="ir"
ARTIFACT_BUNDLE="$SCRIPT_DIR/dist/.boltffi-java-artifacts"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --generator)
            if [[ $# -lt 2 ]]; then
                echo "--generator requires legacy or ir" >&2
                exit 1
            fi
            GENERATOR="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

if [[ "$GENERATOR" != "legacy" && "$GENERATOR" != "ir" ]]; then
    echo "Java generator must be 'legacy' or 'ir', got '$GENERATOR'" >&2
    exit 1
fi

export CARGO_TARGET_DIR="$SCRIPT_DIR/target"

resolve_jdk_home() {
    if [[ -n "${JAVA_HOME:-}" && -f "${JAVA_HOME}/include/jni.h" && -f "${JAVA_HOME}/include/darwin/jni_md.h" ]]; then
        printf '%s\n' "$JAVA_HOME"
        return 0
    fi

    if [[ -n "${JAVA_HOME:-}" && -f "${JAVA_HOME}/libexec/openjdk.jdk/Contents/Home/include/jni.h" && -f "${JAVA_HOME}/libexec/openjdk.jdk/Contents/Home/include/darwin/jni_md.h" ]]; then
        printf '%s\n' "${JAVA_HOME}/libexec/openjdk.jdk/Contents/Home"
        return 0
    fi

    if [[ "$(uname)" == "Darwin" ]]; then
        local detected_java_home
        detected_java_home="$(/usr/libexec/java_home 2>/dev/null || true)"
        if [[ -n "$detected_java_home" && -f "${detected_java_home}/include/jni.h" && -f "${detected_java_home}/include/darwin/jni_md.h" ]]; then
            printf '%s\n' "$detected_java_home"
            return 0
        fi
    fi

    return 1
}

HOST_TRIPLE="$(rustc -Vv | awk '/^host:/ { print $2 }')"
HOST_JAVA_ENV_SUFFIX="$(printf '%s' "$HOST_TRIPLE" | tr '[:lower:]-' '[:upper:]_')"

if resolved_jdk_home="$(resolve_jdk_home)"; then
    export JAVA_HOME="$resolved_jdk_home"
    export "BOLTFFI_JAVA_HOME_${HOST_JAVA_ENV_SUFFIX}=$resolved_jdk_home"
fi

cd "$DEMO_DIR"

cargo build --release -p boltffi_cli --manifest-path "$ROOT_DIR/Cargo.toml"

rm -rf "$SCRIPT_DIR/dist/java"
rm -rf "$ARTIFACT_BUNDLE"

if [[ "$GENERATOR" == "legacy" ]]; then
    "$SCRIPT_DIR/target/release/boltffi" \
        --overlay "$BENCH_OVERLAY" \
        pack java \
        --release \
        --regenerate \
        --legacy
else
    "$SCRIPT_DIR/target/release/boltffi" \
        --overlay "$BENCH_OVERLAY" \
        pack java \
        --release \
        --regenerate
fi

OUTPUT_DIR="$SCRIPT_DIR/dist/java"
MODULE_SOURCE="$OUTPUT_DIR/com/example/bench_boltffi/BenchBoltFFI.java"
RUNTIME_SOURCE="$OUTPUT_DIR/com/example/bench_boltffi/BoltFFINativeRuntime.java"
JNI_SOURCE="$OUTPUT_DIR/jni/jni_glue.c"
JNI_HEADER="$OUTPUT_DIR/jni/demo.h"
HOST_TRIPLE="$(rustc -Vv | awk '/^host:/ { print $2 }')"
STATIC_LIBRARY_FILENAME="$(printf '' | rustc --crate-name demo --crate-type staticlib --print file-names -)"
JNI_LIBRARY_FILENAME="$(printf '' | rustc --crate-name demo_jni --crate-type cdylib --print file-names -)"
DEMO_STATIC_LIBRARY="$SCRIPT_DIR/target/$HOST_TRIPLE/release/$STATIC_LIBRARY_FILENAME"
JNI_LIBRARY="$OUTPUT_DIR/$JNI_LIBRARY_FILENAME"

require_file() {
    local required_path="$1"
    if [[ ! -f "$required_path" ]]; then
        echo "Java $GENERATOR generation did not produce $required_path" >&2
        exit 1
    fi
}

require_file "$MODULE_SOURCE"
require_file "$JNI_SOURCE"
require_file "$JNI_HEADER"
require_file "$DEMO_STATIC_LIBRARY"
require_file "$JNI_LIBRARY"

printf '%s\n' "$GENERATOR" > "$OUTPUT_DIR/.boltffi-java-generator"
verify_generation() {
    PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$ROOT_DIR/benchmarks/scripts" python3 -m java_ir verify-generation \
        --generator "$GENERATOR" \
        --module "$MODULE_SOURCE" \
        --header "$JNI_HEADER" \
        --jni-source "$JNI_SOURCE" \
        --static-library "$DEMO_STATIC_LIBRARY" \
        --jni-library "$JNI_LIBRARY" \
        --output "$OUTPUT_DIR/.boltffi-java-provenance.json" \
        "$@"
}
if [[ -f "$RUNTIME_SOURCE" ]]; then
    verify_generation --runtime "$RUNTIME_SOURCE"
else
    verify_generation
fi
rm -rf "$ARTIFACT_BUNDLE"
mkdir -p "$ARTIFACT_BUNDLE"
cp "$MODULE_SOURCE" "$ARTIFACT_BUNDLE/"
if [[ -f "$RUNTIME_SOURCE" ]]; then
    cp "$RUNTIME_SOURCE" "$ARTIFACT_BUNDLE/"
fi
cp "$JNI_SOURCE" "$ARTIFACT_BUNDLE/"
cp "$JNI_HEADER" "$ARTIFACT_BUNDLE/"
cp "$DEMO_STATIC_LIBRARY" "$ARTIFACT_BUNDLE/"
cp "$JNI_LIBRARY" "$ARTIFACT_BUNDLE/"
