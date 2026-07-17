// Stand-in for what the real target::dart_web codegen would emit: the exact
// same plain-Dart type shapes as the native dart:ffi target's generated
// transport_poc.dart (BoltFFIResult, TransportConfig, TransportError,
// Transport) -- no dart:js_interop, no dart:ffi, nothing platform-specific.
// Only the *adapter* (transport_js_adapter.dart) touches js_interop.
import 'dart:typed_data';

sealed class BoltFFIResult<Ok, Err extends Object> {
  const BoltFFIResult();

  factory BoltFFIResult.ok(Ok value) = BoltFFIResult$Ok;

  factory BoltFFIResult.err(Err value) = BoltFFIResult$Err;
}

final class BoltFFIResult$Ok<Ok, Err extends Object>
    extends BoltFFIResult<Ok, Err> {
  final Ok value;

  const BoltFFIResult$Ok(this.value);
}

final class BoltFFIResult$Err<Ok, Err extends Object>
    extends BoltFFIResult<Ok, Err> {
  final Err value;

  const BoltFFIResult$Err(this.value);
}

final class TransportConfig {
  final int baudRate;
  final String label;

  const TransportConfig({required this.baudRate, required this.label});
}

sealed class TransportError implements Exception {
  const TransportError();
  const factory TransportError.notConfigured() = TransportError$NotConfigured;
  const factory TransportError.timeout() = TransportError$Timeout;
  const factory TransportError.io(String field0) = TransportError$Io;
}

final class TransportError$NotConfigured extends TransportError {
  const TransportError$NotConfigured();
}

final class TransportError$Timeout extends TransportError {
  const TransportError$Timeout();
}

final class TransportError$Io extends TransportError {
  final String field0;
  const TransportError$Io(this.field0);
}

abstract interface class Transport {
  Future<BoltFFIResult<void, TransportError>> configure(
    TransportConfig config,
  );

  Future<BoltFFIResult<Uint8List, TransportError>> read(
    int maximumBytes,
    Duration timeout,
  );

  Future<BoltFFIResult<void, TransportError>> writeAll(Uint8List data);
}
