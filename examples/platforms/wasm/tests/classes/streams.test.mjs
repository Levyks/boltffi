import { assert, demo } from "../support/index.mjs";

export async function run() {
  const bus = demo.EventBus.new();

  const values = bus.subscribeValues()[Symbol.asyncIterator]();
  const nextValue = values.next();
  bus.emitValue(1);
  assert.deepEqual(await nextValue, { value: 1, done: false });
  await values.return();

  const points = bus.subscribePoints()[Symbol.asyncIterator]();
  const nextPoint = points.next();
  bus.emitPoint({ x: 1, y: 2 });
  assert.deepEqual(await nextPoint, { value: { x: 1, y: 2 }, done: false });
  await points.return();

  const messages = bus.subscribeMessages()[Symbol.asyncIterator]();
  const nextMessage = messages.next();
  bus.emitMessage({ text: "alpha", values: [1, 2] });
  const message = await nextMessage;
  assert.equal(message.done, false);
  assert.equal(message.value.text, "alpha");
  assert.deepEqual(Array.from(message.value.values), [1, 2]);
  await messages.return();

  const batch = bus.subscribeValuesBatch();
  assert.equal(bus.emitBatch([2, 3, 4]), 3);
  assert.deepEqual(batch.popBatch(), [2, 3, 4]);
  batch.dispose();

  const callbackValue = new Promise((resolve) => {
    const cancellable = bus.subscribeValuesCallback((value) => {
      cancellable.cancel();
      resolve(value);
    });
  });
  bus.emitValue(5);
  assert.equal(await callbackValue, 5);

  bus.dispose();
}
