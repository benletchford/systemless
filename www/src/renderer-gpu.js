// Pure presentation of complete owned images. Composition, cursor/retained
// detail preparation and guest-visible writes remain on the execution owner.
export const GPU_PRESENTER_PROTOCOL = 3;

// CompactPresentation cells/detail are semantic u32 values. Upload little-endian
// bytes explicitly on uncommon big-endian hosts; normal hosts can use a view.
export function compactBytes(words) {
  const nativeLittleEndian = new Uint8Array(new Uint32Array([0x01020304]).buffer)[0] === 4;
  if (nativeLittleEndian) return new Uint8Array(words.buffer, words.byteOffset, words.byteLength);
  const bytes = new Uint8Array(words.byteLength);
  const view = new DataView(bytes.buffer);
  for (let i = 0; i < words.length; i++) view.setUint32(i * 4, words[i], true);
  return bytes;
}

export function validateCompactFrame(frame, maxTextureSize) {
  const source = frame.compact;
  if (!source || frame.complete !== true || frame.cursor
      || !Number.isSafeInteger(frame.width) || !Number.isSafeInteger(frame.height)
      || !Number.isSafeInteger(source.width) || !Number.isSafeInteger(source.height)
      || source.width < 1 || source.height < 1 || frame.width < 1 || frame.height < 1
      || frame.width > maxTextureSize || frame.height > maxTextureSize
      || source.width * source.height > 16 * 1024 * 1024 || frame.width * frame.height > 16 * 1024 * 1024
      || !Number.isSafeInteger(source.scale) || source.scale < 1 || source.scale > 4
      || !(source.cells instanceof Uint32Array) || !(source.cells.buffer instanceof ArrayBuffer)
      || !(source.detail instanceof Uint32Array) || !(source.detail.buffer instanceof ArrayBuffer)
      || source.cells.length !== source.width * source.height || source.detail.length > 16 * 1024 * 1024) {
    throw new Error('Invalid complete CompactPresentation');
  }
  const outputScale = frame.width / source.width;
  if (!Number.isSafeInteger(outputScale) || outputScale < 1 || outputScale > 4
      || frame.height !== source.height * outputScale) throw new Error('Unsupported compact output scale');
  const tile = source.scale * source.scale;
  for (const cell of source.cells) {
    if (cell >>> 31 && (cell & 0x7fffffff) + tile > source.detail.length) throw new Error('Invalid compact detail reference');
  }
  return outputScale;
}

export function validateGpuFrame(frame, maxTextureSize) {
  if (frame.kind === "compact") { validateCompactFrame(frame, maxTextureSize); return frame.compact.width; }
  const { width, height, pixels, kind } = frame;
  if (frame.complete !== true || !Number.isSafeInteger(width) || !Number.isSafeInteger(height)
      || width < 1 || height < 1 || width > maxTextureSize || height > maxTextureSize
      || width * height > 16 * 1024 * 1024
      || !(pixels instanceof Uint8Array) || !(pixels.buffer instanceof ArrayBuffer)) {
    throw new Error('Invalid complete GPU image');
  }
  const cursor = frame.cursor;
  if (cursor && (!Number.isSafeInteger(cursor.x) || !Number.isSafeInteger(cursor.y)
      || !Number.isSafeInteger(cursor.width) || !Number.isSafeInteger(cursor.height)
      || cursor.x < 0 || cursor.y < 0 || cursor.width < 1 || cursor.height < 1
      || cursor.x + cursor.width > width || cursor.y + cursor.height > height
      || cursor.width * cursor.height > 4096 || !(cursor.pixels instanceof Uint8Array)
      || !(cursor.pixels.buffer instanceof ArrayBuffer) || cursor.pixels.byteLength !== cursor.width * cursor.height * 4)) {
    throw new Error('Invalid owned cursor patch');
  }
  if (kind === 'rgba') {
    if (pixels.byteLength !== width * height * 4) throw new Error('Invalid RGBA image length');
    return width;
  }
  if (kind !== 'indexed8' || !Number.isSafeInteger(frame.stride)
      || frame.stride < width || frame.stride > maxTextureSize
      || frame.stride * height > 16 * 1024 * 1024
      || pixels.byteLength !== frame.stride * height
      || !(frame.palette instanceof Uint8Array) || !(frame.palette.buffer instanceof ArrayBuffer)
      || frame.palette.byteLength !== 256 * 4) throw new Error('Invalid indexed image layout or palette');
  return frame.stride;
}

export class GpuFramePresenter {
  constructor(canvas) {
    const gl = canvas.getContext('webgl', { alpha: false, antialias: false, premultipliedAlpha: false });
    if (!gl) throw new Error('Offscreen WebGL unavailable');
    this.gl = gl;
    this.canvas = canvas;
    this.maxTextureSize = gl.getParameter(gl.MAX_TEXTURE_SIZE);
    const compile = (type, source) => {
      const shader = gl.createShader(type);
      gl.shaderSource(shader, source); gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
        const error = gl.getShaderInfoLog(shader); gl.deleteShader(shader); throw new Error(error);
      }
      return shader;
    };
    const vertex = compile(gl.VERTEX_SHADER, 'attribute vec2 position; void main() { gl_Position = vec4(position, 0.0, 1.0); }');
    let fragment;
    try {
      fragment = compile(gl.FRAGMENT_SHADER, `precision highp float;
        uniform sampler2D image; uniform sampler2D palette; uniform sampler2D cursor;
        uniform vec4 cursorRect;
        uniform sampler2D detail;
        uniform vec4 compactInfo;
        uniform vec2 logicalSize;
        uniform float compact;
        vec3 compactSample(float offset, vec2 sample) {
          float index = offset + sample.y * compactInfo.x + sample.x;
          vec2 pixel = vec2(mod(index, compactInfo.z), floor(index / compactInfo.z));
          return floor(texture2D(detail, (pixel + 0.5) / compactInfo.zw).bgr * 255.0 + 0.5);
        }
        vec3 compactColor(vec2 outputPixel) {
          vec2 cell = floor(outputPixel / compactInfo.y);
          vec4 encoded = floor(texture2D(image, (cell + 0.5) / logicalSize) * 255.0 + 0.5);
          if (encoded.a < 128.0) return encoded.bgr;
          // Validated detail offsets fit 24 bits, exactly representable in highp.
          float offset = encoded.r + encoded.g * 256.0 + encoded.b * 65536.0;
          vec2 within = floor(outputPixel) - cell * compactInfo.y;
          if (compactInfo.x <= compactInfo.y) {
            vec2 sample = min(floor((within * 2.0 + 1.0) * compactInfo.x / (compactInfo.y * 2.0)), vec2(compactInfo.x - 1.0));
            return compactSample(offset, sample);
          }
          vec3 sum = vec3(0.0);
          for (int y = 0; y < 4; y++) for (int x = 0; x < 4; x++) {
            if (float(x) < compactInfo.x && float(y) < compactInfo.x) {
              vec2 low = max(within * compactInfo.x, vec2(float(x), float(y)) * compactInfo.y);
              vec2 high = min((within + 1.0) * compactInfo.x, (vec2(float(x), float(y)) + 1.0) * compactInfo.y);
              vec2 weight = max(high - low, vec2(0.0));
              sum += compactSample(offset, vec2(float(x), float(y))) * weight.x * weight.y;
            }
          }
          float total = compactInfo.x * compactInfo.x;
          return floor((sum + floor(total / 2.0)) / total);
        }
        uniform vec2 imageSize; uniform float indexed;
        void main() {
          if (compact > 0.5) {
            gl_FragColor = vec4(compactColor(vec2(gl_FragCoord.x, imageSize.y - gl_FragCoord.y)) / 255.0, 1.0);
            return;
          }
          vec2 uv = vec2(gl_FragCoord.x / imageSize.x, 1.0 - gl_FragCoord.y / imageSize.y);
          vec4 value = texture2D(image, uv);
          vec4 color = indexed > 0.5 ? texture2D(palette, vec2((value.r * 255.0 + 0.5) / 256.0, 0.5)) : value;
          vec2 position = vec2(gl_FragCoord.x, imageSize.y - gl_FragCoord.y);
          vec2 local = position - cursorRect.xy;
          if (local.x >= 0.0 && local.y >= 0.0 && local.x < cursorRect.z && local.y < cursorRect.w)
            color = texture2D(cursor, local / cursorRect.zw);
          gl_FragColor = color;
        }`);
      this.program = gl.createProgram();
      gl.attachShader(this.program, vertex); gl.attachShader(this.program, fragment); gl.linkProgram(this.program);
      if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(this.program));
    } catch (error) {
      if (this.program) gl.deleteProgram(this.program);
      throw error;
    } finally {
      gl.deleteShader(vertex); if (fragment) gl.deleteShader(fragment);
    }
    gl.useProgram(this.program);
    this.vertices = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, this.vertices);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const position = gl.getAttribLocation(this.program, 'position');
    gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
    this.size = gl.getUniformLocation(this.program, 'imageSize');
    this.indexed = gl.getUniformLocation(this.program, 'indexed');
    this.cursorRect = gl.getUniformLocation(this.program, 'cursorRect');
    this.compact = gl.getUniformLocation(this.program, 'compact');
    this.compactInfo = gl.getUniformLocation(this.program, 'compactInfo');
    this.logicalSize = gl.getUniformLocation(this.program, 'logicalSize');
    this.textures = [0, 1, 2, 3].map(unit => {
      const texture = gl.createTexture(); gl.activeTexture(gl.TEXTURE0 + unit); gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      return texture;
    });
    gl.uniform1i(gl.getUniformLocation(this.program, 'image'), 0);
    gl.uniform1i(gl.getUniformLocation(this.program, 'palette'), 1);
    gl.uniform1i(gl.getUniformLocation(this.program, 'cursor'), 2);
    gl.uniform1i(gl.getUniformLocation(this.program, 'detail'), 3);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.disable(gl.DITHER);
    // Both samplers must be complete even when the RGBA branch is selected.
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, this.textures[1]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(1024));
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D, this.textures[2]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(4));
    gl.activeTexture(gl.TEXTURE3); gl.bindTexture(gl.TEXTURE_2D, this.textures[3]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(4));
  }

  paint(frame) {
    const stride = validateGpuFrame(frame, this.maxTextureSize);
    const gl = this.gl;
    if (gl.isContextLost()) throw new Error('Renderer WebGL context lost');
    if (this.canvas.width !== frame.width) this.canvas.width = frame.width;
    if (this.canvas.height !== frame.height) this.canvas.height = frame.height;
    gl.viewport(0, 0, frame.width, frame.height);
    gl.useProgram(this.program);
    gl.uniform2f(this.size, stride, frame.height);
    gl.uniform1f(this.indexed, frame.kind === 'indexed8' ? 1 : 0);
    gl.uniform1f(this.compact, frame.kind === 'compact' ? 1 : 0);
    if (frame.kind === 'compact') {
      this.paintCompact(frame);
      return;
    }
    if (frame.cursor) {
      const patch = frame.cursor;
      gl.uniform4f(this.cursorRect, patch.x, patch.y, patch.width, patch.height);
      gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D, this.textures[2]);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, patch.width, patch.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, patch.pixels);
    } else gl.uniform4f(this.cursorRect, 0, 0, 0, 0);
    if (frame.kind === 'indexed8') {
      gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, this.textures[1]);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, frame.palette);
    }
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, this.textures[0]);
    const format = frame.kind === 'indexed8' ? gl.LUMINANCE : gl.RGBA;
    gl.texImage2D(gl.TEXTURE_2D, 0, format, stride, frame.height, 0, format, gl.UNSIGNED_BYTE, frame.pixels);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  paintCompact(frame) {
    const gl = this.gl, source = frame.compact;
    const side = 2 ** Math.floor(Math.log2(this.maxTextureSize));
    const width = Math.min(side, Math.max(1, source.detail.length));
    const height = Math.max(1, Math.ceil(source.detail.length / width));
    if (height > this.maxTextureSize) throw new Error('Compact detail exceeds texture capacity');
    gl.uniform2f(this.size, source.width, frame.height);
    gl.uniform2f(this.logicalSize, source.width, source.height);
    gl.uniform4f(this.compactInfo, source.scale, frame.width / source.width, width, height);
    gl.activeTexture(gl.TEXTURE3); gl.bindTexture(gl.TEXTURE_2D, this.textures[3]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    const bytes = compactBytes(source.detail), rows = Math.floor(source.detail.length / width);
    if (rows) gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, rows, gl.RGBA, gl.UNSIGNED_BYTE, bytes.subarray(0, rows * width * 4));
    const tail = source.detail.length % width;
    if (tail) gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, rows, tail, 1, gl.RGBA, gl.UNSIGNED_BYTE, bytes.subarray(rows * width * 4));
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, this.textures[0]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, source.width, source.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, compactBytes(source.cells));
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  dispose() {
    for (const texture of this.textures) this.gl.deleteTexture(texture);
    this.gl.deleteBuffer(this.vertices); this.gl.deleteProgram(this.program);
  }
}
