// Native (dart:ffi) side of the conditional export: just the real
// generated types from the boltffi dart target, re-exported so
// transport_types.dart can pick this file on native and
// transport_types_web.dart's hand-equivalent on web, while user code
// always imports the single `transport_types.dart` entrypoint.
export 'package:transport_poc/transport_poc.dart'
    show
        Transport,
        TransportConfig,
        TransportError,
        TransportError$NotConfigured,
        TransportError$Timeout,
        TransportError$Io,
        BoltFFIResult,
        BoltFFIResult$Ok,
        BoltFFIResult$Err;
