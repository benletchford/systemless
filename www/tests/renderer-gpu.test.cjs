const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const context = vm.createContext({ Uint8Array, ArrayBuffer });
vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/renderer-gpu.js'), 'utf8')
  .replace('export const', 'const').replace('export function', 'function').replace('export class', 'class')
  + '\nthis.validate = validateGpuFrame;', context);
const frame = () => ({ kind: 'indexed8', complete: true, width: 3, height: 2, stride: 4,
  pixels: new Uint8Array(8), palette: new Uint8Array(1024) });

test('indexed images include full padded rows and a complete palette', () => {
  assert.equal(context.validate(frame(), 8192), 4);
  for (const changes of [{ stride: 2 }, { stride: 4.5 }, { pixels: new Uint8Array(7) },
    { palette: new Uint8Array(1023) }, { complete: false }, { kind: 'indexed4' }, { width: 0 }, { height: 8193 }]) {
    assert.throws(() => context.validate({ ...frame(), ...changes }, 8192));
  }
});

test('RGBA packets require exact complete dimensions and owned buffers', () => {
  const packet = { kind: 'rgba', complete: true, width: 3, height: 2, pixels: new Uint8Array(24) };
  assert.equal(context.validate(packet, 8192), 3);
  assert.throws(() => context.validate({ ...packet, pixels: new Uint8Array(23) }, 8192));
  assert.throws(() => context.validate({ ...packet, width: 3.5 }, 8192));
  assert.throws(() => context.validate({ ...packet, pixels: new Uint8Array(new SharedArrayBuffer(24)) }, 8192));
});
