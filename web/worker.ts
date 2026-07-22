/**
 * Profiling worker.
 *
 * The file is read and profiled here so a 100 MB CSV never blocks the main
 * thread; the tab stays responsive while Rust chews through the bytes.
 */

import init, { profile, version } from "./pkg/mirante_wasm.js";
import type { Profile, WorkerRequest, WorkerResponse } from "./types.js";

const post = (message: WorkerResponse) => self.postMessage(message);

const ready = init().then(() => post({ type: "ready", version: version() }));

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  if (event.data.type !== "profile") return;
  const { file } = event.data;

  try {
    await ready;

    const readStart = performance.now();
    const buffer = await file.arrayBuffer();
    const readMs = performance.now() - readStart;

    // Timed as one unit on purpose: the copy into Wasm memory is a real cost
    // of this approach and hiding it would flatter the benchmark.
    const profileStart = performance.now();
    const result = profile(new Uint8Array(buffer)) as Profile;
    const profileMs = performance.now() - profileStart;

    post({
      type: "result",
      profile: result,
      timings: { readMs, profileMs },
      fileName: file.name,
      fileSize: file.size,
    });
  } catch (error) {
    post({ type: "error", message: error instanceof Error ? error.message : String(error) });
  }
};
