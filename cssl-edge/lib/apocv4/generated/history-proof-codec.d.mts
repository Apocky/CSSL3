export const wasmSha256: string;
export const wasmBase64: string;
export function createHistoryCodecInstance(module: WebAssembly.Module): { verify(input: Uint8Array): string };
