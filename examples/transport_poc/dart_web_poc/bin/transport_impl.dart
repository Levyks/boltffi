// Web entry point. Note there is no business logic here at all: it just
// wraps the SAME `PlainTransport` used by the native test
// (../../dart_test/test/transport_test.dart, via transport_shared) in the
// generated-style JS adapter and publishes it for the host JS to grab.
import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:transport_shared/plain_transport.dart';
import 'package:transport_shared/transport_js_adapter.dart';

@JS()
external JSObject get globalThis;

void main() {
  final adapter = TransportJSAdapter(PlainTransport());
  final wrapped = createJSInteropWrapper(adapter);
  globalThis.setProperty('dartTransportFactory'.toJS, (() => wrapped).toJS);
}
