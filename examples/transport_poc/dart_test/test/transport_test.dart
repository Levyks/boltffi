import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:transport_poc/transport_poc.dart';

final class FakeTransport implements Transport {
  TransportConfig? _config;
  final List<int> _buffer = [];

  @override
  Future<BoltFFIResult<void, TransportError>> configure(
    TransportConfig config,
  ) async {
    _config = config;
    return BoltFFIResult.ok(null);
  }

  @override
  Future<BoltFFIResult<void, TransportError>> writeAll(
    Uint8List data,
  ) async {
    if (_config == null) {
      return BoltFFIResult.err(TransportError.notConfigured());
    }
    _buffer.addAll(data);
    return BoltFFIResult.ok(null);
  }

  @override
  Future<BoltFFIResult<Uint8List, TransportError>> read(
    int maximumBytes,
    Duration timeout,
  ) async {
    if (_config == null) {
      return BoltFFIResult.err(TransportError.notConfigured());
    }
    final take = maximumBytes < _buffer.length ? maximumBytes : _buffer.length;
    final chunk = Uint8List.fromList(_buffer.take(take).toList());
    _buffer.removeRange(0, take);
    return BoltFFIResult.ok(chunk);
  }
}

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
    final transport = FakeTransport();
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
