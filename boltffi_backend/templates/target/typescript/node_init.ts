

const _wasmBytes = readFileSync(_wasmPath);
const _module: BoltFFIModule = instantiateBoltFFISync(_wasmBytes, 1, { env: _callbackImports });
const _exports: BoltFFIExports = _module.exports;
{{ constant_initializers }}

export const initialized = Promise.resolve();
export default function init(): Promise<void> { return initialized; }
