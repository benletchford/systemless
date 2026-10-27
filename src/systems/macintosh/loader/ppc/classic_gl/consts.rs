//! OpenGL 1.2 enumerants used by the classic Mac OS OpenGL library.
//!
//! Values are those of the OpenGL 1.2.1 specification and the extension
//! registry (ARB_multitexture, EXT_compiled_vertex_array, EXT_blend_color,
//! ARB_point_parameters, ARB_texture_env_add/combine, EXT_bgra and
//! APPLE_packed_pixels), which Apple's OpenGL 1.2 for Mac OS 9 shipped.
//! <https://registry.khronos.org/OpenGL/specs/gl/glspec121.pdf>

#![allow(dead_code)]

// Errors.
pub const NO_ERROR: u32 = 0;
pub const INVALID_ENUM: u32 = 0x0500;
pub const INVALID_VALUE: u32 = 0x0501;
pub const INVALID_OPERATION: u32 = 0x0502;
pub const STACK_OVERFLOW: u32 = 0x0503;
pub const STACK_UNDERFLOW: u32 = 0x0504;
pub const OUT_OF_MEMORY: u32 = 0x0505;

// Primitives.
pub const POINTS: u32 = 0x0000;
pub const LINES: u32 = 0x0001;
pub const LINE_LOOP: u32 = 0x0002;
pub const LINE_STRIP: u32 = 0x0003;
pub const TRIANGLES: u32 = 0x0004;
pub const TRIANGLE_STRIP: u32 = 0x0005;
pub const TRIANGLE_FAN: u32 = 0x0006;
pub const QUADS: u32 = 0x0007;
pub const QUAD_STRIP: u32 = 0x0008;
pub const POLYGON: u32 = 0x0009;

// Comparison functions.
pub const NEVER: u32 = 0x0200;
pub const LESS: u32 = 0x0201;
pub const EQUAL: u32 = 0x0202;
pub const LEQUAL: u32 = 0x0203;
pub const GREATER: u32 = 0x0204;
pub const NOTEQUAL: u32 = 0x0205;
pub const GEQUAL: u32 = 0x0206;
pub const ALWAYS: u32 = 0x0207;

// Blending factors.
pub const ZERO: u32 = 0;
pub const ONE: u32 = 1;
pub const SRC_COLOR: u32 = 0x0300;
pub const ONE_MINUS_SRC_COLOR: u32 = 0x0301;
pub const SRC_ALPHA: u32 = 0x0302;
pub const ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
pub const DST_ALPHA: u32 = 0x0304;
pub const ONE_MINUS_DST_ALPHA: u32 = 0x0305;
pub const DST_COLOR: u32 = 0x0306;
pub const ONE_MINUS_DST_COLOR: u32 = 0x0307;
pub const SRC_ALPHA_SATURATE: u32 = 0x0308;
pub const CONSTANT_COLOR: u32 = 0x8001;
pub const ONE_MINUS_CONSTANT_COLOR: u32 = 0x8002;
pub const CONSTANT_ALPHA: u32 = 0x8003;
pub const ONE_MINUS_CONSTANT_ALPHA: u32 = 0x8004;

// Buffers.
pub const NONE: u32 = 0;
pub const FRONT_LEFT: u32 = 0x0400;
pub const FRONT_RIGHT: u32 = 0x0401;
pub const BACK_LEFT: u32 = 0x0402;
pub const BACK_RIGHT: u32 = 0x0403;
pub const FRONT: u32 = 0x0404;
pub const BACK: u32 = 0x0405;
pub const LEFT: u32 = 0x0406;
pub const RIGHT: u32 = 0x0407;
pub const FRONT_AND_BACK: u32 = 0x0408;
pub const AUX0: u32 = 0x0409;

// Faces and winding.
pub const CW: u32 = 0x0900;
pub const CCW: u32 = 0x0901;

// Capabilities (glEnable).
pub const POINT_SMOOTH: u32 = 0x0B10;
pub const LINE_SMOOTH: u32 = 0x0B20;
pub const LINE_STIPPLE: u32 = 0x0B24;
pub const POLYGON_SMOOTH: u32 = 0x0B41;
pub const POLYGON_STIPPLE: u32 = 0x0B42;
pub const CULL_FACE: u32 = 0x0B44;
pub const LIGHTING: u32 = 0x0B50;
pub const COLOR_MATERIAL: u32 = 0x0B57;
pub const FOG: u32 = 0x0B60;
pub const DEPTH_TEST: u32 = 0x0B71;
pub const STENCIL_TEST: u32 = 0x0B90;
pub const NORMALIZE: u32 = 0x0BA1;
pub const ALPHA_TEST: u32 = 0x0BC0;
pub const DITHER: u32 = 0x0BD0;
pub const BLEND: u32 = 0x0BE2;
pub const INDEX_LOGIC_OP: u32 = 0x0BF1;
pub const COLOR_LOGIC_OP: u32 = 0x0BF2;
pub const SCISSOR_TEST: u32 = 0x0C11;
pub const TEXTURE_GEN_S: u32 = 0x0C60;
pub const TEXTURE_GEN_T: u32 = 0x0C61;
pub const TEXTURE_GEN_R: u32 = 0x0C62;
pub const TEXTURE_GEN_Q: u32 = 0x0C63;
pub const AUTO_NORMAL: u32 = 0x0D80;
pub const TEXTURE_1D: u32 = 0x0DE0;
pub const TEXTURE_2D: u32 = 0x0DE1;
pub const POLYGON_OFFSET_POINT: u32 = 0x2A01;
pub const POLYGON_OFFSET_LINE: u32 = 0x2A02;
pub const POLYGON_OFFSET_FILL: u32 = 0x8037;
pub const RESCALE_NORMAL: u32 = 0x803A;
pub const CLIP_PLANE0: u32 = 0x3000;
pub const LIGHT0: u32 = 0x4000;
pub const TEXTURE_3D: u32 = 0x806F;

// Client arrays.
pub const VERTEX_ARRAY: u32 = 0x8074;
pub const NORMAL_ARRAY: u32 = 0x8075;
pub const COLOR_ARRAY: u32 = 0x8076;
pub const INDEX_ARRAY: u32 = 0x8077;
pub const TEXTURE_COORD_ARRAY: u32 = 0x8078;
pub const EDGE_FLAG_ARRAY: u32 = 0x8079;

// Data types.
pub const BYTE: u32 = 0x1400;
pub const UNSIGNED_BYTE: u32 = 0x1401;
pub const SHORT: u32 = 0x1402;
pub const UNSIGNED_SHORT: u32 = 0x1403;
pub const INT: u32 = 0x1404;
pub const UNSIGNED_INT: u32 = 0x1405;
pub const FLOAT: u32 = 0x1406;
pub const DOUBLE: u32 = 0x140A;
pub const BITMAP: u32 = 0x1A00;
pub const UNSIGNED_BYTE_3_3_2: u32 = 0x8032;
pub const UNSIGNED_SHORT_4_4_4_4: u32 = 0x8033;
pub const UNSIGNED_SHORT_5_5_5_1: u32 = 0x8034;
pub const UNSIGNED_INT_8_8_8_8: u32 = 0x8035;
pub const UNSIGNED_INT_10_10_10_2: u32 = 0x8036;
pub const UNSIGNED_BYTE_2_3_3_REV: u32 = 0x8362;
pub const UNSIGNED_SHORT_5_6_5: u32 = 0x8363;
pub const UNSIGNED_SHORT_5_6_5_REV: u32 = 0x8364;
pub const UNSIGNED_SHORT_4_4_4_4_REV: u32 = 0x8365;
pub const UNSIGNED_SHORT_1_5_5_5_REV: u32 = 0x8366;
pub const UNSIGNED_INT_8_8_8_8_REV: u32 = 0x8367;
pub const UNSIGNED_INT_2_10_10_10_REV: u32 = 0x8368;

// Pixel formats.
pub const COLOR_INDEX: u32 = 0x1900;
pub const STENCIL_INDEX: u32 = 0x1901;
pub const DEPTH_COMPONENT: u32 = 0x1902;
pub const RED: u32 = 0x1903;
pub const GREEN: u32 = 0x1904;
pub const BLUE: u32 = 0x1905;
pub const ALPHA: u32 = 0x1906;
pub const RGB: u32 = 0x1907;
pub const RGBA: u32 = 0x1908;
pub const LUMINANCE: u32 = 0x1909;
pub const LUMINANCE_ALPHA: u32 = 0x190A;
pub const BGR: u32 = 0x80E0;
pub const BGRA: u32 = 0x80E1;
pub const INTENSITY: u32 = 0x8049;

// Sized internal formats (all stored as RGBA8 by this implementation).
pub const ALPHA4: u32 = 0x803B;
pub const ALPHA8: u32 = 0x803C;
pub const LUMINANCE4: u32 = 0x803F;
pub const LUMINANCE8: u32 = 0x8040;
pub const LUMINANCE4_ALPHA4: u32 = 0x8043;
pub const LUMINANCE8_ALPHA8: u32 = 0x8045;
pub const INTENSITY4: u32 = 0x804A;
pub const INTENSITY8: u32 = 0x804B;
pub const R3_G3_B2: u32 = 0x2A10;
pub const RGB4: u32 = 0x804F;
pub const RGB5: u32 = 0x8050;
pub const RGB8: u32 = 0x8051;
pub const RGBA2: u32 = 0x8055;
pub const RGBA4: u32 = 0x8056;
pub const RGB5_A1: u32 = 0x8057;
pub const RGBA8: u32 = 0x8058;

// Matrix modes.
pub const MODELVIEW: u32 = 0x1700;
pub const PROJECTION: u32 = 0x1701;
pub const TEXTURE: u32 = 0x1702;
pub const COLOR: u32 = 0x1800;
pub const DEPTH: u32 = 0x1801;
pub const STENCIL: u32 = 0x1802;

// Shading and polygon modes.
pub const FLAT: u32 = 0x1D00;
pub const SMOOTH: u32 = 0x1D01;
pub const POINT: u32 = 0x1B00;
pub const LINE: u32 = 0x1B01;
pub const FILL: u32 = 0x1B02;

// Stencil operations.
pub const KEEP: u32 = 0x1E00;
pub const REPLACE: u32 = 0x1E01;
pub const INCR: u32 = 0x1E02;
pub const DECR: u32 = 0x1E03;
pub const INVERT: u32 = 0x150A;

// Strings.
pub const VENDOR: u32 = 0x1F00;
pub const RENDERER: u32 = 0x1F01;
pub const VERSION: u32 = 0x1F02;
pub const EXTENSIONS: u32 = 0x1F03;

// Hints.
pub const DONT_CARE: u32 = 0x1100;
pub const FASTEST: u32 = 0x1101;
pub const NICEST: u32 = 0x1102;
pub const PERSPECTIVE_CORRECTION_HINT: u32 = 0x0C50;
pub const POINT_SMOOTH_HINT: u32 = 0x0C51;
pub const LINE_SMOOTH_HINT: u32 = 0x0C52;
pub const POLYGON_SMOOTH_HINT: u32 = 0x0C53;
pub const FOG_HINT: u32 = 0x0C54;
pub const TRANSFORM_HINT_APPLE: u32 = 0x85B1;

// Lighting.
pub const AMBIENT: u32 = 0x1200;
pub const DIFFUSE: u32 = 0x1201;
pub const SPECULAR: u32 = 0x1202;
pub const POSITION: u32 = 0x1203;
pub const SPOT_DIRECTION: u32 = 0x1204;
pub const SPOT_EXPONENT: u32 = 0x1205;
pub const SPOT_CUTOFF: u32 = 0x1206;
pub const CONSTANT_ATTENUATION: u32 = 0x1207;
pub const LINEAR_ATTENUATION: u32 = 0x1208;
pub const QUADRATIC_ATTENUATION: u32 = 0x1209;
pub const EMISSION: u32 = 0x1600;
pub const SHININESS: u32 = 0x1601;
pub const AMBIENT_AND_DIFFUSE: u32 = 0x1602;
pub const LIGHT_MODEL_LOCAL_VIEWER: u32 = 0x0B51;
pub const LIGHT_MODEL_TWO_SIDE: u32 = 0x0B52;
pub const LIGHT_MODEL_AMBIENT: u32 = 0x0B53;
pub const LIGHT_MODEL_COLOR_CONTROL: u32 = 0x81F8;
pub const SINGLE_COLOR: u32 = 0x81F9;
pub const SEPARATE_SPECULAR_COLOR: u32 = 0x81FA;

// Fog.
pub const FOG_INDEX: u32 = 0x0B61;
pub const FOG_DENSITY: u32 = 0x0B62;
pub const FOG_START: u32 = 0x0B63;
pub const FOG_END: u32 = 0x0B64;
pub const FOG_MODE: u32 = 0x0B65;
pub const FOG_COLOR: u32 = 0x0B66;
pub const EXP: u32 = 0x0800;
pub const EXP2: u32 = 0x0801;
pub const LINEAR: u32 = 0x2601;

// Textures.
pub const NEAREST: u32 = 0x2600;
pub const NEAREST_MIPMAP_NEAREST: u32 = 0x2700;
pub const LINEAR_MIPMAP_NEAREST: u32 = 0x2701;
pub const NEAREST_MIPMAP_LINEAR: u32 = 0x2702;
pub const LINEAR_MIPMAP_LINEAR: u32 = 0x2703;
pub const TEXTURE_MAG_FILTER: u32 = 0x2800;
pub const TEXTURE_MIN_FILTER: u32 = 0x2801;
pub const TEXTURE_WRAP_S: u32 = 0x2802;
pub const TEXTURE_WRAP_T: u32 = 0x2803;
pub const TEXTURE_WRAP_R: u32 = 0x8072;
pub const TEXTURE_BORDER_COLOR: u32 = 0x1004;
pub const TEXTURE_PRIORITY: u32 = 0x8066;
pub const TEXTURE_MIN_LOD: u32 = 0x813A;
pub const TEXTURE_MAX_LOD: u32 = 0x813B;
pub const TEXTURE_BASE_LEVEL: u32 = 0x813C;
pub const TEXTURE_MAX_LEVEL: u32 = 0x813D;
pub const TEXTURE_MAX_ANISOTROPY_EXT: u32 = 0x84FE;
pub const CLAMP: u32 = 0x2900;
pub const REPEAT: u32 = 0x2901;
pub const CLAMP_TO_EDGE: u32 = 0x812F;
pub const TEXTURE_ENV: u32 = 0x2300;
pub const TEXTURE_ENV_MODE: u32 = 0x2200;
pub const TEXTURE_ENV_COLOR: u32 = 0x2201;
pub const MODULATE: u32 = 0x2100;
pub const DECAL: u32 = 0x2101;
pub const ADD: u32 = 0x0104;
pub const COMBINE: u32 = 0x8570;
pub const COMBINE_RGB: u32 = 0x8571;
pub const COMBINE_ALPHA: u32 = 0x8572;
pub const RGB_SCALE: u32 = 0x8573;
pub const ADD_SIGNED: u32 = 0x8574;
pub const INTERPOLATE: u32 = 0x8575;
pub const CONSTANT: u32 = 0x8576;
pub const PRIMARY_COLOR: u32 = 0x8577;
pub const PREVIOUS: u32 = 0x8578;
pub const SOURCE0_RGB: u32 = 0x8580;
pub const SOURCE1_RGB: u32 = 0x8581;
pub const SOURCE2_RGB: u32 = 0x8582;
pub const SOURCE0_ALPHA: u32 = 0x8588;
pub const SOURCE1_ALPHA: u32 = 0x8589;
pub const SOURCE2_ALPHA: u32 = 0x858A;
pub const OPERAND0_RGB: u32 = 0x8590;
pub const OPERAND1_RGB: u32 = 0x8591;
pub const OPERAND2_RGB: u32 = 0x8592;
pub const OPERAND0_ALPHA: u32 = 0x8598;
pub const OPERAND1_ALPHA: u32 = 0x8599;
pub const OPERAND2_ALPHA: u32 = 0x859A;
pub const SUBTRACT: u32 = 0x84E7;
pub const TEXTURE0: u32 = 0x84C0;
pub const ACTIVE_TEXTURE: u32 = 0x84E0;
pub const CLIENT_ACTIVE_TEXTURE: u32 = 0x84E1;
pub const MAX_TEXTURE_UNITS: u32 = 0x84E2;
pub const TEXTURE_BINDING_2D: u32 = 0x8069;

// Texture coordinate generation.
pub const S: u32 = 0x2000;
pub const T: u32 = 0x2001;
pub const R: u32 = 0x2002;
pub const Q: u32 = 0x2003;
pub const TEXTURE_GEN_MODE: u32 = 0x2500;
pub const OBJECT_PLANE: u32 = 0x2501;
pub const EYE_PLANE: u32 = 0x2502;
pub const EYE_LINEAR: u32 = 0x2400;
pub const OBJECT_LINEAR: u32 = 0x2401;
pub const SPHERE_MAP: u32 = 0x2402;

// Pixel storage and transfer.
pub const UNPACK_SWAP_BYTES: u32 = 0x0CF0;
pub const UNPACK_LSB_FIRST: u32 = 0x0CF1;
pub const UNPACK_ROW_LENGTH: u32 = 0x0CF2;
pub const UNPACK_SKIP_ROWS: u32 = 0x0CF3;
pub const UNPACK_SKIP_PIXELS: u32 = 0x0CF4;
pub const UNPACK_ALIGNMENT: u32 = 0x0CF5;
pub const PACK_SWAP_BYTES: u32 = 0x0D00;
pub const PACK_LSB_FIRST: u32 = 0x0D01;
pub const PACK_ROW_LENGTH: u32 = 0x0D02;
pub const PACK_SKIP_ROWS: u32 = 0x0D03;
pub const PACK_SKIP_PIXELS: u32 = 0x0D04;
pub const PACK_ALIGNMENT: u32 = 0x0D05;
pub const MAP_COLOR: u32 = 0x0D10;
pub const MAP_STENCIL: u32 = 0x0D11;
pub const INDEX_SHIFT: u32 = 0x0D12;
pub const INDEX_OFFSET: u32 = 0x0D13;
pub const RED_SCALE: u32 = 0x0D14;
pub const RED_BIAS: u32 = 0x0D15;
pub const GREEN_SCALE: u32 = 0x0D18;
pub const GREEN_BIAS: u32 = 0x0D19;
pub const BLUE_SCALE: u32 = 0x0D1A;
pub const BLUE_BIAS: u32 = 0x0D1B;
pub const ALPHA_SCALE: u32 = 0x0D1C;
pub const ALPHA_BIAS: u32 = 0x0D1D;
pub const DEPTH_SCALE: u32 = 0x0D1E;
pub const DEPTH_BIAS: u32 = 0x0D1F;
pub const PIXEL_MAP_I_TO_I: u32 = 0x0C70;
pub const PIXEL_MAP_A_TO_A: u32 = 0x0C79;

// Display lists.
pub const COMPILE: u32 = 0x1300;
pub const COMPILE_AND_EXECUTE: u32 = 0x1301;

// Clear bits.
pub const DEPTH_BUFFER_BIT: u32 = 0x0000_0100;
pub const STENCIL_BUFFER_BIT: u32 = 0x0000_0400;
pub const COLOR_BUFFER_BIT: u32 = 0x0000_4000;
pub const ACCUM_BUFFER_BIT: u32 = 0x0000_0200;

// glGet state.
pub const CURRENT_COLOR: u32 = 0x0B00;
pub const CURRENT_NORMAL: u32 = 0x0B02;
pub const CURRENT_TEXTURE_COORDS: u32 = 0x0B03;
pub const POINT_SIZE: u32 = 0x0B11;
pub const LINE_WIDTH: u32 = 0x0B21;
pub const LINE_STIPPLE_PATTERN: u32 = 0x0B25;
pub const LINE_STIPPLE_REPEAT: u32 = 0x0B26;
pub const POLYGON_MODE: u32 = 0x0B40;
pub const CULL_FACE_MODE: u32 = 0x0B45;
pub const FRONT_FACE: u32 = 0x0B46;
pub const SHADE_MODEL: u32 = 0x0B54;
pub const DEPTH_RANGE: u32 = 0x0B70;
pub const DEPTH_WRITEMASK: u32 = 0x0B72;
pub const DEPTH_CLEAR_VALUE: u32 = 0x0B73;
pub const DEPTH_FUNC: u32 = 0x0B74;
pub const STENCIL_CLEAR_VALUE: u32 = 0x0B91;
pub const STENCIL_FUNC: u32 = 0x0B92;
pub const STENCIL_VALUE_MASK: u32 = 0x0B93;
pub const STENCIL_FAIL: u32 = 0x0B94;
pub const STENCIL_PASS_DEPTH_FAIL: u32 = 0x0B95;
pub const STENCIL_PASS_DEPTH_PASS: u32 = 0x0B96;
pub const STENCIL_REF: u32 = 0x0B97;
pub const STENCIL_WRITEMASK: u32 = 0x0B98;
pub const MATRIX_MODE: u32 = 0x0BA0;
pub const VIEWPORT: u32 = 0x0BA2;
pub const MODELVIEW_STACK_DEPTH: u32 = 0x0BA3;
pub const PROJECTION_STACK_DEPTH: u32 = 0x0BA4;
pub const TEXTURE_STACK_DEPTH: u32 = 0x0BA5;
pub const MODELVIEW_MATRIX: u32 = 0x0BA6;
pub const PROJECTION_MATRIX: u32 = 0x0BA7;
pub const TEXTURE_MATRIX: u32 = 0x0BA8;
pub const ALPHA_TEST_FUNC: u32 = 0x0BC1;
pub const ALPHA_TEST_REF: u32 = 0x0BC2;
pub const BLEND_DST: u32 = 0x0BE0;
pub const BLEND_SRC: u32 = 0x0BE1;
pub const DRAW_BUFFER: u32 = 0x0C01;
pub const READ_BUFFER: u32 = 0x0C02;
pub const SCISSOR_BOX: u32 = 0x0C10;
pub const COLOR_CLEAR_VALUE: u32 = 0x0C22;
pub const COLOR_WRITEMASK: u32 = 0x0C23;
pub const DOUBLEBUFFER: u32 = 0x0C32;
pub const STEREO: u32 = 0x0C33;
pub const MAX_LIGHTS: u32 = 0x0D31;
pub const MAX_CLIP_PLANES: u32 = 0x0D32;
pub const MAX_TEXTURE_SIZE: u32 = 0x0D33;
pub const MAX_PIXEL_MAP_TABLE: u32 = 0x0D34;
pub const MAX_ATTRIB_STACK_DEPTH: u32 = 0x0D35;
pub const MAX_MODELVIEW_STACK_DEPTH: u32 = 0x0D36;
pub const MAX_NAME_STACK_DEPTH: u32 = 0x0D37;
pub const MAX_PROJECTION_STACK_DEPTH: u32 = 0x0D38;
pub const MAX_TEXTURE_STACK_DEPTH: u32 = 0x0D39;
pub const MAX_VIEWPORT_DIMS: u32 = 0x0D3A;
pub const MAX_LIST_NESTING: u32 = 0x0B31;
pub const MAX_ELEMENTS_VERTICES: u32 = 0x80E8;
pub const MAX_ELEMENTS_INDICES: u32 = 0x80E9;
pub const SUBPIXEL_BITS: u32 = 0x0D50;
pub const RED_BITS: u32 = 0x0D52;
pub const GREEN_BITS: u32 = 0x0D53;
pub const BLUE_BITS: u32 = 0x0D54;
pub const ALPHA_BITS: u32 = 0x0D55;
pub const DEPTH_BITS: u32 = 0x0D56;
pub const STENCIL_BITS: u32 = 0x0D57;
pub const LIST_BASE: u32 = 0x0B32;
pub const LIST_INDEX: u32 = 0x0B33;
pub const LIST_MODE: u32 = 0x0B30;
pub const POLYGON_OFFSET_UNITS: u32 = 0x2A00;
pub const POLYGON_OFFSET_FACTOR: u32 = 0x8038;
pub const BLEND_COLOR: u32 = 0x8005;
pub const VERTEX_ARRAY_SIZE: u32 = 0x807A;
pub const ARRAY_ELEMENT_LOCK_FIRST_EXT: u32 = 0x81A8;
pub const ARRAY_ELEMENT_LOCK_COUNT_EXT: u32 = 0x81A9;

// ARB_point_parameters.
pub const POINT_SIZE_MIN: u32 = 0x8126;
pub const POINT_SIZE_MAX: u32 = 0x8127;
pub const POINT_FADE_THRESHOLD_SIZE: u32 = 0x8128;
pub const POINT_DISTANCE_ATTENUATION: u32 = 0x8129;

// Attribute bits for glPushAttrib.
pub const ALL_ATTRIB_BITS: u32 = 0x000F_FFFF;
pub const HINT_BIT: u32 = 0x0000_8000;

// OpenGL 1.2.1 current raster state.
pub const CURRENT_RASTER_COLOR: u32 = 0x0B04;
pub const CURRENT_RASTER_INDEX: u32 = 0x0B05;
pub const CURRENT_RASTER_TEXTURE_COORDS: u32 = 0x0B06;
pub const CURRENT_RASTER_POSITION: u32 = 0x0B07;
pub const CURRENT_RASTER_POSITION_VALID: u32 = 0x0B08;
pub const CURRENT_RASTER_DISTANCE: u32 = 0x0B09;
