import { assert, assertArrayEqual, assertRejectsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  const worker = demo.AsyncWorker.new("test");
  assert.equal(worker.getPrefix(), "test");
  assert.equal(await worker.process("data"), "test: data");
  // Unlike every other method in this test, this one genuinely suspends
  // across several polls instead of resolving on the first one -- real
  // regression coverage for a genuine Pending/wake/re-poll cycle on an
  // async class method.
  assert.equal(await worker.processAfterYielding("data"), "test: data");
  assert.equal(await worker.tryProcess("data"), "test: data");
  await assertRejectsWithMessage(() => worker.tryProcess(""), "input must not be empty");
  assert.equal(await worker.findItem(42), "test_42");
  assert.equal(await worker.findItem(-1), null);
  assertArrayEqual(await worker.processBatch(["x", "y"]), ["test: x", "test: y"]);
  worker.dispose();
}
