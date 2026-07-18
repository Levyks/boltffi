const UTF8_DECODER = new TextDecoder("utf-8");
const UTF8_ENCODER = new TextEncoder();
export class WireReader {
    view;
    offset;
    constructor(buffer, offset = 0){
        this.view = new DataView(buffer);
        this.offset = offset;
    }
    readBool() {
        const value = this.view.getUint8(this.offset);
        this.offset += 1;
        return value !== 0;
    }
    skip(n) {
        this.offset += n;
    }
    readI8() {
        const value = this.view.getInt8(this.offset);
        this.offset += 1;
        return value;
    }
    readU8() {
        const value = this.view.getUint8(this.offset);
        this.offset += 1;
        return value;
    }
    readI16() {
        const value = this.view.getInt16(this.offset, true);
        this.offset += 2;
        return value;
    }
    readU16() {
        const value = this.view.getUint16(this.offset, true);
        this.offset += 2;
        return value;
    }
    readI32() {
        const value = this.view.getInt32(this.offset, true);
        this.offset += 4;
        return value;
    }
    readU32() {
        const value = this.view.getUint32(this.offset, true);
        this.offset += 4;
        return value;
    }
    readI64() {
        const value = this.view.getBigInt64(this.offset, true);
        this.offset += 8;
        return value;
    }
    readU64() {
        const value = this.view.getBigUint64(this.offset, true);
        this.offset += 8;
        return value;
    }
    readISize() {
        return this.readI32();
    }
    readUSize() {
        return this.readU32();
    }
    readF32() {
        const value = this.view.getFloat32(this.offset, true);
        this.offset += 4;
        return value;
    }
    readF64() {
        const value = this.view.getFloat64(this.offset, true);
        this.offset += 8;
        return value;
    }
    readString() {
        const len = this.readU32();
        const bytes = new Uint8Array(this.view.buffer, this.offset, len);
        this.offset += len;
        return UTF8_DECODER.decode(bytes);
    }
    readBytes() {
        const len = this.readU32();
        const bytes = new Uint8Array(this.view.buffer, this.offset, len);
        this.offset += len;
        return bytes.slice();
    }
    readI8Array() {
        const len = this.readU32();
        const result = new Int8Array(this.view.buffer, this.offset, len);
        this.offset += len;
        return result;
    }
    readU8Array() {
        const len = this.readU32();
        const result = new Uint8Array(this.view.buffer, this.offset, len);
        this.offset += len;
        return result;
    }
    readBoolArray() {
        const len = this.readU32();
        const values = new Uint8Array(this.view.buffer, this.offset, len);
        this.offset += len;
        return Array.from(values, (value)=>value !== 0);
    }
    readTypedArray(typedArray, len) {
        const byteOffset = this.offset;
        const byteLength = len * typedArray.BYTES_PER_ELEMENT;
        this.offset += byteLength;
        if (byteOffset % typedArray.BYTES_PER_ELEMENT === 0) {
            return new typedArray(this.view.buffer, byteOffset, len);
        }
        const copy = new Uint8Array(this.view.buffer, byteOffset, byteLength).slice().buffer;
        return new typedArray(copy);
    }
    readI16Array() {
        const len = this.readU32();
        return this.readTypedArray(Int16Array, len);
    }
    readU16Array() {
        const len = this.readU32();
        return this.readTypedArray(Uint16Array, len);
    }
    readI32Array() {
        const len = this.readU32();
        return this.readTypedArray(Int32Array, len);
    }
    readU32Array() {
        const len = this.readU32();
        return this.readTypedArray(Uint32Array, len);
    }
    readISizeArray() {
        return this.readI32Array();
    }
    readUSizeArray() {
        return this.readU32Array();
    }
    readI64Array() {
        const len = this.readU32();
        return this.readTypedArray(BigInt64Array, len);
    }
    readU64Array() {
        const len = this.readU32();
        return this.readTypedArray(BigUint64Array, len);
    }
    readF32Array() {
        const len = this.readU32();
        return this.readTypedArray(Float32Array, len);
    }
    readF64Array() {
        const len = this.readU32();
        return this.readTypedArray(Float64Array, len);
    }
    readOptional(readValue) {
        const tag = this.readU8();
        if (tag === 0) {
            return null;
        }
        return readValue();
    }
    readArray(readElement) {
        const len = this.readU32();
        const result = [];
        for(let i = 0; i < len; i++){
            result.push(readElement());
        }
        return result;
    }
    readMap(readKey, readValue) {
        const len = this.readU32();
        const result = new Map();
        let index = 0;
        while(index < len){
            result.set(readKey(), readValue());
            index += 1;
        }
        return result;
    }
    readResult(readOk, readErr) {
        const tag = this.readU8();
        if (tag === 0) {
            return readOk();
        }
        throw readErr();
    }
    readDuration() {
        const secs = this.readU64();
        const nanos = this.readU32();
        return {
            secs,
            nanos
        };
    }
    readTimestamp() {
        const secs = this.readI64();
        const nanos = this.readU32();
        const ms = Number(secs) * 1000 + Math.floor(nanos / 1000000);
        return new Date(ms);
    }
    readUuid() {
        const hi = this.readU64();
        const lo = this.readU64();
        const hiHex = hi.toString(16).padStart(16, "0");
        const loHex = lo.toString(16).padStart(16, "0");
        const hex = hiHex + loHex;
        return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
    }
    readUrl() {
        return this.readString();
    }
}
export function wireOk(value) {
    return {
        tag: "ok",
        value
    };
}
export function wireErr(error) {
    return {
        tag: "err",
        error
    };
}
export function matchWireResult(value, ok, err) {
    if (typeof value === "object" && value !== null && "tag" in value && value.tag === "ok" && "value" in value) {
        return ok(value.value);
    }
    if (typeof value === "object" && value !== null && "tag" in value && value.tag === "err" && "error" in value) {
        return err(value.error);
    }
    if (value instanceof Error) {
        return err(value);
    }
    if (typeof value === "object" && value !== null) {
        throw new Error("Ambiguous Result object. Pass wireOk(value) or wireErr(error) for object payloads.");
    }
    return ok(value);
}
export class WireWriter {
    localBuffer;
    localView;
    wasmAllocator;
    wasmPtr;
    allocationSize;
    offset;
    cachedWasmView;
    cachedWasmBuffer;
    constructor(initialSize = 256){
        const normalizedSize = Math.max(initialSize, 1);
        this.localBuffer = new ArrayBuffer(normalizedSize);
        this.localView = new DataView(this.localBuffer);
        this.wasmAllocator = null;
        this.wasmPtr = 0;
        this.allocationSize = normalizedSize;
        this.offset = 0;
        this.cachedWasmView = null;
        this.cachedWasmBuffer = null;
    }
    static withWasmAllocation(initialSize, allocator) {
        const normalizedSize = Math.max(initialSize, 1);
        const pointer = allocator.alloc(normalizedSize);
        if (pointer === 0 && normalizedSize > 0) {
            throw new Error("Failed to allocate memory for writer");
        }
        const writer = new WireWriter(1);
        writer.wasmAllocator = allocator;
        writer.wasmPtr = pointer;
        writer.allocationSize = normalizedSize;
        return writer;
    }
    static withWasmRegion(pointer, size, buffer) {
        const writer = new WireWriter(1);
        writer.wasmAllocator = {
            alloc: ()=>pointer,
            realloc: ()=>{
                throw new Error("Fixed WASM region exceeded its capacity");
            },
            free: ()=>{},
            buffer
        };
        writer.wasmPtr = pointer;
        writer.allocationSize = size;
        return writer;
    }
    release() {
        if (this.wasmAllocator !== null && this.wasmPtr !== 0 && this.allocationSize !== 0) {
            this.wasmAllocator.free(this.wasmPtr, this.allocationSize);
            this.wasmPtr = 0;
            this.allocationSize = 0;
            this.offset = 0;
        }
    }
    reset() {
        if (this.allocationSize === 0) {
            throw new Error("Cannot reset a released WireWriter");
        }
        this.offset = 0;
    }
    get capacity() {
        return this.allocationSize;
    }
    inWasmMemory() {
        return this.wasmAllocator !== null;
    }
    currentBuffer() {
        return this.inWasmMemory() ? this.wasmAllocator.buffer() : this.localBuffer;
    }
    currentView() {
        if (!this.inWasmMemory()) {
            return this.localView;
        }
        const buffer = this.wasmAllocator.buffer();
        if (this.cachedWasmBuffer !== buffer) {
            this.cachedWasmBuffer = buffer;
            this.cachedWasmView = new DataView(buffer);
        }
        return this.cachedWasmView;
    }
    writePosition() {
        return this.inWasmMemory() ? this.wasmPtr + this.offset : this.offset;
    }
    ensureCapacity(additionalBytes) {
        if (this.allocationSize === 0) {
            throw new Error("Cannot write using a released WireWriter");
        }
        const required = this.offset + additionalBytes;
        if (required <= this.allocationSize) {
            return;
        }
        let newSize = this.allocationSize;
        while(newSize < required){
            newSize *= 2;
        }
        if (this.inWasmMemory()) {
            const newPointer = this.wasmAllocator.realloc(this.wasmPtr, this.allocationSize, newSize);
            if (newPointer === 0 && newSize > 0) {
                throw new Error("Failed to reallocate memory for writer");
            }
            this.wasmPtr = newPointer;
            this.allocationSize = newSize;
            return;
        }
        const newBuffer = new ArrayBuffer(newSize);
        new Uint8Array(newBuffer).set(new Uint8Array(this.localBuffer));
        this.localBuffer = newBuffer;
        this.localView = new DataView(this.localBuffer);
        this.allocationSize = newSize;
    }
    get ptr() {
        return this.wasmPtr;
    }
    get len() {
        return this.offset;
    }
    getBytes() {
        const start = this.inWasmMemory() ? this.wasmPtr : 0;
        return new Uint8Array(this.currentBuffer(), start, this.offset).slice();
    }
    writeBool(value) {
        this.ensureCapacity(1);
        this.currentView().setUint8(this.writePosition(), value ? 1 : 0);
        this.offset += 1;
    }
    skip(n) {
        this.ensureCapacity(n);
        const view = this.currentView();
        const pos = this.writePosition();
        for(let i = 0; i < n; i++){
            view.setUint8(pos + i, 0);
        }
        this.offset += n;
    }
    writeI8(value) {
        this.ensureCapacity(1);
        this.currentView().setInt8(this.writePosition(), value);
        this.offset += 1;
    }
    writeU8(value) {
        this.ensureCapacity(1);
        this.currentView().setUint8(this.writePosition(), value);
        this.offset += 1;
    }
    writeI16(value) {
        this.ensureCapacity(2);
        this.currentView().setInt16(this.writePosition(), value, true);
        this.offset += 2;
    }
    writeU16(value) {
        this.ensureCapacity(2);
        this.currentView().setUint16(this.writePosition(), value, true);
        this.offset += 2;
    }
    writeI32(value) {
        this.ensureCapacity(4);
        this.currentView().setInt32(this.writePosition(), value, true);
        this.offset += 4;
    }
    writeU32(value) {
        this.ensureCapacity(4);
        this.currentView().setUint32(this.writePosition(), value, true);
        this.offset += 4;
    }
    writeI64(value) {
        this.ensureCapacity(8);
        this.currentView().setBigInt64(this.writePosition(), value, true);
        this.offset += 8;
    }
    writeU64(value) {
        this.ensureCapacity(8);
        this.currentView().setBigUint64(this.writePosition(), value, true);
        this.offset += 8;
    }
    writeISize(value) {
        this.writeI32(value);
    }
    writeUSize(value) {
        this.writeU32(value);
    }
    writeF32(value) {
        this.ensureCapacity(4);
        this.currentView().setFloat32(this.writePosition(), value, true);
        this.offset += 4;
    }
    writeF64(value) {
        this.ensureCapacity(8);
        this.currentView().setFloat64(this.writePosition(), value, true);
        this.offset += 8;
    }
    writeString(value) {
        const encoded = UTF8_ENCODER.encode(value);
        this.writeU32(encoded.length);
        this.ensureCapacity(encoded.length);
        new Uint8Array(this.currentBuffer()).set(encoded, this.writePosition());
        this.offset += encoded.length;
    }
    writeBytes(value) {
        this.writeU32(value.length);
        this.ensureCapacity(value.length);
        new Uint8Array(this.currentBuffer()).set(value, this.writePosition());
        this.offset += value.length;
    }
    writeOptional(value, writeValue) {
        if (value === null) {
            this.writeU8(0);
        } else {
            this.writeU8(1);
            writeValue(value);
        }
    }
    writeArray(values, writeElement) {
        this.writeU32(values.length);
        for (const v of values){
            writeElement(v);
        }
    }
    writeMap(values, writeKey, writeValue) {
        this.writeU32(values.size);
        values.forEach((value, key)=>{
            writeKey(key);
            writeValue(value);
        });
    }
    writeResult(value, writeOk, writeErr) {
        matchWireResult(value, (ok)=>{
            this.writeU8(0);
            writeOk(ok);
        }, (err)=>{
            this.writeU8(1);
            writeErr(err);
        });
    }
    writeDuration(value) {
        this.writeU64(value.secs);
        this.writeU32(value.nanos);
    }
    writeTimestamp(value) {
        const ms = value.getTime();
        const wholeSeconds = Math.floor(ms / 1000);
        const secs = BigInt(wholeSeconds);
        const nanos = (ms - wholeSeconds * 1000) * 1000000;
        this.writeI64(secs);
        this.writeU32(nanos);
    }
    writeUuid(value) {
        const hex = value.replace(/-/g, "");
        const hi = BigInt("0x" + hex.slice(0, 16));
        const lo = BigInt("0x" + hex.slice(16, 32));
        this.writeU64(hi);
        this.writeU64(lo);
    }
    writeUrl(value) {
        this.writeString(value);
    }
}
export function wireStringSize(value) {
    return 4 + UTF8_ENCODER.encode(value).length;
}
export function utf8ByteCount(value) {
    return UTF8_ENCODER.encode(value).length;
}
export function wireOptionalSize(value, size) {
    return value === null ? 1 : 1 + size(value);
}
export function wireArraySize(values, size) {
    return values.reduce((bytes, value)=>bytes + size(value), 4);
}
export function wireMapSize(values, keySize, valueSize) {
    let bytes = 4;
    values.forEach((value, key)=>{
        bytes += keySize(key) + valueSize(value);
    });
    return bytes;
}
export function wireResultSize(value, ok, err) {
    return 1 + matchWireResult(value, ok, err);
}
