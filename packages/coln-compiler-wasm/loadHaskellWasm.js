import {
  WASI,
  OpenFile,
  File,
  ConsoleStdout,
} from "https://cdn.jsdelivr.net/npm/@bjorn3/browser_wasi_shim@0.3.0/dist/index.js";

/** Load a GHC-compiled WASI module in the browser. */
export default async function ({ wasmUrl, ghc_wasm_jsffi }) {
  const wasi = new WASI(
    [],
    [],
    [
      new OpenFile(new File([])),
      ConsoleStdout.lineBuffered(console.log),
      ConsoleStdout.lineBuffered(console.error),
    ],
  );

  const exports = {};
  const { instance } = await WebAssembly.instantiateStreaming(fetch(wasmUrl), {
    wasi_snapshot_preview1: wasi.wasiImport,
    ghc_wasm_jsffi: ghc_wasm_jsffi(exports),
  });
  Object.assign(exports, instance.exports);
  wasi.initialize(instance);
  return exports;
}
