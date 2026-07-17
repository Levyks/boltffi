import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { compile } from "./impl.mjs";
import * as bindings from "../dist/wasm/generated/transport_poc_node.ts";

// Equivalent of the generated transport_poc_web_loader.mjs, using the raw
// .ts source directly since this environment has no tsc to compile it.
globalThis.__boltffi_transport_poc = bindings;

const dartBytes = await readFile(new URL("./impl.wasm", import.meta.url));
const dartApp = await compile(dartBytes);
const dartInstance = await dartApp.instantiate({});
dartInstance.invokeMain();

const payload = Uint8Array.from([11, 22, 33, 44]);
const result = await globalThis.dartRunRoundtrip(payload);

assert.deepEqual(Array.from(result), Array.from(payload));
console.log(
  "PASS: round trip through GENERATED target::dart_web bindings (Rust-wasm <-> Dart-wasm)"
);

const errorMessage = await globalThis.dartRunFailingRoundtrip(payload);
assert.ok(
  errorMessage.includes("Timeout") || errorMessage.includes("timeout"),
  `expected a Timeout-related error, got: ${errorMessage}`
);
console.log("PASS: TransportError.timeout() from Dart propagates back through generated bindings:", errorMessage);
