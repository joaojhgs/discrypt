export type EncodedFrame = { counter: bigint; kid: Uint8Array; bytes: Uint8Array };
export function assertNoRawKeysInJs(_: never): never { throw new Error('SFrame keys stay in Rust; JS only passes encoded frames.'); }
