package com.example.bench_compare;

import com.example.bench_boltffi.BenchBoltFFI;
import com.example.bench_boltffi.Direction;
import com.example.bench_boltffi.TaskStatus;
import java.util.List;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Level;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.infra.Blackhole;

@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.NANOSECONDS)
@State(Scope.Thread)
public class BoltffiJavaEnumBench {
    private TaskStatus pending;
    private TaskStatus inProgress;
    private TaskStatus completed;
    private List<Direction> directions;

    @Setup(Level.Trial)
    public void verifyEnumBehavior() {
        pending = TaskStatus.Pending.INSTANCE;
        inProgress = new TaskStatus.InProgress(50);
        completed = new TaskStatus.Completed(100);
        directions = BenchBoltFFI.generateDirections(100);
        require(BenchBoltFFI.oppositeDirection(Direction.NORTH) == Direction.SOUTH, "simple_enum");
        require(BenchBoltFFI.directionToDegrees(Direction.EAST) == 90, "simple_enum");
        require(BenchBoltFFI.echoDirection(Direction.WEST) == Direction.WEST, "echo_direction");
        require(BenchBoltFFI.findDirection(0).isPresent(), "find_direction");
        require(BenchBoltFFI.getStatusProgress(inProgress) == 50, "data_enum_input");
        require(BenchBoltFFI.isStatusComplete(completed), "data_enum_input");
        require(BenchBoltFFI.echoTaskStatus(pending).equals(pending), "echo_task_status_unit_variant");
        require(BenchBoltFFI.echoTaskStatus(inProgress).equals(inProgress), "echo_task_status_small_payload");
        require(BenchBoltFFI.echoTaskStatus(completed).equals(completed), "echo_task_status_completed_payload");
        require(directions.size() == 100, "generate_directions_100");
        require(BenchBoltFFI.countNorth(directions) == 25, "count_north_100");
    }

    @Benchmark
    public void boltffi_java_simple_enum(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.oppositeDirection(Direction.NORTH));
        blackhole.consume(BenchBoltFFI.directionToDegrees(Direction.EAST));
    }

    @Benchmark
    public void boltffi_java_echo_direction(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.echoDirection(Direction.WEST));
    }

    @Benchmark
    public void boltffi_java_find_direction(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.findDirection(0));
    }

    @Benchmark
    public void boltffi_java_data_enum_input(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.getStatusProgress(inProgress));
        blackhole.consume(BenchBoltFFI.isStatusComplete(completed));
    }

    @Benchmark
    public void boltffi_java_echo_task_status_unit_variant(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.echoTaskStatus(pending));
    }

    @Benchmark
    public void boltffi_java_echo_task_status_small_payload(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.echoTaskStatus(inProgress));
    }

    @Benchmark
    public void boltffi_java_echo_task_status_completed_payload(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.echoTaskStatus(completed));
    }

    @Benchmark
    public void boltffi_java_generate_directions_100(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.generateDirections(100));
    }

    @Benchmark
    public void boltffi_java_count_north_100(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.countNorth(directions));
    }

    private static void require(boolean condition, String behavior) {
        if (!condition) {
            throw new AssertionError(behavior + " behavior mismatch");
        }
    }
}
