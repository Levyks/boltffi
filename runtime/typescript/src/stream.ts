export type StreamBatch<T> = (handle: number, maxCount: number) => T[];
export type StreamLifecycle = (handle: number) => void;

export class StreamSession<T> implements AsyncIterable<T>, Disposable {
  private closed = false;
  private unsubscribed = false;

  constructor(
    private readonly handle: number,
    private readonly batch: StreamBatch<T>,
    private readonly unsubscribeHandle: StreamLifecycle,
    private readonly freeHandle: StreamLifecycle
  ) {}

  popBatch(maxCount = 16): T[] {
    return this.closed || this.handle === 0 ? [] : this.batch(this.handle, maxCount);
  }

  unsubscribe(): void {
    if (!this.unsubscribed && this.handle !== 0) {
      this.unsubscribed = true;
      this.unsubscribeHandle(this.handle);
    }
  }

  consume(callback: (item: T) => void): StreamCancellable<T> {
    return new StreamCancellable(this, callback);
  }

  [Symbol.dispose](): void {
    this.dispose();
  }

  dispose(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    if (this.handle !== 0) {
      this.unsubscribe();
      this.freeHandle(this.handle);
    }
  }

  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    try {
      while (!this.closed) {
        const items = this.popBatch();
        let index = 0;
        while (index < items.length) {
          yield items[index];
          index += 1;
        }
        if (items.length === 0) {
          await new Promise<void>((resolve) => setTimeout(resolve, 0));
        }
      }
    } finally {
      this.dispose();
    }
  }
}

export class StreamCancellable<T> implements Disposable {
  readonly done: Promise<void>;

  constructor(
    private readonly session: StreamSession<T>,
    callback: (item: T) => void
  ) {
    this.done = this.consume(callback);
  }

  cancel(): void {
    this.session.dispose();
  }

  [Symbol.dispose](): void {
    this.cancel();
  }

  private async consume(callback: (item: T) => void): Promise<void> {
    const iterator = this.session[Symbol.asyncIterator]();
    let next = await iterator.next();
    while (!next.done) {
      callback(next.value);
      next = await iterator.next();
    }
  }
}
