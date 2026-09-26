const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function processor() {
  let Processor;
  const context = vm.createContext({
    sampleRate: 22050, Float32Array, Uint8Array,
    AudioWorkletProcessor: class { constructor() { this.port = { postMessage() {} }; } },
    registerProcessor(_name, type) { Processor = type; },
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../assets/audio-worklet.js'), 'utf8'), context);
  const p = new Processor();
  p.port.onmessage({ data: { type: 'diagnostics', enabled: true } });
  return p;
}

function render(p, length = 8) {
  const output = new Float32Array(length);
  p.process([], [[output]]);
  return Array.from(output);
}

test('underruns exclude startup, intentional PCM silence, and cleared playback', () => {
  const p = processor();
  assert.deepEqual(render(p), Array(8).fill(0));
  assert.equal(p.diagnostics.underrunBlocks, 0);
  p.port.onmessage({ data: new Uint8Array([128, 128, 128, 128]) });
  assert.deepEqual(render(p), Array(8).fill(0));
  assert.equal(p.diagnostics.underrunBlocks, 1);
  assert.equal(p.diagnostics.underrunOutputSamples, 4);
  p.port.onmessage({ data: { type: 'clear' } });
  render(p);
  assert.equal(p.diagnostics.underrunOutputSamples, 4);
});

test('diagnostics leave PCM output unchanged and can be disabled', () => {
  const p = processor();
  p.port.onmessage({ data: new Uint8Array([0, 64, 128, 192]) });
  assert.deepEqual(render(p, 4), [-1, -0.5, 0, 0.5]);
  assert.equal(p.diagnostics.underrunBlocks, 0);
  p.port.onmessage({ data: { type: 'diagnostics', enabled: false } });
  render(p);
  assert.equal(p.diagnostics.underrunOutputSamples, 0);
});
