import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';
import 'package:transport_poc/transport_poc.dart';

@JS()
external JSObject get globalThis;

final class PlainTransport implements Transport {
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
      return BoltFFIResult.err(const TransportError.notConfigured());
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
      return BoltFFIResult.err(const TransportError.notConfigured());
    }
    final take = maximumBytes < _buffer.length ? maximumBytes : _buffer.length;
    final chunk = Uint8List.fromList(_buffer.take(take).toList());
    _buffer.removeRange(0, take);
    return BoltFFIResult.ok(chunk);
  }
}

final class FailingTransport implements Transport {
  @override
  Future<BoltFFIResult<void, TransportError>> configure(TransportConfig config) async => BoltFFIResult.err(const TransportError.timeout());
  @override
  Future<BoltFFIResult<void, TransportError>> writeAll(Uint8List data) async => BoltFFIResult.err(const TransportError.timeout());
  @override
  Future<BoltFFIResult<Uint8List, TransportError>> read(int maximumBytes, Duration timeout) async => BoltFFIResult.err(const TransportError.timeout());
}

void main() {
  globalThis.setProperty(
    'dartRunRoundtrip'.toJS,
    ((JSAny payload) {
      final bytes = (payload as JSUint8Array).toDart;
      return runTransportRoundtrip(PlainTransport(), bytes).value.then(
        (result) => result.toJS,
      ).toJS;
    }).toJS,
  );
  globalThis.setProperty(
    'dartRunFailingRoundtrip'.toJS,
    ((JSAny payload) {
      final bytes = (payload as JSUint8Array).toDart;
      return runTransportRoundtrip(FailingTransport(), bytes).value.then(
        (result) => 'unexpected success'.toJS,
        onError: (e) => '${e.runtimeType}: $e'.toJS,
      ).toJS;
    }).toJS,
  );
}
