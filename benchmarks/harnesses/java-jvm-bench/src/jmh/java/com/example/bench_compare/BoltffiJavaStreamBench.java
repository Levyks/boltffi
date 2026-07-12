package com.example.bench_compare;

import com.example.bench_boltffi.BoltffiJavaStreamState;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Level;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;
import org.openjdk.jmh.infra.Blackhole;

@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.NANOSECONDS)
@State(Scope.Thread)
public class BoltffiJavaStreamBench {
    private BoltffiJavaStreamState stream;

    @Setup(Level.Trial)
    public void setup() {
        stream = new BoltffiJavaStreamState();
        stream.roundTrip();
    }

    @TearDown(Level.Trial)
    public void tearDown() {
        stream.close();
    }

    @Benchmark
    public void boltffi_java_stream_batch_round_trip(Blackhole blackhole) {
        blackhole.consume(stream.roundTrip());
    }
}
