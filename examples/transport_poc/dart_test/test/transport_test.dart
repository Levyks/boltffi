import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:transport_poc/transport_poc.dart';
// The exact same class file used, unmodified, by the web POC
// (dart_web_poc via the js_interop adapter) -- proof that one plain-Dart
// implementation satisfies both the native dart:ffi-generated `Transport`
// interface (imported here) and the web target's, with zero platform code.
import 'package:transport_shared/plain_transport.dart';

final class FailingTransport implements Transport {
  @override
  Future<BoltFFIResult<void, TransportError>> configure(
    TransportConfig config,
  ) async => BoltFFIResult.err(TransportError.timeout());

  @override
  Future<BoltFFIResult<void, TransportError>> writeAll(
    Uint8List data,
  ) async => BoltFFIResult.err(TransportError.timeout());

  @override
  Future<BoltFFIResult<Uint8List, TransportError>> read(
    int maximumBytes,
    Duration timeout,
  ) async => BoltFFIResult.err(TransportError.timeout());
}

void main() {
  test('Dart-implemented Transport round-trips through Rust', () async {
    final transport = PlainTransport();
    final payload = Uint8List.fromList([1, 2, 3, 4, 5]);

    final result = await runTransportRoundtrip(transport, payload).value;

    expect(result, payload);
  });

  test('errors returned from Dart propagate back through Rust', () async {
    final transport = FailingTransport();
    final payload = Uint8List.fromList([9]);

    await expectLater(
      runTransportRoundtrip(transport, payload).value,
      throwsA(isA<TransportError>()),
    );
  });
}
