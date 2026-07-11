#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
SUITE="${1:-primitive}"
case "$SUITE" in
    primitive)
        BENCHMARK_CLASS="BoltffiJavaPrimitiveBench"
        ;;
    record)
        BENCHMARK_CLASS="BoltffiJavaRecordBench"
        ;;
    enum)
        BENCHMARK_CLASS="BoltffiJavaEnumBench"
        ;;
    class)
        BENCHMARK_CLASS="BoltffiJavaClassBench"
        ;;
    *)
        printf 'Usage: %s [primitive|record|enum|class]\n' "$0" >&2
        exit 2
        ;;
esac
RESULTS_DIR="$SCRIPT_DIR/build/results/jmh-$SUITE"
RESULTS_ROOT="$SCRIPT_DIR/build/results"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/boltffi-java-ir-bench-XXXXXX")"
ANALYZER_ROOT="$ROOT_DIR/benchmarks/scripts"
GENERATED_ROOT="$ROOT_DIR/benchmarks/generated/boltffi"
INCLUDE="com\\.example\\.bench_compare\\.${BENCHMARK_CLASS}\\.boltffi_java_.*$"

cleanup() {
    local exit_code=$?
    trap - EXIT
    if [[ $exit_code -eq 0 ]]; then
        chmod -R u+w "$WORK_DIR"
        rm -rf "$WORK_DIR"
    else
        printf 'Comparison failed; preserved artifacts at %s\n' "$WORK_DIR" >&2
    fi
    exit "$exit_code"
}

trap cleanup EXIT

prepare_generator() {
    local generator="$1"
    local prepared_root="$WORK_DIR/prepared/$generator"
    local generated="$prepared_root/generated"
    local generation_artifacts="$prepared_root/generation-artifacts"
    local benchmark_jar="$prepared_root/benchmark.jar"
    local java_class="$prepared_root/BenchBoltFFI.class"
    local java_launcher="$prepared_root/java-launcher.txt"
    local gradle_build="$WORK_DIR/gradle/$generator"

    mkdir -p "$prepared_root"
    "$GENERATED_ROOT/build-java.sh" --generator "$generator"
    cp -R "$GENERATED_ROOT/dist/java" "$generated"
    cp -R "$GENERATED_ROOT/dist/.boltffi-java-artifacts" "$generation_artifacts"
    PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$ANALYZER_ROOT" python3 -m java_ir verify-sources \
        --generator "$generator" \
        --generated "$generated" \
        --generation-artifacts "$generation_artifacts" \
        --generation-provenance "$generated/.boltffi-java-provenance.json"
    chmod -R a-w "$generated" "$generation_artifacts"
    "$SCRIPT_DIR/gradlew" \
        -p "$SCRIPT_DIR" \
        jmhJar \
        writeBenchmarkJavaLauncher \
        --rerun-tasks \
        "-PboltffiJavaGenerator=$generator" \
        "-PboltffiJavaComparisonSuite=$SUITE" \
        "-PboltffiJavaPreparedDir=$generated" \
        "-PboltffiJavaComparisonBuildDir=$gradle_build"
    cp "$gradle_build/libs/java-jvm-bench-1.0-SNAPSHOT-jmh.jar" "$benchmark_jar"
    cp "$gradle_build/classes/java/main/com/example/bench_boltffi/BenchBoltFFI.class" "$java_class"
    cp "$gradle_build/java-launcher.txt" "$java_launcher"
    PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$ANALYZER_ROOT" python3 -m java_ir prepare \
        --generator "$generator" \
        --suite "$SUITE" \
        --root "$prepared_root" \
        --generated "$generated" \
        --generation-artifacts "$generation_artifacts" \
        --generation-provenance "$generated/.boltffi-java-provenance.json" \
        --java-class "$java_class" \
        --jar "$benchmark_jar" \
        --java-launcher "$java_launcher" \
        --output "$prepared_root/prepared-provenance.json"
    chmod -R a-w "$prepared_root"
}

run_measurement() {
    local generator="$1"
    local run_name="$2"
    local prepared_root="$RESULTS_DIR/prepared/$generator"
    local java_launcher
    local fork_arguments
    local result="$WORK_DIR/runs/$run_name.json"
    local log="$WORK_DIR/runs/$run_name.log"
    java_launcher="$(<"$prepared_root/java-launcher.txt")"
    fork_arguments="-Djava.library.path=\"$prepared_root/generated\" --enable-native-access=ALL-UNNAMED -Xlog:library=info"

    printf '\n=== %s with prepared %s artifacts ===\n' "$run_name" "$generator"
    JAVA_TOOL_OPTIONS='' \
    JDK_JAVA_OPTIONS='' \
    _JAVA_OPTIONS='' \
    "$java_launcher" \
        -jar "$prepared_root/benchmark.jar" \
        "$INCLUDE" \
        -f 1 \
        -wi 3 \
        -w 1s \
        -i 3 \
        -r 1s \
        -t 1 \
        -bm avgt \
        -tu ns \
        -foe true \
        -rf json \
        -rff "$result" \
        -jvm "$java_launcher" \
        -jvmArgsAppend "$fork_arguments" \
        2>&1 | tee "$log"
}

if [[ -d "$RESULTS_ROOT" ]]; then
    chmod -R u+w "$RESULTS_ROOT"
fi
rm -rf "$RESULTS_DIR"
mkdir -p "$WORK_DIR/prepared" "$WORK_DIR/runs"
prepare_generator legacy
prepare_generator ir

mkdir -p "$RESULTS_DIR"
mv "$WORK_DIR/prepared" "$RESULTS_DIR/prepared"

run_measurement legacy cycle-1-legacy-a
run_measurement ir cycle-1-ir-a
run_measurement ir cycle-1-ir-b
run_measurement legacy cycle-1-legacy-b
run_measurement legacy cycle-2-legacy-a
run_measurement ir cycle-2-ir-a
run_measurement ir cycle-2-ir-b
run_measurement legacy cycle-2-legacy-b
run_measurement legacy cycle-3-legacy-a
run_measurement ir cycle-3-ir-a
run_measurement ir cycle-3-ir-b
run_measurement legacy cycle-3-legacy-b

PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$ANALYZER_ROOT" python3 -m java_ir compare \
    --suite "$SUITE" \
    --prepared "$RESULTS_DIR/prepared" \
    --runs "$WORK_DIR/runs" \
    --results "$RESULTS_DIR"
cp -R "$WORK_DIR/runs" "$RESULTS_DIR/runs"

PYTHONDONTWRITEBYTECODE=1 python3 "$ROOT_DIR/benchmarks/scripts/jmh_to_benchmark_run.py" \
    --suite java-jvm \
    --results "$RESULTS_DIR/results.json" \
    --output "$RESULTS_DIR/benchmark_run.json" \
    --artifact "$RESULTS_DIR/comparison-provenance.json" \
    --artifact "$RESULTS_DIR/prepared/legacy/prepared-provenance.json" \
    --artifact "$RESULTS_DIR/prepared/ir/prepared-provenance.json"
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$ANALYZER_ROOT" python3 -m java_ir verdict \
    --provenance "$RESULTS_DIR/comparison-provenance.json"

echo
echo "Results: $RESULTS_DIR/results.json"
echo "Normalized run: $RESULTS_DIR/benchmark_run.json"
