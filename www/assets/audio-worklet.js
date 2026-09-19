// Runs on the audio thread. Consumes mono u8 PCM chunks posted from the
// main thread, converts them to f32, and resamples from the emulator's
// 22050 Hz output to the AudioContext's destination rate (usually 44.1k
// or 48k). Upsampling holds source samples to preserve classic effect
// edges; downsampling uses linear interpolation.
//
// If the main thread stalls and we run out of chunks, we output silence
// rather than crashing or playing stale samples — the audio thread itself
// never blocks on the main thread.

class SystemlessAudioProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.sourceRate = 22050;
    this.maxQueuedMs = 500;
    this.maxQueuedSourceSamples = Math.round(this.sourceRate * this.maxQueuedMs / 1000);
    this.chunks = [];
    this.head = 0;
    this.queuedSamples = 0;
    this.idx = 0;      // index within chunks[head]
    this.phase = 0.0;  // fractional position into source sample stream
    this.diagnosticsEnabled = false;
    this.diagnostics = {
      queuedChunks: 0,
      queuedSourceSamples: 0,
      processedBlocks: 0,
      processedOutputSamples: 0,
      nonSilentOutputSamples: 0,
    };
    this.port.onmessage = (e) => {
      if (e.data instanceof Float32Array && e.data.length > 0) {
        this.enqueueChunk(e.data);
      } else if (e.data instanceof Uint8Array && e.data.length > 0) {
        this.enqueueChunk(this.convertU8Pcm(e.data));
      } else if (e.data && e.data.type === 'clear') {
        this.clearQueue();
      } else if (e.data && e.data.type === 'diagnostics') {
        this.diagnosticsEnabled = e.data.enabled !== false;
        this.postDiagnostics('diagnostics');
      }
    };
  }

  enqueueChunk(chunk) {
    this.chunks.push(chunk);
    this.queuedSamples += chunk.length;
    this.diagnostics.queuedChunks++;
    this.diagnostics.queuedSourceSamples += chunk.length;
    this.trimQueue();
    this.postDiagnostics('enqueue');
  }

  convertU8Pcm(samples) {
    const chunk = new Float32Array(samples.length);
    for (let i = 0; i < samples.length; i++) {
      chunk[i] = (samples[i] - 128) / 128;
    }
    return chunk;
  }

  clearQueue() {
    this.chunks = [];
    this.head = 0;
    this.queuedSamples = 0;
    this.idx = 0;
    this.phase = 0.0;
    this.postDiagnostics('clear');
  }

  postDiagnostics(reason) {
    if (!this.diagnosticsEnabled) return;
    this.port.postMessage({
      type: 'diagnostics',
      reason,
      queuedSamples: this.queuedSamples,
      ...this.diagnostics,
    });
  }

  // Keep latency bounded if the main thread catches up after a stall or if
  // samples arrive while the AudioContext is not actively being consumed.
  trimQueue() {
    while (this.queuedSamples > this.maxQueuedSourceSamples && this.head < this.chunks.length) {
      const overflow = this.queuedSamples - this.maxQueuedSourceSamples;
      const remainingInFirstChunk = this.chunks[this.head].length - this.idx;
      if (overflow >= remainingInFirstChunk) {
        this.head++;
        this.queuedSamples -= remainingInFirstChunk;
        this.idx = 0;
      } else {
        this.idx += overflow;
        this.queuedSamples -= overflow;
      }
      this.phase = 0.0;
    }
    this.compactQueue();
  }

  compactQueue() {
    if (this.head === 0) return;
    if (this.head >= this.chunks.length) {
      this.chunks = [];
      this.head = 0;
    } else if (this.head >= 32) {
      this.chunks = this.chunks.slice(this.head);
      this.head = 0;
    }
  }

  process(inputs, outputs) {
    const output = outputs[0][0];
    // Destination-samples-per-source-sample is (dest / source); stepping
    // the source phase by (source / dest) each dest sample covers one
    // source sample every ratio dest samples.
    const step = this.sourceRate / sampleRate;
    const trackDiagnostics = this.diagnosticsEnabled;

    let nonSilentThisBlock = 0;
    for (let i = 0; i < output.length; i++) {
      if (this.head >= this.chunks.length) {
        output[i] = 0;
        continue;
      }
      const chunk = this.chunks[this.head];
      const a = chunk[this.idx];
      let sample = a;
      if (step >= 1.0) {
        const b = this.idx + 1 < chunk.length
          ? chunk[this.idx + 1]
          : (this.head + 1 < this.chunks.length ? this.chunks[this.head + 1][0] : a);
        sample = a + (b - a) * this.phase;
      }
      output[i] = sample;
      if (trackDiagnostics && Math.abs(output[i]) > 0.000001) {
        nonSilentThisBlock++;
      }
      this.phase += step;
      while (this.phase >= 1.0) {
        this.idx++;
        this.queuedSamples = Math.max(0, this.queuedSamples - 1);
        if (this.idx >= this.chunks[this.head].length) {
          this.head++;
          this.idx = 0;
        }
        this.phase -= 1.0;
        if (this.head >= this.chunks.length) {
          this.compactQueue();
          break;
        }
      }
    }
    if (trackDiagnostics) {
      this.diagnostics.processedBlocks++;
      this.diagnostics.processedOutputSamples += output.length;
      this.diagnostics.nonSilentOutputSamples += nonSilentThisBlock;
      if (nonSilentThisBlock > 0 || this.diagnostics.processedBlocks % 16 === 0) {
        this.postDiagnostics('process');
      }
    }

    // Keep processing forever.
    return true;
  }
}

registerProcessor('systemless-audio', SystemlessAudioProcessor);
