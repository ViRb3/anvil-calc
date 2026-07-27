import init, { process_wasm } from "./anvil_calc.js";

const wasmReady = init();

self.addEventListener("message", async ({ data: { id, input } }) => {
  try {
    await wasmReady;
    self.postMessage({ id, result: process_wasm(input) });
  } catch (error) {
    self.postMessage({
      id,
      error:
        error instanceof Error
          ? error.message
          : "The calculation could not be completed.",
    });
  }
});
