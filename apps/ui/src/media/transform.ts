export type EncodedFrame = {
  readonly counter: bigint;
  readonly kid: Uint8Array;
  readonly bytes: Uint8Array;
};

export type ClearEncodedFrame = { readonly bytes: Uint8Array };

export type RustMediaTransform = {
  protectEncoded(frame: ClearEncodedFrame): Promise<EncodedFrame>;
  openEncoded(frame: EncodedFrame): Promise<ClearEncodedFrame>;
};

export class KeylessEncodedTransform {
  readonly #rust: RustMediaTransform;

  constructor(rust: RustMediaTransform) {
    this.#rust = rust;
  }

  async protect(frame: ClearEncodedFrame): Promise<EncodedFrame> {
    return this.#rust.protectEncoded({ bytes: copyBytes(frame.bytes) });
  }

  async open(frame: EncodedFrame): Promise<ClearEncodedFrame> {
    return this.#rust.openEncoded({
      kid: copyBytes(frame.kid),
      counter: frame.counter,
      bytes: copyBytes(frame.bytes),
    });
  }
}

export function frameForRust(frame: EncodedFrame): EncodedFrame {
  return {
    kid: copyBytes(frame.kid),
    counter: frame.counter,
    bytes: copyBytes(frame.bytes),
  };
}

export function assertNoRawKeysInJs(_: never): never {
  throw new Error('SFrame keys stay in Rust; JS only passes encoded frames, KIDs, and counters.');
}

function copyBytes(bytes: Uint8Array): Uint8Array {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy;
}
