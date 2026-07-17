// Stand-in for what the real target::dart_web codegen would generate: a
// thin adapter wrapping ANY plain-Dart `Transport` implementation (the
// user's class, unmodified) and exposing it to JS via
// createJSInteropWrapper. All js_interop/wasm-marshalling detail lives
// here -- the user never sees dart:js_interop.
import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'transport_types.dart';

/// `matchWireResult` (in `@boltffi/runtime`) only lets bare primitives pass
/// through as an implicit "ok" value; any object-typed success (a
/// `Uint8Array`, a record, etc.) must be wrapped explicitly, or it's
/// rejected as an ambiguous `WireResult`/`WireErr` shape.
JSObject _wireOk(JSAny? value) {
  final result = JSObject();
  result.setProperty('tag'.toJS, 'ok'.toJS);
  result.setProperty('value'.toJS, value);
  return result;
}

@JSExport()
final class TransportJSAdapter {
  final Transport _impl;

  TransportJSAdapter(this._impl);

  JSPromise<JSAny?> configure(JSAny? config) => _configure(config).toJS;

  Future<JSAny?> _configure(JSAny? config) async {
    final jsConfig = config as JSObject;
    final decoded = TransportConfig(
      baudRate: jsConfig.getProperty<JSNumber>('baudRate'.toJS).toDartInt,
      label: jsConfig.getProperty<JSString>('label'.toJS).toDart,
    );
    final result = await _impl.configure(decoded);
    return switch (result) {
      BoltFFIResult$Ok() => _wireOk(null),
      BoltFFIResult$Err(:final value) => throw StateError(value.toString()),
    };
  }

  JSPromise<JSAny?> writeAll(JSUint8Array data) => _writeAll(data).toJS;

  Future<JSAny?> _writeAll(JSUint8Array data) async {
    final result = await _impl.writeAll(data.toDart);
    return switch (result) {
      BoltFFIResult$Ok() => _wireOk(null),
      BoltFFIResult$Err(:final value) => throw StateError(value.toString()),
    };
  }

  JSPromise<JSAny?> read(JSNumber maximumBytes, JSAny? timeout) =>
      _read(maximumBytes.toDartInt).toJS;

  Future<JSAny?> _read(int maximumBytes) async {
    // Standing in for the generated Duration decode; the POC's Rust side
    // doesn't branch on the timeout value, so it's not threaded through
    // here.
    final result = await _impl.read(maximumBytes, Duration.zero);
    return switch (result) {
      BoltFFIResult$Ok(:final value) => _wireOk(value.toJS),
      BoltFFIResult$Err(:final value) => throw StateError(value.toString()),
    };
  }
}
