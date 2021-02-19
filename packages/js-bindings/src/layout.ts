import * as BufferLayout from 'buffer-layout';

/**
 * Layout for a public key
 */
export const publicKey = (property: string) => BufferLayout.blob(32, property);

/**
 * Layout for a 64bit unsigned value
 */
export const uint64 = (property: string) => BufferLayout.blob(8, property);