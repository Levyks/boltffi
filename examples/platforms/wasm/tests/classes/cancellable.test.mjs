import { assert, demo } from "../support/index.mjs";

export async function run() {
  const counter = demo.CancellableCounter.new();
  assert.equal(counter.progress(), 0);
  const total = await counter.countTo(3, 10);
  assert.ok(total >= 3);
  assert.ok(counter.progress() >= 3);
  counter.dispose();
}
