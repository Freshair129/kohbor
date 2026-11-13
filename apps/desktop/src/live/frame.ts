// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { FrameHeader, ParsedFrame } from "./types.ts";

/**
 * Host frame v1 header: magic:u32, version:u16, flags:u16, width:u16, height:u16,
 * stride:u32, session_epoch:u32, calibration_generation:u32, frame_seq:u32, age_ms:u32.
 * Every field is little-endian; the BGRA payload starts at byte 32.
 */
export const FRAME_MAGIC = 0x4b4d4150;
export const FRAME_VERSION = 1;
export const FRAME_HEADER_BYTES = 32;
const MAX_DIMENSION = 4096;

/** Returns null for every malformed, unsupported, or size-mismatched envelope. */
export function parseFrame(input: ArrayBuffer): ParsedFrame | null {
  if (!(input instanceof ArrayBuffer) || input.byteLength < FRAME_HEADER_BYTES) return null;
  const view = new DataView(input);
  if (view.getUint32(0, true) !== FRAME_MAGIC || view.getUint16(4, true) !== FRAME_VERSION) return null;
  const flags = view.getUint16(6, true);
  const width = view.getUint16(8, true);
  const height = view.getUint16(10, true);
  const stride = view.getUint32(12, true);
  const sessionEpoch = view.getUint32(16, true);
  const calibrationGeneration = view.getUint32(20, true);
  const frameSequence = view.getUint32(24, true);
  const ageMs = view.getUint32(28, true);
  if (
    width === 0 || height === 0 || width > MAX_DIMENSION || height > MAX_DIMENSION ||
    stride < width * 4
  ) return null;

  const payloadBytes = stride * height;
  if (!Number.isSafeInteger(payloadBytes) || input.byteLength !== FRAME_HEADER_BYTES + payloadBytes) return null;

  const header: FrameHeader = {
    flags,
    width,
    height,
    stride,
    session_epoch: sessionEpoch,
    calibration_generation: calibrationGeneration,
    frame_seq: frameSequence,
    age_ms: ageMs,
    byte_length: payloadBytes,
  };
  return { header, bgra: new Uint8Array(input, FRAME_HEADER_BYTES, payloadBytes) };
}

/** Convert each BGRA pixel to RGBA while reusing the supplied destination when sized correctly. */
export function bgraToRgba(
  frame: ParsedFrame,
  reuse?: Uint8ClampedArray<ArrayBuffer>,
): Uint8ClampedArray<ArrayBuffer> {
  const { width, height, stride } = frame.header;
  const length = width * height * 4;
  const rgba: Uint8ClampedArray<ArrayBuffer> = reuse?.length === length
    ? reuse
    : new Uint8ClampedArray(new ArrayBuffer(length));
  for (let y = 0; y < height; y += 1) {
    const sourceRow = y * stride;
    const targetRow = y * width * 4;
    for (let x = 0; x < width; x += 1) {
      const source = sourceRow + x * 4;
      const target = targetRow + x * 4;
      rgba[target] = frame.bgra[source + 2]!;
      rgba[target + 1] = frame.bgra[source + 1]!;
      rgba[target + 2] = frame.bgra[source]!;
      rgba[target + 3] = frame.bgra[source + 3]!;
    }
  }
  return rgba;
}

/** Synthetic fixture helper for tests and the dev-only mock; it does not create a media asset. */
export function createFrameBuffer(header: Omit<FrameHeader, "byte_length">, bgra: Uint8Array): ArrayBuffer {
  const payloadBytes = header.stride * header.height;
  if (bgra.byteLength !== payloadBytes) throw new RangeError("frame payload size does not match header");
  const buffer = new ArrayBuffer(FRAME_HEADER_BYTES + payloadBytes);
  const view = new DataView(buffer);
  view.setUint32(0, FRAME_MAGIC, true);
  view.setUint16(4, FRAME_VERSION, true);
  view.setUint16(6, header.flags, true);
  view.setUint16(8, header.width, true);
  view.setUint16(10, header.height, true);
  view.setUint32(12, header.stride, true);
  view.setUint32(16, header.session_epoch, true);
  view.setUint32(20, header.calibration_generation, true);
  view.setUint32(24, header.frame_seq, true);
  view.setUint32(28, header.age_ms, true);
  new Uint8Array(buffer, FRAME_HEADER_BYTES).set(bgra);
  return buffer;
}
