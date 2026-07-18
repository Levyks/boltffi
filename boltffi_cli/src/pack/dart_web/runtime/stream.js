export var StreamPollResult = /*#__PURE__*/ function(StreamPollResult) {
    StreamPollResult[StreamPollResult["Ready"] = 0] = "Ready";
    StreamPollResult[StreamPollResult["Closed"] = 1] = "Closed";
    return StreamPollResult;
}({});
export class StreamPollManager {
    pending = new Map();
    poll(handle, pollHandle) {
        if (handle === 0) {
            return Promise.resolve(1);
        }
        if (this.pending.has(handle)) {
            return Promise.reject(new Error(`Stream ${handle} already has a pending poll`));
        }
        return new Promise((resolve, reject)=>{
            this.pending.set(handle, {
                resolve,
                reject
            });
            try {
                pollHandle(handle);
            } catch (error) {
                this.pending.delete(handle);
                reject(error instanceof Error ? error : new Error(String(error)));
            }
        });
    }
    wake(handle, result) {
        const pending = this.pending.get(handle);
        if (pending === undefined) {
            return;
        }
        this.pending.delete(handle);
        if (result === 0 || result === 1) {
            pending.resolve(result);
            return;
        }
        pending.reject(new Error(`Unknown stream poll result: ${result}`));
    }
}
export class StreamSession {
    handle;
    batch;
    pollHandle;
    polls;
    unsubscribeHandle;
    freeHandle;
    closed;
    unsubscribed = false;
    constructor(handle, batch, pollHandle, polls, unsubscribeHandle, freeHandle){
        this.handle = handle;
        this.batch = batch;
        this.pollHandle = pollHandle;
        this.polls = polls;
        this.unsubscribeHandle = unsubscribeHandle;
        this.freeHandle = freeHandle;
        this.closed = handle === 0;
    }
    popBatch(maxCount = 16) {
        return this.closed || this.handle === 0 ? [] : this.batch(this.handle, maxCount);
    }
    unsubscribe() {
        if (!this.unsubscribed && this.handle !== 0) {
            this.unsubscribed = true;
            this.unsubscribeHandle(this.handle);
        }
    }
    consume(callback) {
        return new StreamCancellable(this, callback);
    }
    dispose() {
        if (this.closed) {
            return;
        }
        this.closed = true;
        if (this.handle !== 0) {
            this.unsubscribe();
            this.freeHandle(this.handle);
        }
    }
    async *[Symbol.asyncIterator]() {
        try {
            while(!this.closed){
                const items = this.popBatch();
                if (items.length !== 0) {
                    yield* items;
                    continue;
                }
                const result = await this.polls.poll(this.handle, this.pollHandle);
                if (this.closed) {
                    return;
                }
                if (result === 1) {
                    let remaining = this.popBatch();
                    while(remaining.length !== 0){
                        yield* remaining;
                        remaining = this.popBatch();
                    }
                    return;
                }
            }
        } finally{
            this.dispose();
        }
    }
}
export class StreamCancellable {
    session;
    done;
    constructor(session, callback){
        this.session = session;
        this.done = this.consume(callback);
    }
    cancel() {
        this.session.dispose();
    }
    async consume(callback) {
        const iterator = this.session[Symbol.asyncIterator]();
        try {
            let next = await iterator.next();
            while(!next.done){
                callback(next.value);
                next = await iterator.next();
            }
        } finally{
            await iterator.return?.();
        }
    }
}
