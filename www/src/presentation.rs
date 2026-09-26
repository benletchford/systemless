//! Browser presentation backends consuming owned pixel or QD3D packets.
//! Input, display demand, layout, runtime lifecycle and guest execution belong
//! to the screen/controller. Context creation is the only DOM-facing boundary.

use js_sys::{Array, Float32Array, Reflect, Uint8Array, Uint8ClampedArray};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, ImageData, WebGlBuffer, WebGlProgram,
    WebGlRenderingContext, WebGlShader, WebGlTexture,
};

pub(crate) enum CanvasFrame {
    WebGl(WebGlFrame),
    Canvas2d(Canvas2dFrame),
    Offscreen(OffscreenFrame),
}

impl CanvasFrame {
    pub(crate) fn new_worker(
        canvas: &HtmlCanvasElement,
        width: u32,
        height: u32,
        generation: u32,
    ) -> Option<Self> {
        let user_agent = web_sys::window()
            .and_then(|window| window.navigator().user_agent().ok())
            .unwrap_or_default();
        if !requires_canvas_2d_presenter(&user_agent) {
            if let Ok(handle) = crate::renderer_bridge::create_renderer(canvas, generation) {
                if !handle.is_null() {
                    return Some(Self::Offscreen(OffscreenFrame {
                        canvas: canvas.clone(),
                        handle,
                        fallback: None,
                        needs_snapshot: false,
                        painted: false,
                        fatal: None,
                    }));
                }
            }
        }
        Self::new(canvas, width, height)
    }

    pub(crate) fn poll_recovery(&mut self) -> Result<(), String> {
        if let Self::Offscreen(frame) = self {
            frame.poll_recovery()
        } else {
            Ok(())
        }
    }

    pub(crate) fn needs_snapshot(&self) -> bool {
        matches!(self, Self::Offscreen(frame) if frame.needs_snapshot)
    }

    pub(crate) fn pending(&self) -> bool {
        matches!(self, Self::Offscreen(frame) if frame.pending())
    }

    pub(crate) fn asynchronous(&self) -> bool {
        matches!(self, Self::Offscreen(frame) if frame.fallback.is_none() && frame.fatal.is_none())
    }

    pub(crate) fn submitted(&self) -> bool {
        matches!(self, Self::Offscreen(frame) if frame.submitted())
    }

    pub(crate) fn new(canvas: &HtmlCanvasElement, width: u32, height: u32) -> Option<Self> {
        let user_agent = web_sys::window()
            .and_then(|window| window.navigator().user_agent().ok())
            .unwrap_or_default();
        if !requires_canvas_2d_presenter(&user_agent) {
            if let Some(gl) = canvas
                .get_context("webgl")
                .ok()
                .flatten()
                .and_then(|context| context.dyn_into::<WebGlRenderingContext>().ok())
            {
                return WebGlFrame::new(gl).map(Self::WebGl);
            }
        }

        let context = canvas
            .get_context("2d")
            .ok()
            .flatten()?
            .dyn_into::<CanvasRenderingContext2d>()
            .ok()?;
        Canvas2dFrame::new(context, width, height).map(Self::Canvas2d)
    }

    pub(crate) fn paint(&mut self, width: u32, height: u32, rgba: &[u8]) {
        match self {
            Self::WebGl(frame) => frame.paint(width, height, rgba),
            Self::Canvas2d(frame) => frame.paint(width, height, rgba),
            Self::Offscreen(frame) => frame.paint_js(width, height, &Uint8Array::from(rgba)),
        }
    }

    pub(crate) fn paint_js(&mut self, width: u32, height: u32, rgba: &Uint8Array) {
        match self {
            Self::WebGl(frame) => frame.paint_js(width, height, rgba),
            Self::Canvas2d(frame) => frame.paint_js(width, height, rgba),
            Self::Offscreen(frame) => frame.paint_js(width, height, rgba),
        }
    }

    pub(crate) fn supports_q3_gpu(&self) -> bool {
        matches!(self, Self::WebGl(_))
    }

    pub(crate) fn paint_q3(&mut self, packet: &JsValue) -> Option<(u32, u32)> {
        match self {
            Self::WebGl(frame) => frame.paint_q3(packet),
            Self::Canvas2d(_) | Self::Offscreen(_) => None,
        }
    }

    pub(crate) fn backend_name(&self) -> &'static str {
        match self {
            Self::WebGl(_) => "webgl",
            Self::Canvas2d(_) => "canvas2d",
            Self::Offscreen(frame) if frame.fallback.is_some() => "canvas2d-renderer-fallback",
            Self::Offscreen(_) => "offscreen-canvas2d",
        }
    }
}

pub(crate) struct OffscreenFrame {
    canvas: HtmlCanvasElement,
    handle: JsValue,
    fallback: Option<Canvas2dFrame>,
    needs_snapshot: bool,
    painted: bool,
    fatal: Option<String>,
}

impl OffscreenFrame {
    fn poll_recovery(&mut self) -> Result<(), String> {
        if let Some(error) = self.fatal.as_ref() {
            return Err(error.clone());
        }
        if self.fallback.is_some() {
            return Ok(());
        }
        let status = crate::renderer_bridge::renderer_status(&self.handle);
        if Reflect::get(&status, &JsValue::from_str("phase"))
            .ok()
            .and_then(|v| v.as_string())
            .as_deref()
            != Some("failed")
        {
            return Ok(());
        }
        let error = Reflect::get(&status, &JsValue::from_str("error"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "Renderer failed".into());
        let _ = self.canvas.set_attribute("data-render-fallback", &error);
        let recovery = crate::renderer_bridge::take_recovery(&self.handle);
        crate::renderer_bridge::dispose_renderer(&self.handle);
        // The failed worker owned a separate canvas. The original input canvas
        // has never acquired a context, so Canvas2D can take over without losing
        // listeners, focus, pointer capture or the running guest.
        self.fallback = self
            .canvas
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|context| context.dyn_into::<CanvasRenderingContext2d>().ok())
            .and_then(|context| {
                Canvas2dFrame::new(context, self.canvas.width(), self.canvas.height())
            });
        if self.fallback.is_none() {
            let message = format!("{error}; unable to initialize fallback display");
            self.fatal = Some(message.clone());
            return Err(message);
        }
        self.needs_snapshot = true;
        if !recovery.is_null() {
            if let (Some(width), Some(height), Ok(pixels)) = (
                js_number_property(&recovery, "width"),
                js_number_property(&recovery, "height"),
                Reflect::get(&recovery, &JsValue::from_str("pixels")),
            ) {
                if let Ok(pixels) = pixels.dyn_into::<Uint8Array>() {
                    self.paint_js(width as u32, height as u32, &pixels);
                }
            }
        }
        Ok(())
    }

    fn paint_js(&mut self, width: u32, height: u32, pixels: &Uint8Array) {
        if self.fatal.is_some() {
            return;
        }
        if let Some(fallback) = self.fallback.as_mut() {
            fallback.paint_js(width, height, pixels);
            self.needs_snapshot = false;
            self.painted = true;
        } else {
            crate::renderer_bridge::paint_renderer(&self.handle, width, height, pixels);
        }
    }

    fn submitted(&self) -> bool {
        self.painted
            || (self.fallback.is_none()
                && self.fatal.is_none()
                && js_bool_property(
                    &crate::renderer_bridge::renderer_status(&self.handle),
                    "submitted",
                )
                .unwrap_or(false))
    }

    fn pending(&self) -> bool {
        self.fallback.is_none()
            && self.fatal.is_none()
            && js_bool_property(
                &crate::renderer_bridge::renderer_status(&self.handle),
                "pending",
            )
            .unwrap_or(false)
    }
}

impl Drop for OffscreenFrame {
    fn drop(&mut self) {
        crate::renderer_bridge::dispose_renderer(&self.handle);
    }
}

fn requires_canvas_2d_presenter(user_agent: &str) -> bool {
    // WebKit's WebGL texture uploads can corrupt rapid indexed-palette
    // transitions. Canvas2D consumes the same RGBA frame without remapping it.
    let ios_webkit = ["iPhone", "iPad", "iPod"]
        .iter()
        .any(|marker| user_agent.contains(marker));
    let desktop_safari = user_agent.contains("Safari/")
        && ![
            "Chrome/",
            "Chromium/",
            "CriOS/",
            "Edg/",
            "EdgiOS/",
            "OPR/",
            "FxiOS/",
        ]
        .iter()
        .any(|marker| user_agent.contains(marker));
    ios_webkit || desktop_safari
}

pub(crate) struct Canvas2dFrame {
    context: CanvasRenderingContext2d,
    width: u32,
    height: u32,
    image_data: ImageData,
    pixels: Uint8ClampedArray,
}

impl Canvas2dFrame {
    fn new(context: CanvasRenderingContext2d, width: u32, height: u32) -> Option<Self> {
        let image_data = context
            .create_image_data_with_sw_and_sh(width.max(1) as f64, height.max(1) as f64)
            .ok()?;
        let pixels = image_data_pixels(&image_data)?;
        Some(Self {
            context,
            width,
            height,
            image_data,
            pixels,
        })
    }

    fn paint(&mut self, width: u32, height: u32, rgba: &[u8]) {
        if self.width != width || self.height != height {
            let Some(next) = Self::new(self.context.clone(), width, height) else {
                return;
            };
            *self = next;
        }
        if self.pixels.length() as usize != rgba.len() {
            return;
        }
        self.pixels.copy_from(rgba);
        let _ = self.context.put_image_data(&self.image_data, 0.0, 0.0);
    }

    fn paint_js(&mut self, width: u32, height: u32, rgba: &Uint8Array) {
        self.paint(width, height, &rgba.to_vec());
    }
}

pub(crate) struct WebGlFrame {
    gl: WebGlRenderingContext,
    frame_program: WebGlProgram,
    frame_vertices: WebGlBuffer,
    frame_texture: WebGlTexture,
    frame_position: u32,
    frame_texture_coordinate: u32,
    q3_program: WebGlProgram,
    q3_vertices: WebGlBuffer,
    q3_screen_position: u32,
    q3_texture_coordinate: u32,
    q3_color: u32,
    width: u32,
    height: u32,
}

impl WebGlFrame {
    fn new(gl: WebGlRenderingContext) -> Option<Self> {
        let vertex_shader = compile_webgl_shader(
            &gl,
            WebGlRenderingContext::VERTEX_SHADER,
            r#"attribute vec2 position;
attribute vec2 texture_coordinate;
varying vec2 texture_position;
void main() {
    texture_position = texture_coordinate;
    gl_Position = vec4(position, 0.0, 1.0);
}"#,
        )?;
        let fragment_shader = compile_webgl_shader(
            &gl,
            WebGlRenderingContext::FRAGMENT_SHADER,
            r#"precision mediump float;
varying vec2 texture_position;
uniform sampler2D frame_texture;
void main() {
    gl_FragColor = texture2D(frame_texture, texture_position);
}"#,
        )?;
        let program = link_webgl_program(&gl, &vertex_shader, &fragment_shader)?;
        let vertices = gl.create_buffer()?;
        let texture = gl.create_texture()?;
        gl.use_program(Some(&program));
        gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&vertices));
        let vertex_data = Float32Array::from(webgl_frame_vertices().as_slice());
        gl.buffer_data_with_array_buffer_view(
            WebGlRenderingContext::ARRAY_BUFFER,
            vertex_data.as_ref(),
            WebGlRenderingContext::STATIC_DRAW,
        );
        let mut frame_locations = Vec::new();
        for (name, offset) in [("position", 0), ("texture_coordinate", 2 * 4)] {
            let location = gl.get_attrib_location(&program, name);
            if location < 0 {
                return None;
            }
            let location = location as u32;
            gl.enable_vertex_attrib_array(location);
            gl.vertex_attrib_pointer_with_i32(
                location,
                2,
                WebGlRenderingContext::FLOAT,
                false,
                4 * 4,
                offset,
            );
            frame_locations.push(location);
        }
        gl.active_texture(WebGlRenderingContext::TEXTURE0);
        gl.bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(&texture));
        gl.tex_parameteri(
            WebGlRenderingContext::TEXTURE_2D,
            WebGlRenderingContext::TEXTURE_MIN_FILTER,
            WebGlRenderingContext::NEAREST as i32,
        );
        gl.tex_parameteri(
            WebGlRenderingContext::TEXTURE_2D,
            WebGlRenderingContext::TEXTURE_MAG_FILTER,
            WebGlRenderingContext::NEAREST as i32,
        );
        gl.tex_parameteri(
            WebGlRenderingContext::TEXTURE_2D,
            WebGlRenderingContext::TEXTURE_WRAP_S,
            WebGlRenderingContext::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameteri(
            WebGlRenderingContext::TEXTURE_2D,
            WebGlRenderingContext::TEXTURE_WRAP_T,
            WebGlRenderingContext::CLAMP_TO_EDGE as i32,
        );
        gl.uniform1i(
            gl.get_uniform_location(&program, "frame_texture").as_ref(),
            0,
        );
        gl.pixel_storei(WebGlRenderingContext::UNPACK_ALIGNMENT, 1);
        gl.pixel_storei(WebGlRenderingContext::UNPACK_FLIP_Y_WEBGL, 1);

        let q3_vertex_shader = compile_webgl_shader(
            &gl,
            WebGlRenderingContext::VERTEX_SHADER,
            r#"attribute vec4 screen_position;
attribute vec2 texture_coordinate;
attribute vec4 vertex_color;
uniform vec2 target_size;
varying vec2 texture_position;
varying vec4 fragment_color;
void main() {
    float reciprocal_w = max(abs(screen_position.w), 0.000001);
    float clip_w = 1.0 / reciprocal_w;
    vec2 ndc = vec2(
        screen_position.x / target_size.x * 2.0 - 1.0,
        1.0 - screen_position.y / target_size.y * 2.0
    );
    gl_Position = vec4(ndc * clip_w, screen_position.z * clip_w, clip_w);
    texture_position = texture_coordinate;
    fragment_color = vertex_color;
}"#,
        )?;
        let q3_fragment_shader = compile_webgl_shader(
            &gl,
            WebGlRenderingContext::FRAGMENT_SHADER,
            r#"precision mediump float;
varying vec2 texture_position;
varying vec4 fragment_color;
uniform sampler2D frame_texture;
uniform bool use_texture;
void main() {
    vec4 texel = use_texture ? texture2D(frame_texture, texture_position) : vec4(1.0);
    gl_FragColor = fragment_color * texel;
}"#,
        )?;
        let q3_program = link_webgl_program(&gl, &q3_vertex_shader, &q3_fragment_shader)?;
        let q3_vertices = gl.create_buffer()?;
        let q3_screen_position = gl.get_attrib_location(&q3_program, "screen_position");
        let q3_texture_coordinate = gl.get_attrib_location(&q3_program, "texture_coordinate");
        let q3_color = gl.get_attrib_location(&q3_program, "vertex_color");
        if q3_screen_position < 0 || q3_texture_coordinate < 0 || q3_color < 0 {
            return None;
        }

        Some(Self {
            gl,
            frame_program: program,
            frame_vertices: vertices,
            frame_texture: texture,
            frame_position: frame_locations[0],
            frame_texture_coordinate: frame_locations[1],
            q3_program,
            q3_vertices,
            q3_screen_position: q3_screen_position as u32,
            q3_texture_coordinate: q3_texture_coordinate as u32,
            q3_color: q3_color as u32,
            width: 0,
            height: 0,
        })
    }

    fn paint(&mut self, width: u32, height: u32, rgba: &[u8]) {
        let width = width.max(1);
        let height = height.max(1);
        if rgba.len() != width as usize * height as usize * 4 {
            return;
        }
        self.gl.use_program(Some(&self.frame_program));
        self.gl.bind_buffer(
            WebGlRenderingContext::ARRAY_BUFFER,
            Some(&self.frame_vertices),
        );
        for (location, offset) in [
            (self.frame_position, 0),
            (self.frame_texture_coordinate, 2 * 4),
        ] {
            self.gl.enable_vertex_attrib_array(location);
            self.gl.vertex_attrib_pointer_with_i32(
                location,
                2,
                WebGlRenderingContext::FLOAT,
                false,
                4 * 4,
                offset,
            );
        }
        self.gl.active_texture(WebGlRenderingContext::TEXTURE0);
        self.gl
            .bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(&self.frame_texture));
        self.gl.disable(WebGlRenderingContext::DEPTH_TEST);
        self.gl.disable(WebGlRenderingContext::BLEND);
        let result = if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.gl
                .tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                    WebGlRenderingContext::TEXTURE_2D,
                    0,
                    WebGlRenderingContext::RGBA as i32,
                    width as i32,
                    height as i32,
                    0,
                    WebGlRenderingContext::RGBA,
                    WebGlRenderingContext::UNSIGNED_BYTE,
                    Some(rgba),
                )
        } else {
            self.gl
                .tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                    WebGlRenderingContext::TEXTURE_2D,
                    0,
                    0,
                    0,
                    width as i32,
                    height as i32,
                    WebGlRenderingContext::RGBA,
                    WebGlRenderingContext::UNSIGNED_BYTE,
                    Some(rgba),
                )
        };
        if result.is_err() {
            return;
        }
        self.gl.viewport(0, 0, width as i32, height as i32);
        self.gl
            .draw_arrays(WebGlRenderingContext::TRIANGLE_STRIP, 0, 4);
    }

    fn paint_js(&mut self, width: u32, height: u32, rgba: &Uint8Array) {
        let width = width.max(1);
        let height = height.max(1);
        if rgba.length() as usize != width as usize * height as usize * 4 {
            return;
        }
        self.gl.use_program(Some(&self.frame_program));
        self.gl.bind_buffer(
            WebGlRenderingContext::ARRAY_BUFFER,
            Some(&self.frame_vertices),
        );
        for (location, offset) in [
            (self.frame_position, 0),
            (self.frame_texture_coordinate, 2 * 4),
        ] {
            self.gl.enable_vertex_attrib_array(location);
            self.gl.vertex_attrib_pointer_with_i32(
                location,
                2,
                WebGlRenderingContext::FLOAT,
                false,
                4 * 4,
                offset,
            );
        }
        self.gl.active_texture(WebGlRenderingContext::TEXTURE0);
        self.gl
            .bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(&self.frame_texture));
        self.gl.disable(WebGlRenderingContext::DEPTH_TEST);
        self.gl.disable(WebGlRenderingContext::BLEND);
        let result = if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.gl
                .tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_js_u8_array(
                    WebGlRenderingContext::TEXTURE_2D,
                    0,
                    WebGlRenderingContext::RGBA as i32,
                    width as i32,
                    height as i32,
                    0,
                    WebGlRenderingContext::RGBA,
                    WebGlRenderingContext::UNSIGNED_BYTE,
                    Some(rgba),
                )
        } else {
            self.gl
                .tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_js_u8_array(
                    WebGlRenderingContext::TEXTURE_2D,
                    0,
                    0,
                    0,
                    width as i32,
                    height as i32,
                    WebGlRenderingContext::RGBA,
                    WebGlRenderingContext::UNSIGNED_BYTE,
                    Some(rgba),
                )
        };
        if result.is_err() {
            return;
        }
        self.gl.viewport(0, 0, width as i32, height as i32);
        self.gl
            .draw_arrays(WebGlRenderingContext::TRIANGLE_STRIP, 0, 4);
    }

    fn paint_q3(&mut self, packet: &JsValue) -> Option<(u32, u32)> {
        let width = js_number_property(packet, "width")? as u32;
        let height = js_number_property(packet, "height")? as u32;
        if width == 0 || height == 0 {
            return None;
        }
        self.width = width;
        self.height = height;
        self.gl.use_program(Some(&self.q3_program));
        self.gl
            .bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.q3_vertices));
        for (location, size, offset) in [
            (self.q3_screen_position, 4, 0),
            (self.q3_texture_coordinate, 2, 4 * 4),
            (self.q3_color, 4, 6 * 4),
        ] {
            self.gl.enable_vertex_attrib_array(location);
            self.gl.vertex_attrib_pointer_with_i32(
                location,
                size,
                WebGlRenderingContext::FLOAT,
                false,
                10 * 4,
                offset,
            );
        }
        self.gl.uniform2f(
            self.gl
                .get_uniform_location(&self.q3_program, "target_size")
                .as_ref(),
            width as f32,
            height as f32,
        );
        self.gl.uniform1i(
            self.gl
                .get_uniform_location(&self.q3_program, "frame_texture")
                .as_ref(),
            0,
        );
        self.gl.viewport(0, 0, width as i32, height as i32);
        self.gl.enable(WebGlRenderingContext::SCISSOR_TEST);
        if let Ok(viewport) = Reflect::get(packet, &JsValue::from_str("viewport")) {
            let viewport = Float32Array::new(&viewport);
            if viewport.length() == 4 {
                let left = viewport.get_index(0) as i32;
                let top = viewport.get_index(1) as i32;
                let right = viewport.get_index(2) as i32;
                let bottom = viewport.get_index(3) as i32;
                self.gl.scissor(
                    left,
                    height as i32 - bottom - 1,
                    right - left + 1,
                    bottom - top + 1,
                );
            }
        }
        self.gl.enable(WebGlRenderingContext::DEPTH_TEST);
        self.gl.depth_func(WebGlRenderingContext::LEQUAL);
        self.gl.depth_mask(true);
        self.gl.clear_depth(1.0);
        self.gl.clear(WebGlRenderingContext::DEPTH_BUFFER_BIT);
        if let Ok(clear) = Reflect::get(packet, &JsValue::from_str("clearColor")) {
            if !clear.is_undefined() {
                let clear = Float32Array::new(&clear);
                if clear.length() == 4 {
                    self.gl.clear_color(
                        clear.get_index(0),
                        clear.get_index(1),
                        clear.get_index(2),
                        clear.get_index(3),
                    );
                    self.gl.clear(WebGlRenderingContext::COLOR_BUFFER_BIT);
                }
            }
        }

        let texture_values =
            Array::from(&Reflect::get(packet, &JsValue::from_str("textures")).ok()?);
        let mut textures = Vec::with_capacity(texture_values.length() as usize);
        for texture_value in texture_values.iter() {
            let texture = self.gl.create_texture()?;
            self.gl.active_texture(WebGlRenderingContext::TEXTURE0);
            self.gl
                .bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(&texture));
            let texture_width = js_number_property(&texture_value, "width")? as i32;
            let texture_height = js_number_property(&texture_value, "height")? as i32;
            let rgba =
                Uint8Array::new(&Reflect::get(&texture_value, &JsValue::from_str("rgba")).ok()?)
                    .to_vec();
            self.gl
                .tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                    WebGlRenderingContext::TEXTURE_2D,
                    0,
                    WebGlRenderingContext::RGBA as i32,
                    texture_width,
                    texture_height,
                    0,
                    WebGlRenderingContext::RGBA,
                    WebGlRenderingContext::UNSIGNED_BYTE,
                    Some(&rgba),
                )
                .ok()?;
            for parameter in [
                WebGlRenderingContext::TEXTURE_MIN_FILTER,
                WebGlRenderingContext::TEXTURE_MAG_FILTER,
            ] {
                self.gl.tex_parameteri(
                    WebGlRenderingContext::TEXTURE_2D,
                    parameter,
                    WebGlRenderingContext::NEAREST as i32,
                );
            }
            for (parameter, wrap) in [
                (
                    WebGlRenderingContext::TEXTURE_WRAP_S,
                    js_bool_property(&texture_value, "wrapU").unwrap_or(false),
                ),
                (
                    WebGlRenderingContext::TEXTURE_WRAP_T,
                    js_bool_property(&texture_value, "wrapV").unwrap_or(false),
                ),
            ] {
                self.gl.tex_parameteri(
                    WebGlRenderingContext::TEXTURE_2D,
                    parameter,
                    if wrap {
                        WebGlRenderingContext::REPEAT as i32
                    } else {
                        WebGlRenderingContext::CLAMP_TO_EDGE as i32
                    },
                );
            }
            textures.push(texture);
        }

        let draws = Array::from(&Reflect::get(packet, &JsValue::from_str("draws")).ok()?);
        for draw in draws.iter() {
            let vertices =
                Float32Array::new(&Reflect::get(&draw, &JsValue::from_str("vertices")).ok()?);
            if vertices.length() == 0 || vertices.length() % 10 != 0 {
                continue;
            }
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ARRAY_BUFFER,
                vertices.as_ref(),
                WebGlRenderingContext::STREAM_DRAW,
            );
            let texture_index = js_number_property(&draw, "texture").unwrap_or(-1.0) as i32;
            let texture = usize::try_from(texture_index)
                .ok()
                .and_then(|index| textures.get(index));
            self.gl.active_texture(WebGlRenderingContext::TEXTURE0);
            self.gl
                .bind_texture(WebGlRenderingContext::TEXTURE_2D, texture);
            self.gl.uniform1i(
                self.gl
                    .get_uniform_location(&self.q3_program, "use_texture")
                    .as_ref(),
                i32::from(texture.is_some()),
            );
            if js_bool_property(&draw, "blend").unwrap_or(false) {
                self.gl.enable(WebGlRenderingContext::BLEND);
                self.gl.blend_func(
                    WebGlRenderingContext::SRC_ALPHA,
                    WebGlRenderingContext::ONE_MINUS_SRC_ALPHA,
                );
            } else {
                self.gl.disable(WebGlRenderingContext::BLEND);
            }
            self.gl
                .depth_mask(js_bool_property(&draw, "writeDepth").unwrap_or(true));
            self.gl.draw_arrays(
                WebGlRenderingContext::TRIANGLES,
                0,
                (vertices.length() / 10) as i32,
            );
        }
        for texture in textures {
            self.gl.delete_texture(Some(&texture));
        }
        self.gl.disable(WebGlRenderingContext::SCISSOR_TEST);
        self.gl.depth_mask(true);
        Some((width, height))
    }
}

fn webgl_frame_vertices() -> [f32; 16] {
    [
        -1.0, -1.0, 0.0, 0.0, 1.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    ]
}

fn compile_webgl_shader(
    gl: &WebGlRenderingContext,
    shader_type: u32,
    source: &str,
) -> Option<WebGlShader> {
    let shader = gl.create_shader(shader_type)?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    gl.get_shader_parameter(&shader, WebGlRenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
        .then_some(shader)
}

fn link_webgl_program(
    gl: &WebGlRenderingContext,
    vertex_shader: &WebGlShader,
    fragment_shader: &WebGlShader,
) -> Option<WebGlProgram> {
    let program = gl.create_program()?;
    gl.attach_shader(&program, vertex_shader);
    gl.attach_shader(&program, fragment_shader);
    gl.link_program(&program);
    gl.get_program_parameter(&program, WebGlRenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
        .then_some(program)
}

fn image_data_pixels(image_data: &ImageData) -> Option<Uint8ClampedArray> {
    Reflect::get(image_data.as_ref(), &JsValue::from_str("data"))
        .ok()?
        .dyn_into::<Uint8ClampedArray>()
        .ok()
}

fn js_bool_property(value: &JsValue, name: &str) -> Option<bool> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()?
        .as_bool()
}

fn js_number_property(value: &JsValue, name: &str) -> Option<f64> {
    Reflect::get(value, &JsValue::from_str(name)).ok()?.as_f64()
}

#[cfg(test)]
mod tests {
    #[test]
    fn webgl_frame_quad_preserves_full_rgba_texture_coordinates() {
        assert_eq!(
            super::webgl_frame_vertices(),
            [-1.0, -1.0, 0.0, 0.0, 1.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,]
        );
    }

    #[test]
    fn safari_and_ios_use_canvas_2d_for_stable_palette_presentation() {
        let safari = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                      (KHTML, like Gecko) Version/18.6 Safari/605.1.15";
        let ios_chrome = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) \
                          AppleWebKit/605.1.15 CriOS/138.0 Mobile/15E148 Safari/604.1";
        assert!(super::requires_canvas_2d_presenter(safari));
        assert!(super::requires_canvas_2d_presenter(ios_chrome));
    }

    #[test]
    fn chromium_and_firefox_keep_webgl_presentation() {
        let chrome = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                      (KHTML, like Gecko) Chrome/138.0 Safari/537.36";
        let firefox = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:140.0) \
                       Gecko/20100101 Firefox/140.0";
        assert!(!super::requires_canvas_2d_presenter(chrome));
        assert!(!super::requires_canvas_2d_presenter(firefox));
    }
}
