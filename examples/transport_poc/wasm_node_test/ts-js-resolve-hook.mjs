import { pathToFileURL } from "node:url";

// Minimal loader hook for POC testing without a real package manager or
// `tsc` install in this environment:
//
// 1. Generated bindings import the published "@boltffi/runtime" package;
//    map that (and its subpaths) straight to the local runtime source.
// 2. TypeScript sources here import sibling modules with a `.js` extension
//    (the standard ESM+TS convention, since `tsc` doesn't rewrite
//    extensions). Node's native TS support strips types but does not
//    resolve `.js` specifiers to sibling `.ts` files, so we do that
//    resolution ourselves.
const RUNTIME_ROOT = pathToFileURL(
  new URL("../../../runtime/typescript/src/", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")
).href;

export async function resolve(specifier, context, nextResolve) {
  if (specifier === "@boltffi/runtime") {
    return nextResolve(new URL("index.ts", RUNTIME_ROOT).href, context);
  }
  if (specifier.startsWith("@boltffi/runtime/")) {
    return nextResolve(new URL(specifier.slice("@boltffi/runtime/".length), RUNTIME_ROOT).href, context);
  }
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    if (specifier.endsWith(".js")) {
      const tsSpecifier = specifier.slice(0, -3) + ".ts";
      return nextResolve(tsSpecifier, context);
    }
    throw error;
  }
}
