// The ONE implementation a user writes. Plain Dart, no dart:ffi, no
// dart:js_interop, no platform checks -- compiled unchanged into both the
// native (dart:ffi) and web (js_interop/wasm) targets.
import 'dart:typed_data';

import 'transport_types.dart';

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
