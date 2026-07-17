import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

@JS()
external JSObject get globalThis;

/// `matchWireResult` (in `@boltffi/runtime`) only lets bare primitives
/// (numbers, strings, `undefined`) pass through as an implicit "ok" value.
/// Any object-typed success value -- a `Uint8Array`, a record, etc. -- is
/// otherwise ambiguous with a mis-shaped `WireResult`/`WireErr` and gets
/// rejected, so those must be wrapped explicitly as `{tag: 'ok', value}`.
JSObject _wireOk(JSAny? value) {
  final result = JSObject();
  result.setProperty('tag'.toJS, 'ok'.toJS);
  result.setProperty('value'.toJS, value);
  return result;
}

@JSExport()
final class DartTransport {
  bool _configured = false;
  final List<int> _buffer = [];

  JSPromise<JSAny?> configure(JSAny? config) => _configure().toJS;

  Future<JSAny?> _configure() async {
    _configured = true;
    return _wireOk(null);
  }

  JSPromise<JSAny?> writeAll(JSUint8Array data) => _writeAll(data).toJS;

  Future<JSAny?> _writeAll(JSUint8Array data) async {
    if (!_configured) {
      throw StateError('transport not configured');
    }
    _buffer.addAll(data.toDart);
    return _wireOk(null);
  }

  JSPromise<JSAny?> read(JSNumber maximumBytes, JSAny? timeout) =>
      _read(maximumBytes.toDartInt).toJS;

  Future<JSAny?> _read(int maximumBytes) async {
    if (!_configured) {
      throw StateError('transport not configured');
    }
    final take = maximumBytes < _buffer.length ? maximumBytes : _buffer.length;
    final chunk = Uint8List.fromList(_buffer.take(take).toList());
    _buffer.removeRange(0, take);
    return _wireOk(chunk.toJS);
  }
}

void main() {
  final transport = DartTransport();
  final wrapped = createJSInteropWrapper(transport);
  globalThis.setProperty('dartTransportFactory'.toJS, (() => wrapped).toJS);
}
