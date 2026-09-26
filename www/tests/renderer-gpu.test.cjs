const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const context = vm.createContext({ Uint8Array, Uint32Array, ArrayBuffer });
vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/renderer-gpu.js'), 'utf8')
  .replace('export const', 'const').replaceAll('export function', 'function').replace('export class', 'class')
  + '\nthis.validate = validateGpuFrame; this.bytes = compactBytes;', context);
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


test('cursor patches must be complete owned rectangles inside the same screen', () => {
  const cursor = { x: 1, y: 0, width: 2, height: 1, pixels: new Uint8Array(8) };
  assert.equal(context.validate({ ...frame(), cursor }, 8192), 4);
  for (const changes of [{ x: -1 }, { x: 2 }, { y: 2 }, { width: 0 }, { pixels: new Uint8Array(7) }]) {
    assert.throws(() => context.validate({ ...frame(), cursor: { ...cursor, ...changes } }, 8192));
  }
});


test('compact packets validate native cell/detail references and integer output scale', () => {
  const compact = {width:2,height:1,scale:2,cells:new Uint32Array([0x123456,0x80000000]),detail:new Uint32Array(4)};
  const packet = {kind:'compact',complete:true,width:6,height:3,compact};
  assert.equal(context.validate(packet,8192),2);
  for(const changes of [{scale:0},{cells:new Uint32Array(1)},{detail:new Uint32Array(3)},
    {cells:new Uint32Array([0,0x80000001])},{detail:new Uint32Array(new SharedArrayBuffer(16))}]) {
    assert.throws(()=>context.validate({...packet,compact:{...compact,...changes}},8192));
  }
  assert.throws(()=>context.validate({...packet,width:5},8192));
  assert.throws(()=>context.validate({...packet,cursor:{}},8192));
});

test('compact words upload in explicit little-endian order including typed-array offsets', () => {
  const words=new Uint32Array([0xdeadbeef,0x80123456,0x00abcdef,0xdeadbeef]);
  assert.deepEqual([...context.bytes(words.subarray(1,3))],[0x56,0x34,0x12,0x80,0xef,0xcd,0xab,0]);
});
