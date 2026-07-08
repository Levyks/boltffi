import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.keyword_fields.typed_event.should_roundtrip_raw_identifier_field");
  const event = { id: 99n, type_: "circle" };
  assert.deepEqual(demo.echoTypedEvent(event), event);
}