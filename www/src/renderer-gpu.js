// Pure presentation of complete owned images. Composition, cursor/retained
// detail preparation and guest-visible writes remain on the execution owner.
export const GPU_PRESENTER_PROTOCOL = 2;

export function validateGpuFrame(frame, maxTextureSize) {
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
        uniform vec2 imageSize; uniform float indexed;
        void main() {
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
    this.textures = [0, 1, 2].map(unit => {
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
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.disable(gl.DITHER);
    // Both samplers must be complete even when the RGBA branch is selected.
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, this.textures[1]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(1024));
    gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D, this.textures[2]);
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

  dispose() {
    for (const texture of this.textures) this.gl.deleteTexture(texture);
    this.gl.deleteBuffer(this.vertices); this.gl.deleteProgram(this.program);
  }
}
