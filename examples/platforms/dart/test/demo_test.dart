import 'dart:async';

import 'package:demo/demo.dart';
import 'package:test/test.dart';

void main() {
  group('async export cancellation', () {
    test('completes normally when not cancelled', () async {
      final counter = CancellableCounter();
      final result = await counter.countTo(3, 5).value;
      expect(result, 3);
      expect(counter.progress(), 3);
    });

    test('cancel() stops the Rust future from making further progress', () async {
      final counter = CancellableCounter();
      final operation = counter.countTo(1000, 20);

      // Let a couple of ticks land, then cancel mid-flight.
      await Future<void>.delayed(const Duration(milliseconds: 60));
      final progressAtCancel = counter.progress();
      expect(
        progressAtCancel,
        lessThan(1000),
        reason: 'sanity check: the count should still be far from target',
      );
      await operation.cancel();

      // Give any in-flight tick a chance to land, and give a
      // *would-be-uncancelled* run enough time to have reached the target.
      await Future<void>.delayed(const Duration(milliseconds: 200));

      // Progress may have ticked once more (a delay already in flight when
      // cancel() was called can still land), but must not have gotten
      // anywhere near completing, and must be stable afterwards.
      final progressAfterCancel = counter.progress();
      expect(progressAfterCancel, lessThan(progressAtCancel + 3));

      await Future<void>.delayed(const Duration(milliseconds: 100));
      expect(
        counter.progress(),
        progressAfterCancel,
        reason: 'progress must not still be climbing once cancelled',
      );
    });

    test('cancel() prevents the operation from ever completing', () async {
      final counter = CancellableCounter();
      final operation = counter.countTo(1000, 20);

      await Future<void>.delayed(const Duration(milliseconds: 30));
      await operation.cancel();

      expect(operation.isCanceled, isTrue);
      await expectLater(
        operation.value.timeout(const Duration(milliseconds: 300)),
        throwsA(isA<TimeoutException>()),
      );
    });

    test('cancel() after natural completion is a harmless no-op', () async {
      final counter = CancellableCounter();
      final operation = counter.countTo(2, 5);
      final result = await operation.value;
      expect(result, 2);

      // Must not throw, hang, or double-free the already-completed future.
      await operation.cancel();
      expect(operation.isCanceled, isFalse);
    });
  });
}
