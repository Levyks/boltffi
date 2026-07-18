import { WireReader, WireWriter } from "./wire.js";
import { StreamPollManager } from "./stream.js";
const FFI_BUF_DESCRIPTOR_SIZE = 16;
const FFI_STATUS_SIZE = 4;
const OPTION_F64_NONE = 0xffff_ffff_ffff_ffffn;
const OPTION_F64_NAN = 0x7ff8_0000_0000_0000n;
const MIN_WRITER_CAPACITY = 64;
const MAX_WRITERS_PER_CAPACITY = 32;
var PackedPrimitive = /*#__PURE__*/ function(PackedPrimitive) {
    PackedPrimitive[PackedPrimitive["Bool"] = 0] = "Bool";
    PackedPrimitive[PackedPrimitive["I8"] = 1] = "I8";
    PackedPrimitive[PackedPrimitive["U8"] = 2] = "U8";
    PackedPrimitive[PackedPrimitive["I16"] = 3] = "I16";
    PackedPrimitive[PackedPrimitive["U16"] = 4] = "U16";
    PackedPrimitive[PackedPrimitive["I32"] = 5] = "I32";
    PackedPrimitive[PackedPrimitive["U32"] = 6] = "U32";
    PackedPrimitive[PackedPrimitive["I64"] = 7] = "I64";
    PackedPrimitive[PackedPrimitive["U64"] = 8] = "U64";
    PackedPrimitive[PackedPrimitive["F32"] = 9] = "F32";
    PackedPrimitive[PackedPrimitive["F64"] = 10] = "F64";
    return PackedPrimitive;
}(PackedPrimitive || {});
export var WasmPollStatus = /*#__PURE__*/ function(WasmPollStatus) {
    WasmPollStatus[WasmPollStatus["Pending"] = 0] = "Pending";
    WasmPollStatus[WasmPollStatus["Ready"] = 1] = "Ready";
    WasmPollStatus[WasmPollStatus["Cancelled"] = -1] = "Cancelled";
    WasmPollStatus[WasmPollStatus["Panicked"] = -2] = "Panicked";
    return WasmPollStatus;
}({});
export class BoltFFIPanicError extends Error {
    constructor(message){
        super(message);
        this.name = "BoltFFIPanicError";
    }
}
export class BoltFFICancelledError extends Error {
    constructor(){
        super("Future was cancelled");
        this.name = "BoltFFICancelledError";
    }
}
export class AsyncFutureManager {
    pendingFutures = new Map();
    wokenHandles = new Set();
    drainScheduled = false;
    _module = null;
    setModule(module) {
        this._module = module;
    }
    wake(handle) {
        this.wokenHandles.add(handle);
        if (!this.drainScheduled) {
            this.drainScheduled = true;
            queueMicrotask(()=>this.drainWakes());
        }
    }
    drainWakes() {
        this.drainScheduled = false;
        const batch = [
            ...this.wokenHandles
        ];
        this.wokenHandles.clear();
        for (const handle of batch){
            this.repollHandle(handle);
        }
    }
    repollHandle(handle) {
        const entry = this.pendingFutures.get(handle);
        if (!entry) return;
        const status = entry.pollSync(handle);
        if (status === 1) {
            this.pendingFutures.delete(handle);
            entry.resolve(handle);
        } else if (status < 0) {
            this.pendingFutures.delete(handle);
            entry.reject(this.extractAsyncError(handle, status, entry));
        }
    }
    extractAsyncError(handle, status, entry) {
        if (status === -2 && this._module) {
            const bufPtr = entry.panicMessage(handle);
            const reader = this._module.readerFromBuf(bufPtr);
            const message = reader.readString();
            this._module.freeBuf(bufPtr);
            entry.free(handle);
            return new BoltFFIPanicError(message);
        }
        entry.free(handle);
        if (status === -1) {
            return new BoltFFICancelledError();
        }
        return new Error(`Unknown poll status: ${status}`);
    }
    pollAsync(handle, pollSync, panicMessage, free) {
        return new Promise((resolve, reject)=>{
            this.pendingFutures.set(handle, {
                resolve,
                reject,
                pollSync,
                panicMessage,
                free
            });
            const status = pollSync(handle);
            if (status === 1) {
                this.pendingFutures.delete(handle);
                resolve(handle);
            } else if (status < 0) {
                this.pendingFutures.delete(handle);
                reject(this.extractAsyncError(handle, status, {
                    resolve,
                    reject,
                    pollSync,
                    panicMessage,
                    free
                }));
            }
        });
    }
}
export const WASM_ABI_VERSION = 2;
export class BoltFFIModule {
    exports;
    asyncManager;
    streamManager;
    _memory;
    _encoder;
    _decoder;
    _writerPool;
    _cachedU8 = null;
    _cachedI8 = null;
    _cachedI16 = null;
    _cachedU16 = null;
    _cachedI32 = null;
    _cachedU32 = null;
    _cachedI64 = null;
    _cachedU64 = null;
    _cachedF32 = null;
    _cachedF64 = null;
    _cachedView = null;
    _optionF64Storage = new ArrayBuffer(8);
    _optionF64Bits = new BigUint64Array(this._optionF64Storage);
    _optionF64Values = new Float64Array(this._optionF64Storage);
    _returnSlotAddr = 0;
    constructor(instance, asyncManager, streamManager){
        this.exports = instance.exports;
        this._memory = this.exports.memory;
        this._encoder = new TextEncoder();
        this._decoder = new TextDecoder("utf-8");
        this._writerPool = new Map();
        this.asyncManager = asyncManager;
        this.streamManager = streamManager;
        asyncManager.setModule(this);
        this._returnSlotAddr = this.exports.boltffi_wasm_return_slot_addr();
    }
    readReturnSlot() {
        const view = this.getU32();
        const idx = this._returnSlotAddr >>> 2;
        return {
            ptr: view[idx],
            len: view[idx + 1],
            cap: view[idx + 2],
            align: view[idx + 3]
        };
    }
    writeReturnSlot(allocation, alignment) {
        const view = this.getU32();
        const index = this._returnSlotAddr >>> 2;
        view[index] = allocation.ptr;
        view[index + 1] = allocation.allocationSize;
        view[index + 2] = allocation.allocationSize;
        view[index + 3] = alignment;
    }
    writeWriterReturnSlot(writer, alignment) {
        const view = this.getU32();
        const index = this._returnSlotAddr >>> 2;
        view[index] = writer.ptr;
        view[index + 1] = writer.len;
        view[index + 2] = writer.capacity;
        view[index + 3] = alignment;
    }
    completeAsync(complete) {
        const statusPtr = this.allocStatus();
        try {
            const result = complete(statusPtr);
            this.checkStatus(this.readStatus(statusPtr));
            return result;
        } finally{
            this.freeStatus(statusPtr);
        }
    }
    allocStatus() {
        const ptr = this.exports.boltffi_wasm_alloc(FFI_STATUS_SIZE);
        if (ptr === 0) {
            throw new Error("Failed to allocate memory for status");
        }
        this.getView().setInt32(ptr, 0, true);
        return ptr;
    }
    readStatus(ptr) {
        return this.getView().getInt32(ptr, true);
    }
    freeStatus(ptr) {
        if (ptr !== 0) {
            this.exports.boltffi_wasm_free(ptr, FFI_STATUS_SIZE);
        }
    }
    checkStatus(status) {
        if (status === 0) {
            return;
        }
        if (status === 3) {
            throw new Error("invalid argument");
        }
        if (status === 4) {
            throw new BoltFFICancelledError();
        }
        throw new Error(`FFI failed in async completion with code ${status}`);
    }
    getView() {
        if (this._cachedView === null || this._cachedView.buffer !== this._memory.buffer) {
            this._cachedView = new DataView(this._memory.buffer);
        }
        return this._cachedView;
    }
    getBytes() {
        if (this._cachedU8 === null || this._cachedU8.buffer !== this._memory.buffer) {
            this._cachedU8 = new Uint8Array(this._memory.buffer);
        }
        return this._cachedU8;
    }
    getI8() {
        if (this._cachedI8 === null || this._cachedI8.buffer !== this._memory.buffer) {
            this._cachedI8 = new Int8Array(this._memory.buffer);
        }
        return this._cachedI8;
    }
    getI16() {
        if (this._cachedI16 === null || this._cachedI16.buffer !== this._memory.buffer) {
            this._cachedI16 = new Int16Array(this._memory.buffer);
        }
        return this._cachedI16;
    }
    getU16() {
        if (this._cachedU16 === null || this._cachedU16.buffer !== this._memory.buffer) {
            this._cachedU16 = new Uint16Array(this._memory.buffer);
        }
        return this._cachedU16;
    }
    getI32() {
        if (this._cachedI32 === null || this._cachedI32.buffer !== this._memory.buffer) {
            this._cachedI32 = new Int32Array(this._memory.buffer);
        }
        return this._cachedI32;
    }
    getU32() {
        if (this._cachedU32 === null || this._cachedU32.buffer !== this._memory.buffer) {
            this._cachedU32 = new Uint32Array(this._memory.buffer);
        }
        return this._cachedU32;
    }
    getI64() {
        if (this._cachedI64 === null || this._cachedI64.buffer !== this._memory.buffer) {
            this._cachedI64 = new BigInt64Array(this._memory.buffer);
        }
        return this._cachedI64;
    }
    getU64() {
        if (this._cachedU64 === null || this._cachedU64.buffer !== this._memory.buffer) {
            this._cachedU64 = new BigUint64Array(this._memory.buffer);
        }
        return this._cachedU64;
    }
    getF32() {
        if (this._cachedF32 === null || this._cachedF32.buffer !== this._memory.buffer) {
            this._cachedF32 = new Float32Array(this._memory.buffer);
        }
        return this._cachedF32;
    }
    getF64() {
        if (this._cachedF64 === null || this._cachedF64.buffer !== this._memory.buffer) {
            this._cachedF64 = new Float64Array(this._memory.buffer);
        }
        return this._cachedF64;
    }
    allocString(value) {
        const encoded = this._encoder.encode(value);
        const ptr = this.exports.boltffi_wasm_alloc(encoded.length);
        if (ptr === 0 && encoded.length > 0) {
            throw new Error("Failed to allocate memory for string");
        }
        this.getBytes().set(encoded, ptr);
        return {
            ptr,
            len: encoded.length
        };
    }
    allocOwnedBytes(value) {
        const ptr = this.exports.boltffi_wasm_alloc_owned_bytes(value.length);
        if (ptr === 0 && value.length > 0) {
            throw new Error("Failed to allocate owned bytes");
        }
        this.getBytes().set(value, ptr);
        return {
            ptr,
            len: value.length
        };
    }
    allocOwnedString(value) {
        const len = value.length;
        const ptr = this.exports.boltffi_wasm_alloc_owned_bytes(len);
        if (ptr === 0 && len > 0) {
            throw new Error("Failed to allocate owned string");
        }
        const bytes = new Uint8Array(this._memory.buffer, ptr, len);
        const encoded = this._encoder.encodeInto(value, bytes);
        if (encoded.read === value.length && encoded.written === len) {
            return {
                ptr,
                len
            };
        }
        bytes.fill(0, encoded.written);
        this.exports.boltffi_wasm_free_string_return(ptr, len);
        return this.allocOwnedBytes(this._encoder.encode(value));
    }
    allocOwnedWireString(value) {
        const encoded = this._encoder.encode(value);
        const allocation = this.allocOwnedBytes(new Uint8Array(4 + encoded.length));
        this.getView().setUint32(allocation.ptr, encoded.length, true);
        this.getBytes().set(encoded, allocation.ptr + 4);
        return allocation;
    }
    allocWireString(value) {
        const len = 4 + value.length;
        const ptr = this.exports.boltffi_wasm_alloc(len);
        if (ptr === 0) {
            throw new Error("Failed to allocate memory for wire string");
        }
        const encoded = this._encoder.encodeInto(value, new Uint8Array(this._memory.buffer, ptr + 4, value.length));
        if (encoded.read !== value.length) {
            this.exports.boltffi_wasm_free(ptr, len);
            return this.allocWireBytes(this._encoder.encode(value));
        }
        this.getView().setUint32(ptr, encoded.written, true);
        return {
            ptr,
            len: 4 + encoded.written
        };
    }
    allocWireBytes(value) {
        const len = 4 + value.length;
        const ptr = this.exports.boltffi_wasm_alloc(len);
        if (ptr === 0) {
            throw new Error("Failed to allocate memory for wire value");
        }
        this.getView().setUint32(ptr, value.length, true);
        this.getBytes().set(value, ptr + 4);
        return {
            ptr,
            len
        };
    }
    freeAlloc(alloc) {
        if (alloc.ptr !== 0 && alloc.len !== 0) {
            this.exports.boltffi_wasm_free(alloc.ptr, alloc.len);
        }
    }
    allocBytes(value) {
        const ptr = this.exports.boltffi_wasm_alloc(value.length);
        if (ptr === 0 && value.length > 0) {
            throw new Error("Failed to allocate memory for bytes");
        }
        this.getBytes().set(value, ptr);
        return {
            ptr,
            len: value.length
        };
    }
    allocStreamBuffer(itemCapacity, itemSize) {
        const len = itemCapacity * itemSize;
        const ptr = this.exports.boltffi_wasm_alloc(len);
        if (ptr === 0 && len > 0) {
            throw new Error("Failed to allocate stream buffer");
        }
        return {
            ptr,
            len
        };
    }
    allocI8Array(value) {
        const len = value.length;
        const byteLen = len;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        new Int8Array(this._memory.buffer, ptr, len).set(value);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    borrowBoolArray(ptr, len) {
        return Array.from(this.getBytes().subarray(ptr, ptr + len), (value)=>value !== 0);
    }
    borrowI8Array(ptr, len) {
        return this.getI8().subarray(ptr, ptr + len);
    }
    borrowI16Array(ptr, len) {
        return this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + len);
    }
    borrowU16Array(ptr, len) {
        return this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + len);
    }
    borrowI32Array(ptr, len) {
        return this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + len);
    }
    borrowU32Array(ptr, len) {
        return this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + len);
    }
    borrowI64Array(ptr, len) {
        return this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + len);
    }
    borrowU64Array(ptr, len) {
        return this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + len);
    }
    borrowF32Array(ptr, len) {
        return this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + len);
    }
    borrowF64Array(ptr, len) {
        return this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + len);
    }
    allocU8Array(value) {
        const len = value.length;
        const ptr = this.exports.boltffi_wasm_alloc(len);
        this.getBytes().set(value, ptr);
        return {
            ptr,
            len,
            allocationSize: len
        };
    }
    allocI16Array(value) {
        const len = value.length;
        const byteLen = len << 1;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getI16().set(value, ptr >>> 1);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocU16Array(value) {
        const len = value.length;
        const byteLen = len << 1;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getU16().set(value, ptr >>> 1);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocI32Array(value) {
        const len = value.length;
        const byteLen = len << 2;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getI32().set(value, ptr >>> 2);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocU32Array(value) {
        const len = value.length;
        const byteLen = len << 2;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getU32().set(value, ptr >>> 2);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocI64Array(value) {
        const len = value.length;
        const byteLen = len << 3;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getI64().set(value, ptr >>> 3);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocU64Array(value) {
        const len = value.length;
        const byteLen = len << 3;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getU64().set(value, ptr >>> 3);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocF32Array(value) {
        const len = value.length;
        const byteLen = len << 2;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getF32().set(value, ptr >>> 2);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocF64Array(value) {
        const len = value.length;
        const byteLen = len << 3;
        const ptr = this.exports.boltffi_wasm_alloc(byteLen);
        this.getF64().set(value, ptr >>> 3);
        return {
            ptr,
            len,
            allocationSize: byteLen
        };
    }
    allocBoolArray(value) {
        const len = value.length;
        const ptr = this.exports.boltffi_wasm_alloc(len);
        const view = new Uint8Array(this._memory.buffer, ptr, len);
        for(let i = 0; i < len; i++){
            view[i] = value[i] ? 1 : 0;
        }
        return {
            ptr,
            len,
            allocationSize: len
        };
    }
    allocPrimitiveBuffer(value, elementType) {
        const bytesPerElement = this.primitiveElementSize(elementType);
        const elementCount = value.length;
        const allocationSize = elementCount * bytesPerElement;
        const ptr = this.exports.boltffi_wasm_alloc(allocationSize);
        if (ptr === 0 && allocationSize > 0) {
            throw new Error("Failed to allocate memory for primitive buffer");
        }
        const view = this.getView();
        value.forEach((entry, index)=>{
            const offset = ptr + index * bytesPerElement;
            this.writePrimitiveElement(view, offset, entry, elementType);
        });
        return {
            ptr,
            len: elementCount,
            allocationSize
        };
    }
    allocCompositeBuffer(value, elementSize, writeElement) {
        const writer = this.allocWriter(value.length * elementSize);
        value.forEach((entry)=>writeElement(writer, entry));
        return writer;
    }
    borrowRecordArray(ptr, byteLen, stride, decode) {
        if (ptr === 0 || byteLen === 0) return [];
        if (byteLen % stride !== 0) {
            throw new Error(`Invalid record array byte length ${byteLen} for stride ${stride}`);
        }
        const count = byteLen / stride;
        const result = new Array(count);
        const reader = new WireReader(this._memory.buffer, ptr);
        for(let index = 0; index < count; index++){
            result[index] = decode(reader);
        }
        return result;
    }
    freePrimitiveBuffer(allocation) {
        if (allocation.ptr !== 0 && allocation.allocationSize !== 0) {
            this.exports.boltffi_wasm_free(allocation.ptr, allocation.allocationSize);
        }
    }
    copyPrimitiveBufferInto(allocation, target, elementType) {
        const { ptr, len } = allocation;
        switch(elementType){
            case "i8":
                target.set(this.getI8().subarray(ptr, ptr + len));
                return;
            case "i16":
                target.set(this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + len));
                return;
            case "u16":
                target.set(this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + len));
                return;
            case "i32":
            case "isize":
                target.set(this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + len));
                return;
            case "u32":
            case "usize":
                target.set(this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + len));
                return;
            case "i64":
                target.set(this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + len));
                return;
            case "u64":
                target.set(this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + len));
                return;
            case "f32":
                target.set(this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + len));
                return;
            case "f64":
                target.set(this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + len));
        }
    }
    allocWriter(size) {
        const requestedCapacity = Math.max(size, MIN_WRITER_CAPACITY);
        const pooled = this._writerPool.get(requestedCapacity);
        if (pooled !== undefined) {
            const writer = pooled.pop();
            if (writer !== undefined) {
                writer.reset();
                return writer;
            }
        }
        const allocator = {
            alloc: (allocationSize)=>this.exports.boltffi_wasm_alloc(allocationSize),
            realloc: (ptr, oldSize, newSize)=>this.exports.boltffi_wasm_realloc(ptr, oldSize, newSize),
            free: (ptr, allocationSize)=>this.exports.boltffi_wasm_free(ptr, allocationSize),
            buffer: ()=>this._memory.buffer
        };
        return WireWriter.withWasmAllocation(requestedCapacity, allocator);
    }
    allocOwnedWriter(size) {
        const allocator = {
            alloc: (allocationSize)=>this.exports.boltffi_wasm_alloc_owned_bytes(allocationSize),
            realloc: ()=>{
                throw new Error("Owned writer exceeded its size plan");
            },
            free: (ptr, allocationSize)=>this.exports.boltffi_wasm_free_string_return(ptr, allocationSize),
            buffer: ()=>this._memory.buffer
        };
        return WireWriter.withWasmAllocation(size, allocator);
    }
    freeWriter(writer) {
        const capacity = writer.capacity;
        writer.reset();
        const bucket = this._writerPool.get(capacity) ?? [];
        if (bucket.length < MAX_WRITERS_PER_CAPACITY) {
            bucket.push(writer);
            this._writerPool.set(capacity, bucket);
            return;
        }
        writer.release();
    }
    readerFromWriter(writer) {
        return new WireReader(this._memory.buffer, writer.ptr);
    }
    writerFromMemory(ptr, size) {
        return WireWriter.withWasmRegion(ptr, size, ()=>this._memory.buffer);
    }
    allocBufDescriptor() {
        const ptr = this.exports.boltffi_wasm_alloc(FFI_BUF_DESCRIPTOR_SIZE);
        if (ptr === 0) {
            throw new Error("Failed to allocate memory for buffer descriptor");
        }
        new Uint8Array(this._memory.buffer, ptr, FFI_BUF_DESCRIPTOR_SIZE).fill(0);
        return ptr;
    }
    freeBufDescriptor(ptr) {
        if (ptr !== 0) {
            this.exports.boltffi_wasm_free(ptr, FFI_BUF_DESCRIPTOR_SIZE);
        }
    }
    readerFromBuf(bufPtr) {
        const view = this.getView();
        const ptr = view.getUint32(bufPtr, true);
        return new WireReader(this._memory.buffer, ptr);
    }
    freeBuf(bufPtr) {
        const { ptr, cap, align } = this.readBufDescriptor(bufPtr);
        if (ptr !== 0 && cap !== 0) {
            this.exports.boltffi_wasm_free_buf(ptr, cap, align);
        }
        this.exports.boltffi_wasm_free(bufPtr, FFI_BUF_DESCRIPTOR_SIZE);
    }
    writeBufDescriptor(bufPtr, dataPtr, dataLen, dataCap, dataAlign = 1) {
        const view = this.getView();
        view.setUint32(bufPtr, dataPtr, true);
        view.setUint32(bufPtr + 4, dataLen, true);
        view.setUint32(bufPtr + 8, dataCap, true);
        view.setUint32(bufPtr + 12, dataAlign, true);
    }
    writeCallbackBuffer(bufPtr, dataPtr, dataLen, dataCap) {
        const view = this.getView();
        view.setUint32(bufPtr, dataPtr, true);
        view.setUint32(bufPtr + 4, dataLen, true);
        view.setUint32(bufPtr + 8, dataCap, true);
    }
    readBufDescriptor(bufPtr) {
        const view = this.getView();
        return {
            ptr: view.getUint32(bufPtr, true),
            len: view.getUint32(bufPtr + 4, true),
            cap: view.getUint32(bufPtr + 8, true),
            align: view.getUint32(bufPtr + 12, true) || 1
        };
    }
    takeBufU8Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Uint8Array(0);
        return this.getBytes().subarray(ptr, ptr + len).slice();
    }
    takeBufI8Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Int8Array(0);
        return this.getI8().subarray(ptr, ptr + len).slice();
    }
    takeBufI16Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Int16Array(0);
        const elemCount = len >>> 1;
        return this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
    }
    takeBufU16Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Uint16Array(0);
        const elemCount = len >>> 1;
        return this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
    }
    takeBufI32Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Int32Array(0);
        const elemCount = len >>> 2;
        return this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
    }
    takeBufU32Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Uint32Array(0);
        const elemCount = len >>> 2;
        return this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
    }
    takeBufI64Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new BigInt64Array(0);
        const elemCount = len >>> 3;
        return this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
    }
    takeBufU64Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new BigUint64Array(0);
        const elemCount = len >>> 3;
        return this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
    }
    takeBufF32Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Float32Array(0);
        const elemCount = len >>> 2;
        return this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
    }
    takeBufF64Array(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return new Float64Array(0);
        const elemCount = len >>> 3;
        return this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
    }
    takeBufBoolArray(bufPtr) {
        const { ptr, len } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return [];
        const bytes = this.getBytes().subarray(ptr, ptr + len);
        return Array.from(bytes, (value)=>value !== 0);
    }
    takeBufStructArray(bufPtr, stride, decode) {
        const { ptr, len: byteLen } = this.readBufDescriptor(bufPtr);
        if (ptr === 0) return [];
        const copy = new Uint8Array(this._memory.buffer, ptr, byteLen).slice();
        const view = new DataView(copy.buffer, copy.byteOffset, copy.byteLength);
        const count = byteLen / stride | 0;
        return Array.from({
            length: count
        }, (_, index)=>decode(view, index * stride));
    }
    writeToMemory(ptr, data) {
        this.getBytes().set(data, ptr);
    }
    writeI32(ptr, value) {
        this.getView().setInt32(ptr, value, true);
    }
    writeU64(ptr, value) {
        this.getView().setBigUint64(ptr, value, true);
    }
    readFromMemory(ptr, len) {
        return this.getBytes().slice(ptr, ptr + len);
    }
    readerFromMemory(ptr, len) {
        const bytes = this.readFromMemory(ptr, len);
        return new WireReader(bytes.buffer, bytes.byteOffset);
    }
    unpackPacked(packed) {
        return {
            pointer: Number(packed & 0xffff_ffffn),
            length: Number(packed >> 32n & 0xffff_ffffn)
        };
    }
    freePacked(pointer, length) {
        if (pointer !== 0 && length !== 0) {
            this.exports.boltffi_wasm_free_string_return(pointer, length);
        }
    }
    takePackedOptionalPrimitive(packed, encodedSize, primitive) {
        const { pointer, length } = this.unpackPacked(packed);
        if (pointer === 0 || length === 0) {
            return null;
        }
        try {
            const view = this.getView();
            if (view.getUint8(pointer) === 0) {
                return null;
            }
            if (length < 1 + encodedSize) {
                throw new Error("Invalid packed optional payload");
            }
            const valueOffset = pointer + 1;
            switch(primitive){
                case 0:
                    return view.getUint8(valueOffset) !== 0;
                case 1:
                    return view.getInt8(valueOffset);
                case 2:
                    return view.getUint8(valueOffset);
                case 3:
                    return view.getInt16(valueOffset, true);
                case 4:
                    return view.getUint16(valueOffset, true);
                case 5:
                    return view.getInt32(valueOffset, true);
                case 6:
                    return view.getUint32(valueOffset, true);
                case 7:
                    return view.getBigInt64(valueOffset, true);
                case 8:
                    return view.getBigUint64(valueOffset, true);
                case 9:
                    return view.getFloat32(valueOffset, true);
                case 10:
                    return view.getFloat64(valueOffset, true);
            }
        } finally{
            this.freePacked(pointer, length);
        }
    }
    takePackedOptionalBool(packed) {
        return this.takePackedOptionalPrimitive(packed, 1, 0);
    }
    takePackedOptionalI8(packed) {
        return this.takePackedOptionalPrimitive(packed, 1, 1);
    }
    takePackedOptionalU8(packed) {
        return this.takePackedOptionalPrimitive(packed, 1, 2);
    }
    takePackedOptionalI16(packed) {
        return this.takePackedOptionalPrimitive(packed, 2, 3);
    }
    takePackedOptionalU16(packed) {
        return this.takePackedOptionalPrimitive(packed, 2, 4);
    }
    takePackedOptionalI32(packed) {
        return this.takePackedOptionalPrimitive(packed, 4, 5);
    }
    takePackedOptionalU32(packed) {
        return this.takePackedOptionalPrimitive(packed, 4, 6);
    }
    takePackedOptionalI64(packed) {
        return this.takePackedOptionalPrimitive(packed, 8, 7);
    }
    takePackedOptionalU64(packed) {
        return this.takePackedOptionalPrimitive(packed, 8, 8);
    }
    takePackedOptionalF32(packed) {
        return this.takePackedOptionalPrimitive(packed, 4, 9);
    }
    takePackedOptionalF64(packed) {
        return this.takePackedOptionalPrimitive(packed, 8, 10);
    }
    unpackOptionBool(packed) {
        if (Number.isNaN(packed)) return null;
        return packed !== 0;
    }
    unpackOptionI8(packed) {
        if (Number.isNaN(packed)) return null;
        return packed | 0;
    }
    unpackOptionU8(packed) {
        if (Number.isNaN(packed)) return null;
        return packed >>> 0;
    }
    unpackOptionI16(packed) {
        if (Number.isNaN(packed)) return null;
        return packed | 0;
    }
    unpackOptionU16(packed) {
        if (Number.isNaN(packed)) return null;
        return packed >>> 0;
    }
    unpackOptionI32(packed) {
        if (Number.isNaN(packed)) return null;
        return packed | 0;
    }
    unpackOptionU32(packed) {
        if (Number.isNaN(packed)) return null;
        return packed >>> 0;
    }
    packOptionScalar(value) {
        if (value === null) return Number.NaN;
        if (typeof value === "boolean") return value ? 1 : 0;
        return value;
    }
    packOptionF64Bits(value) {
        if (value === null) return OPTION_F64_NONE;
        if (Number.isNaN(value)) return OPTION_F64_NAN;
        this._optionF64Values[0] = value;
        return this._optionF64Bits[0];
    }
    unpackOptionF64Bits(packed) {
        if (packed === OPTION_F64_NONE) return null;
        this._optionF64Bits[0] = packed;
        return this._optionF64Values[0];
    }
    unpackOptionF32(packed) {
        if (Number.isNaN(packed)) return null;
        return packed;
    }
    unpackOptionF64(packed) {
        if (!Number.isNaN(packed)) return packed;
        const slotIndex = this._returnSlotAddr >>> 2;
        return this.getU32()[slotIndex] === 0 ? null : packed;
    }
    takePackedUtf8String(packed) {
        const { pointer, length } = this.unpackPacked(packed);
        if (pointer === 0 || length === 0) {
            return "";
        }
        const bytes = new Uint8Array(this._memory.buffer, pointer, length);
        try {
            return this._decoder.decode(bytes);
        } finally{
            this.freePacked(pointer, length);
        }
    }
    takePackedWireString(packed) {
        const { pointer, length } = this.unpackPacked(packed);
        if (pointer === 0 || length < 4) {
            throw new Error("Invalid packed wire string");
        }
        try {
            const payloadLength = this.getView().getUint32(pointer, true);
            if (payloadLength !== length - 4) {
                throw new Error("Invalid packed wire string length");
            }
            return this._decoder.decode(new Uint8Array(this._memory.buffer, pointer + 4, payloadLength));
        } finally{
            this.freePacked(pointer, length);
        }
    }
    takePackedWireBytes(packed) {
        const { pointer, length } = this.unpackPacked(packed);
        if (pointer === 0 || length < 4) {
            throw new Error("Invalid packed wire bytes");
        }
        try {
            const payloadLength = this.getView().getUint32(pointer, true);
            if (payloadLength !== length - 4) {
                throw new Error("Invalid packed wire bytes length");
            }
            return new Uint8Array(this._memory.buffer, pointer + 4, payloadLength).slice();
        } finally{
            this.freePacked(pointer, length);
        }
    }
    takePackedBuffer(packed) {
        const { pointer, length } = this.unpackPacked(packed);
        if (pointer === 0 || length === 0) {
            return new WireReader(new ArrayBuffer(0), 0);
        }
        const bytes = new Uint8Array(this._memory.buffer, pointer, length);
        const copy = bytes.slice();
        this.freePacked(pointer, length);
        return new WireReader(copy.buffer, 0);
    }
    takePackedI8Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Int8Array(0);
        const result = this.getI8().subarray(pointer, pointer + byteLen).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedU8Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Uint8Array(0);
        const result = this.getBytes().subarray(pointer, pointer + byteLen).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    readSlot() {
        const slotView = this.getU32();
        const slotIdx = this._returnSlotAddr >>> 2;
        return {
            ptr: slotView[slotIdx],
            len: slotView[slotIdx + 1],
            cap: slotView[slotIdx + 2],
            align: slotView[slotIdx + 3] || 1
        };
    }
    freeSlotBuf(ptr, cap, align) {
        this.exports.boltffi_wasm_free_buf(ptr, cap, align);
    }
    takeSlotU8Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Uint8Array(0);
        const result = this.getBytes().subarray(ptr, ptr + len).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotI8Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Int8Array(0);
        const result = this.getI8().subarray(ptr, ptr + len).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotI32Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Int32Array(0);
        const elemCount = len >>> 2;
        const result = this.getI32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotU32Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Uint32Array(0);
        const elemCount = len >>> 2;
        const result = this.getU32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotF32Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Float32Array(0);
        const elemCount = len >>> 2;
        const result = this.getF32().subarray(ptr >>> 2, (ptr >>> 2) + elemCount).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotF64Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Float64Array(0);
        const elemCount = len >>> 3;
        const result = this.getF64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotI16Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Int16Array(0);
        const elemCount = len >>> 1;
        const result = this.getI16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotU16Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new Uint16Array(0);
        const elemCount = len >>> 1;
        const result = this.getU16().subarray(ptr >>> 1, (ptr >>> 1) + elemCount).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotI64Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new BigInt64Array(0);
        const elemCount = len >>> 3;
        const result = this.getI64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotU64Array() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return new BigUint64Array(0);
        const elemCount = len >>> 3;
        return this.getU64().subarray(ptr >>> 3, (ptr >>> 3) + elemCount).slice();
    }
    takeSlotBoolArray() {
        const { ptr, len, cap, align } = this.readSlot();
        if (ptr === 0) return [];
        const bytes = this.getBytes().subarray(ptr, ptr + len);
        const result = Array.from(bytes, (b)=>b !== 0);
        this.freeSlotBuf(ptr, cap, align);
        return result;
    }
    takeSlotStructArray(stride, decode) {
        const { ptr, len: byteLen, cap, align } = this.readSlot();
        if (ptr === 0) return [];
        const count = byteLen / stride | 0;
        const copy = new Uint8Array(this._memory.buffer, ptr, byteLen).slice();
        this.freeSlotBuf(ptr, cap, align);
        const view = new DataView(copy.buffer, copy.byteOffset, copy.byteLength);
        const result = new Array(count);
        for(let i = 0; i < count; i++){
            result[i] = decode(view, i * stride);
        }
        return result;
    }
    takeSlotRecordArray(stride, decode) {
        const { ptr, len: byteLen, cap, align } = this.readSlot();
        if (ptr === 0) return [];
        try {
            return this.borrowRecordArray(ptr, byteLen, stride, decode);
        } finally{
            this.freeSlotBuf(ptr, cap, align);
        }
    }
    takePackedI16Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Int16Array(0);
        const elemCount = byteLen / 2;
        const result = new Int16Array(this._memory.buffer, pointer, elemCount).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedU16Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Uint16Array(0);
        const elemCount = byteLen / 2;
        const result = new Uint16Array(this._memory.buffer, pointer, elemCount).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedI32Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Int32Array(0);
        const elemCount = byteLen / 4;
        const result = this.getI32().subarray(pointer / 4, pointer / 4 + elemCount).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedU32Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Uint32Array(0);
        const elemCount = byteLen / 4;
        const result = this.getU32().subarray(pointer / 4, pointer / 4 + elemCount).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedI64Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new BigInt64Array(0);
        const result = new BigInt64Array(this._memory.buffer, pointer, byteLen / 8).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedU64Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new BigUint64Array(0);
        const result = new BigUint64Array(this._memory.buffer, pointer, byteLen / 8).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedF32Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Float32Array(0);
        const elemCount = byteLen / 4;
        const result = this.getF32().subarray(pointer / 4, pointer / 4 + elemCount).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    takePackedF64Array(packed) {
        const pointer = Number(packed & 0xffff_ffffn);
        const byteLen = Number(packed >> 32n & 0xffff_ffffn);
        if (pointer === 0 || byteLen === 0) return new Float64Array(0);
        const elemCount = byteLen / 8;
        const result = this.getF64().subarray(pointer / 8, pointer / 8 + elemCount).slice();
        this.exports.boltffi_wasm_free_string_return(pointer, byteLen);
        return result;
    }
    primitiveElementSize(elementType) {
        switch(elementType){
            case "bool":
            case "i8":
            case "u8":
                return 1;
            case "i16":
            case "u16":
                return 2;
            case "i32":
            case "u32":
            case "isize":
            case "usize":
            case "f32":
                return 4;
            case "i64":
            case "u64":
            case "f64":
                return 8;
        }
    }
    writePrimitiveElement(view, offset, value, elementType) {
        switch(elementType){
            case "bool":
                view.setUint8(offset, value ? 1 : 0);
                return;
            case "i8":
                view.setInt8(offset, Number(value));
                return;
            case "u8":
                view.setUint8(offset, Number(value));
                return;
            case "i16":
                view.setInt16(offset, Number(value), true);
                return;
            case "u16":
                view.setUint16(offset, Number(value), true);
                return;
            case "i32":
            case "isize":
                view.setInt32(offset, Number(value), true);
                return;
            case "u32":
            case "usize":
                view.setUint32(offset, Number(value), true);
                return;
            case "i64":
                view.setBigInt64(offset, BigInt(value), true);
                return;
            case "u64":
                view.setBigUint64(offset, BigInt(value), true);
                return;
            case "f32":
                view.setFloat32(offset, Number(value), true);
                return;
            case "f64":
                view.setFloat64(offset, Number(value), true);
                return;
        }
    }
}
function createUnimplementedImport(importName) {
    return ()=>{
        throw new Error(`Unimplemented wasm import: ${importName}`);
    };
}
function createImportModuleProxy(moduleName) {
    return new Proxy({}, {
        get: (_target, propertyName)=>createUnimplementedImport(`${moduleName}.${String(propertyName)}`)
    });
}
export async function instantiateBoltFFI(source, expectedVersion, imports) {
    let wasmSource;
    if (source instanceof Response) {
        wasmSource = await source.arrayBuffer();
    } else {
        wasmSource = source;
    }
    const asyncManager = new AsyncFutureManager();
    const streamManager = new StreamPollManager();
    const importObject = {
        env: {
            __boltffi_wake: (handle)=>asyncManager.wake(handle),
            __boltffi_stream_wake: (handle, result)=>streamManager.wake(handle, result),
            ...imports?.env ?? {}
        },
        __wbindgen_placeholder__: createImportModuleProxy("__wbindgen_placeholder__"),
        __wbindgen_externref_xform__: createImportModuleProxy("__wbindgen_externref_xform__")
    };
    const { instance } = await WebAssembly.instantiate(wasmSource, importObject);
    const module = new BoltFFIModule(instance, asyncManager, streamManager);
    const actualVersion = module.exports.boltffi_wasm_abi_version();
    if (actualVersion !== expectedVersion) {
        throw new Error(`BoltFFI ABI version mismatch: expected ${expectedVersion}, got ${actualVersion}`);
    }
    return module;
}
export function instantiateBoltFFISync(source, expectedVersion, imports) {
    const asyncManager = new AsyncFutureManager();
    const streamManager = new StreamPollManager();
    const importObject = {
        env: {
            __boltffi_wake: (handle)=>asyncManager.wake(handle),
            __boltffi_stream_wake: (handle, result)=>streamManager.wake(handle, result),
            ...imports?.env ?? {}
        },
        __wbindgen_placeholder__: createImportModuleProxy("__wbindgen_placeholder__"),
        __wbindgen_externref_xform__: createImportModuleProxy("__wbindgen_externref_xform__")
    };
    const wasmModule = new WebAssembly.Module(source);
    const instance = new WebAssembly.Instance(wasmModule, importObject);
    const module = new BoltFFIModule(instance, asyncManager, streamManager);
    const actualVersion = module.exports.boltffi_wasm_abi_version();
    if (actualVersion !== expectedVersion) {
        throw new Error(`BoltFFI ABI version mismatch: expected ${expectedVersion}, got ${actualVersion}`);
    }
    return module;
}
