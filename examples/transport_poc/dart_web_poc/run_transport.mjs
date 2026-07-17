import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { compile } from "./transport_impl.mjs";
import {
  runTransportRoundtrip,
} from "../dist/wasm/generated/transport_poc_node.ts";

// Instantiate the Dart-compiled wasm module and grab the Transport factory
// it published on globalThis.
const dartBytes = await readFile(
  new URL("./transport_impl.wasm", import.meta.url)
);
const dartApp = await compile(dartBytes);
const dartInstance = await dartApp.instantiate({});
dartInstance.invokeMain();

const transport = globalThis.dartTransportFactory();

const payload = Uint8Array.from([10, 20, 30, 40, 50]);
const result = await runTransportRoundtrip(transport, payload);

assert.deepEqual(Array.from(result), Array.from(payload));
console.log(
  "PASS: round trip through Rust-wasm <-> Dart-wasm (js_interop) Transport"
);
