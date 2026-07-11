from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class BenchmarkCase:
    name: str
    legacy_symbol: str
    ir_symbol: str

    def symbol(self, generator: str) -> str:
        return self.legacy_symbol if generator == "legacy" else self.ir_symbol

    def opposite_symbol(self, generator: str) -> str:
        return self.ir_symbol if generator == "legacy" else self.legacy_symbol


@dataclass(frozen=True)
class ComparisonRun:
    cycle: int
    position: str
    generator: str

    @property
    def name(self) -> str:
        return f"cycle-{self.cycle}-{self.generator}-{self.position}"


@dataclass(frozen=True)
class ComparisonPair:
    cycle: int
    position: str
    legacy_run: str
    ir_run: str


@dataclass(frozen=True)
class BenchmarkSuite:
    name: str
    benchmark_class: str
    cases: tuple[BenchmarkCase, ...]

    @property
    def prefix(self) -> str:
        return f"com.example.bench_compare.{self.benchmark_class}.boltffi_java_"


PRIMITIVE_CASES = (
    BenchmarkCase(
        "echo_bool",
        "boltffi_echo_bool",
        "boltffi_function_demo_primitives_scalars_echo_bool",
    ),
    BenchmarkCase(
        "negate_bool",
        "boltffi_negate_bool",
        "boltffi_function_demo_primitives_scalars_negate_bool",
    ),
    BenchmarkCase(
        "echo_i8", "boltffi_echo_i8", "boltffi_function_demo_primitives_scalars_echo_i8"
    ),
    BenchmarkCase(
        "echo_u8", "boltffi_echo_u8", "boltffi_function_demo_primitives_scalars_echo_u8"
    ),
    BenchmarkCase(
        "echo_i16",
        "boltffi_echo_i16",
        "boltffi_function_demo_primitives_scalars_echo_i16",
    ),
    BenchmarkCase(
        "echo_u16",
        "boltffi_echo_u16",
        "boltffi_function_demo_primitives_scalars_echo_u16",
    ),
    BenchmarkCase(
        "echo_i32",
        "boltffi_echo_i32",
        "boltffi_function_demo_primitives_scalars_echo_i32",
    ),
    BenchmarkCase(
        "add_i32", "boltffi_add_i32", "boltffi_function_demo_primitives_scalars_add_i32"
    ),
    BenchmarkCase(
        "echo_u32",
        "boltffi_echo_u32",
        "boltffi_function_demo_primitives_scalars_echo_u32",
    ),
    BenchmarkCase(
        "echo_i64",
        "boltffi_echo_i64",
        "boltffi_function_demo_primitives_scalars_echo_i64",
    ),
    BenchmarkCase(
        "echo_u64",
        "boltffi_echo_u64",
        "boltffi_function_demo_primitives_scalars_echo_u64",
    ),
    BenchmarkCase(
        "echo_f32",
        "boltffi_echo_f32",
        "boltffi_function_demo_primitives_scalars_echo_f32",
    ),
    BenchmarkCase(
        "add_f32", "boltffi_add_f32", "boltffi_function_demo_primitives_scalars_add_f32"
    ),
    BenchmarkCase(
        "echo_f64",
        "boltffi_echo_f64",
        "boltffi_function_demo_primitives_scalars_echo_f64",
    ),
    BenchmarkCase(
        "add_f64", "boltffi_add_f64", "boltffi_function_demo_primitives_scalars_add_f64"
    ),
    BenchmarkCase(
        "echo_usize",
        "boltffi_echo_usize",
        "boltffi_function_demo_primitives_scalars_echo_usize",
    ),
    BenchmarkCase(
        "echo_isize",
        "boltffi_echo_isize",
        "boltffi_function_demo_primitives_scalars_echo_isize",
    ),
    BenchmarkCase(
        "noop", "boltffi_noop", "boltffi_function_demo_primitives_scalars_noop"
    ),
    BenchmarkCase("add", "boltffi_add", "boltffi_function_demo_primitives_scalars_add"),
    BenchmarkCase(
        "multiply",
        "boltffi_multiply",
        "boltffi_function_demo_primitives_scalars_multiply",
    ),
    BenchmarkCase(
        "inc_u64_value",
        "boltffi_inc_u64_value",
        "boltffi_function_demo_primitives_vecs_inc_u64_value",
    ),
)
RECORD_CASES = (
    BenchmarkCase(
        "echo_point",
        "boltffi_echo_point",
        "boltffi_function_demo_records_blittable_echo_point",
    ),
    BenchmarkCase(
        "point_distance",
        "boltffi_point_distance",
        "boltffi_method_record_demo_records_blittable_point_distance",
    ),
    BenchmarkCase(
        "point_scale",
        "boltffi_point_scale",
        "boltffi_method_record_demo_records_blittable_point_scale",
    ),
    BenchmarkCase(
        "echo_line",
        "boltffi_echo_line",
        "boltffi_function_demo_records_nested_echo_line",
    ),
    BenchmarkCase(
        "line_length",
        "boltffi_line_length",
        "boltffi_function_demo_records_nested_line_length",
    ),
    BenchmarkCase(
        "echo_service_config",
        "boltffi_echo_service_config",
        "boltffi_function_demo_records_default_values_echo_service_config",
    ),
    BenchmarkCase(
        "echo_tagged_scores",
        "boltffi_echo_tagged_scores",
        "boltffi_function_demo_records_with_collections_echo_tagged_scores",
    ),
    BenchmarkCase(
        "generate_user_profiles_100",
        "boltffi_generate_user_profiles",
        "boltffi_function_demo_records_with_collections_generate_user_profiles",
    ),
    BenchmarkCase(
        "sum_user_scores_100",
        "boltffi_sum_user_scores",
        "boltffi_function_demo_records_with_collections_sum_user_scores",
    ),
)
ENUM_CASES = (
    BenchmarkCase(
        "simple_enum",
        "boltffi_opposite_direction",
        "boltffi_function_demo_enums_c_style_opposite_direction",
    ),
    BenchmarkCase(
        "echo_direction",
        "boltffi_echo_direction",
        "boltffi_function_demo_enums_c_style_echo_direction",
    ),
    BenchmarkCase(
        "find_direction",
        "boltffi_find_direction",
        "boltffi_function_demo_enums_c_style_find_direction",
    ),
    BenchmarkCase(
        "data_enum_input",
        "boltffi_get_status_progress",
        "boltffi_function_demo_enums_data_enum_get_status_progress",
    ),
    BenchmarkCase(
        "echo_task_status_unit_variant",
        "boltffi_echo_task_status",
        "boltffi_function_demo_enums_data_enum_echo_task_status",
    ),
    BenchmarkCase(
        "echo_task_status_small_payload",
        "boltffi_echo_task_status",
        "boltffi_function_demo_enums_data_enum_echo_task_status",
    ),
    BenchmarkCase(
        "echo_task_status_completed_payload",
        "boltffi_echo_task_status",
        "boltffi_function_demo_enums_data_enum_echo_task_status",
    ),
    BenchmarkCase(
        "generate_directions_100",
        "boltffi_generate_directions",
        "boltffi_function_demo_enums_c_style_generate_directions",
    ),
    BenchmarkCase(
        "count_north_100",
        "boltffi_count_north",
        "boltffi_function_demo_enums_c_style_count_north",
    ),
)
SUITES = {
    suite.name: suite
    for suite in (
        BenchmarkSuite("primitive", "BoltffiJavaPrimitiveBench", PRIMITIVE_CASES),
        BenchmarkSuite("record", "BoltffiJavaRecordBench", RECORD_CASES),
        BenchmarkSuite("enum", "BoltffiJavaEnumBench", ENUM_CASES),
    )
}
ALL_CASES = tuple(case for suite in SUITES.values() for case in suite.cases)
RUNS = tuple(
    ComparisonRun(cycle, position, generator)
    for cycle in range(1, 4)
    for position, generator in (
        ("a", "legacy"),
        ("a", "ir"),
        ("b", "ir"),
        ("b", "legacy"),
    )
)
PAIRS = tuple(
    pair
    for cycle in range(1, 4)
    for pair in (
        ComparisonPair(cycle, "a", f"cycle-{cycle}-legacy-a", f"cycle-{cycle}-ir-a"),
        ComparisonPair(cycle, "b", f"cycle-{cycle}-legacy-b", f"cycle-{cycle}-ir-b"),
    )
)
BENCHMARK_PREFIX = SUITES["primitive"].prefix
EXPECTED_JVM_ARGS = (
    "-Djava.library.path=<prepared-java>",
    "--enable-native-access=ALL-UNNAMED",
    "-Xlog:library=info",
)
NON_INFERIORITY_MARGIN = 1.05
ONE_SIDED_T_95_DF_5 = 2.0150483733330233
TWO_SIDED_T_95_DF_5 = 2.570581835636314


def ensure_generator(generator: str) -> str:
    if generator not in {"legacy", "ir"}:
        raise SystemExit(f"unknown Java generator: {generator}")
    return generator


def expected_symbols(generator: str) -> frozenset[str]:
    return frozenset(case.symbol(generator) for case in ALL_CASES)


def opposite_symbols(generator: str) -> frozenset[str]:
    return frozenset(case.opposite_symbol(generator) for case in ALL_CASES)


def jni_export(symbol: str) -> str:
    return "Java_com_example_bench_1boltffi_Native_" + symbol.replace("_", "_1")


def primitive_case(benchmark_name: str) -> str:
    return benchmark_case(SUITES["primitive"], benchmark_name)


def benchmark_suite(name: str) -> BenchmarkSuite:
    try:
        return SUITES[name]
    except KeyError as error:
        raise SystemExit(f"unknown Java comparison suite: {name}") from error


def benchmark_case(suite: BenchmarkSuite, benchmark_name: str) -> str:
    if not benchmark_name.startswith(suite.prefix):
        raise SystemExit(f"unexpected {suite.name} benchmark identifier: {benchmark_name}")
    return benchmark_name.removeprefix(suite.prefix)
