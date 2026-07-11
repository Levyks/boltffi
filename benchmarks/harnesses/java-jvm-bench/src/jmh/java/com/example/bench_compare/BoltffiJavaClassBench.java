package com.example.bench_compare;

import com.example.bench_boltffi.Accumulator;
import com.example.bench_boltffi.BenchBoltFFI;
import com.example.bench_boltffi.Counter;
import com.example.bench_boltffi.Inventory;
import com.example.bench_boltffi.MapView;
import com.example.bench_boltffi.Marker;
import com.example.bench_boltffi.MarkerOptions;
import com.example.bench_boltffi.MathUtils;
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
public class BoltffiJavaClassBench {
    private Counter counter;
    private Accumulator accumulator;
    private Inventory inventory;
    private MapView mapView;
    private MarkerOptions marker;

    @Setup(Level.Trial)
    public void setup() {
        counter = new Counter(7);
        accumulator = new Accumulator();
        inventory = new Inventory(2);
        mapView = new MapView();
        marker = new MarkerOptions(11, "pin");
        if (counter.get() != 7) throw new AssertionError("counter construction");
        if (MathUtils.add(2, 3) != 5) throw new AssertionError("static class method");
        if (!BenchBoltFFI.describeCounter(counter).contains("7")) {
            throw new AssertionError("class parameter");
        }
        try (Marker created = mapView.addMarker(marker)) {
            if (created.id() != 11 || !created.title().equals("pin")) {
                throw new AssertionError("class return");
            }
        }
    }

    @TearDown(Level.Trial)
    public void tearDown() {
        mapView.close();
        inventory.close();
        accumulator.close();
        counter.close();
    }

    @Benchmark
    public void boltffi_java_construct_close_counter(Blackhole blackhole) {
        try (Counter value = new Counter(5)) {
            blackhole.consume(value.get());
        }
    }

    @Benchmark
    public void boltffi_java_counter_get(Blackhole blackhole) {
        blackhole.consume(counter.get());
    }

    @Benchmark
    public void boltffi_java_counter_increment() {
        counter.increment();
    }

    @Benchmark
    public void boltffi_java_accumulator_add() {
        accumulator.add(3L);
    }

    @Benchmark
    public void boltffi_java_inventory_count(Blackhole blackhole) {
        blackhole.consume(inventory.count());
    }

    @Benchmark
    public void boltffi_java_inventory_add_remove(Blackhole blackhole) {
        if (!inventory.add("value")) throw new AssertionError("inventory capacity");
        blackhole.consume(inventory.remove(0));
    }

    @Benchmark
    public void boltffi_java_static_math_add(Blackhole blackhole) {
        blackhole.consume(MathUtils.add(12, 30));
    }

    @Benchmark
    public void boltffi_java_map_view_add_marker(Blackhole blackhole) {
        try (Marker created = mapView.addMarker(marker)) {
            blackhole.consume(created.id());
        }
    }

    @Benchmark
    public void boltffi_java_describe_counter(Blackhole blackhole) {
        blackhole.consume(BenchBoltFFI.describeCounter(counter));
    }
}
