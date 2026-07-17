import assert from "node:assert/strict";
import { wireOk, wireErr } from "../../../runtime/typescript/src/wire.ts";
import {
  registerTransport,
  runTransportRoundtrip,
} from "../dist/wasm/generated/transport_poc_node.ts";

class FakeTransport {
  #config = null;
  #buffer = [];

  async configure(config) {
    this.#config = config;
    return wireOk(undefined);
  }

  async writeAll(data) {
    if (this.#config === null) {
      return wireErr({ tag: "NotConfigured" });
    }
    this.#buffer.push(...data);
    return wireOk(undefined);
  }

  async read(maximumBytes, _timeout) {
    if (this.#config === null) {
      return wireErr({ tag: "NotConfigured" });
    }
    const take = Math.min(maximumBytes, this.#buffer.length);
    const chunk = Uint8Array.from(this.#buffer.splice(0, take));
    return wireOk(chunk);
  }
}

class FailingTransport {
  async configure(_config) {
    return wireErr({ tag: "Timeout" });
  }
  async writeAll(_data) {
    return wireErr({ tag: "Timeout" });
  }
  async read(_maximumBytes, _timeout) {
    return wireErr({ tag: "Timeout" });
  }
}

async function testRoundTrip() {
  const transport = new FakeTransport();
  const payload = Uint8Array.from([1, 2, 3, 4, 5]);

  const result = await runTransportRoundtrip(transport, payload);

  assert.deepEqual(Array.from(result), Array.from(payload));
  console.log("PASS: round trip through wasm + JS-implemented Transport");
}

async function testErrorPropagation() {
  const transport = new FailingTransport();
  const payload = Uint8Array.from([9]);

  await assert.rejects(() => runTransportRoundtrip(transport, payload));
  console.log("PASS: TransportError from JS propagates back through wasm");
}

// registerTransport/unregisterTransport aren't exercised directly by
// runTransportRoundtrip's own bookkeeping, but touch them to make sure the
// low-level handle registration path itself doesn't throw.
function testHandleRegistration() {
  const handle = registerTransport(new FakeTransport());
  assert.ok(handle !== 0, "handle should be non-zero");
  console.log("PASS: callback handle registration");
}

await testHandleRegistration();
await testRoundTrip();
await testErrorPropagation();

console.log("\nAll wasm/JS Transport tests passed.");
