//! OpenGL entry points exported by the classic Mac OS `OpenGLLibrary`.
//!
//! Attribute families (glVertex*, glColor*, glTexCoord*, glMultiTexCoord*ARB,
//! glNormal3*, glRect*) are recognised from the OpenGL naming convention so
//! every size, component type and vector form shares one decoder.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttribType {
    Byte,
    UnsignedByte,
    Short,
    UnsignedShort,
    Int,
    UnsignedInt,
    Float,
    Double,
}

impl AttribType {
    fn parse(suffix: &str) -> Option<(Self, &str)> {
        for (prefix, kind) in [
            ("ub", Self::UnsignedByte),
            ("us", Self::UnsignedShort),
            ("ui", Self::UnsignedInt),
            ("b", Self::Byte),
            ("s", Self::Short),
            ("i", Self::Int),
            ("f", Self::Float),
            ("d", Self::Double),
        ] {
            if let Some(rest) = suffix.strip_prefix(prefix) {
                return Some((kind, rest));
            }
        }
        None
    }

    pub fn bytes(self) -> u32 {
        match self {
            Self::Byte | Self::UnsignedByte => 1,
            Self::Short | Self::UnsignedShort => 2,
            Self::Int | Self::UnsignedInt | Self::Float => 4,
            Self::Double => 8,
        }
    }
}

macro_rules! named_functions {
    ($($name:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum GlNamed { $($name),* }

        impl GlNamed {
            fn from_suffix(suffix: &str) -> Option<Self> {
                match suffix {
                    $(stringify!($name) => Some(Self::$name),)*
                    _ => None,
                }
            }
        }
    };
}

named_functions! {
    Accum, ActiveTextureARB, AlphaFunc, AreTexturesResident, ArrayElement, Begin,
    BindTexture, BlendColorEXT, BlendFunc, CallList, CallLists, Clear, ClearAccum,
    ClearColor, ClearDepth, ClearIndex, ClearStencil, ClientActiveTextureARB,
    ClipPlane, ColorMask, ColorMaterial, ColorPointer, CopyTexImage2D,
    CopyTexSubImage2D, CullFace, DeleteLists, DeleteTextures, DepthFunc, DepthMask,
    DepthRange, Disable, DisableClientState, DrawArrays, DrawBuffer, DrawElements,
    DrawRangeElements, EdgeFlag, EdgeFlagv, Enable, EnableClientState, End, EndList,
    Finish, Flush, Fogf, Fogfv, Fogi, Fogiv, FrontFace, Frustum, GenLists, GenTextures,
    GetBooleanv, GetDoublev, GetError, GetFloatv, GetIntegerv, GetLightfv,
    GetMaterialfv, GetPointerv, GetString, GetTexEnvfv, GetTexEnviv,
    GetTexParameterfv, GetTexParameteriv, GetTexLevelParameteriv, GetClipPlane,
    Hint, IsEnabled, IsList, IsTexture, LightModelf, LightModelfv, LightModeli,
    LightModeliv, Lightf, Lightfv, Lighti, Lightiv, LineStipple, LineWidth, ListBase,
    LoadIdentity, LoadMatrixd, LoadMatrixf, LockArraysEXT, Materialf, Materialfv,
    Materiali, Materialiv, MatrixMode, MultMatrixd, MultMatrixf, NewList,
    NormalPointer, Ortho, PixelMapfv, PixelMapuiv, PixelMapusv, PixelStoref,
    PixelStorei, PixelTransferf, PixelTransferi, PointParameterfARB,
    PointParameterfvARB, PointParameterfEXT, PointParameterfvEXT, PointSize,
    PolygonMode, PolygonOffset, PopAttrib, PopClientAttrib, PopMatrix,
    PrioritizeTextures, PushAttrib, PushClientAttrib, PushMatrix, ReadBuffer,
    ReadPixels, Rotated, Rotatef, Scaled, Scalef, Scissor, ShadeModel, StencilFunc,
    StencilMask, StencilOp, TexCoordPointer, TexEnvf, TexEnvfv, TexEnvi, TexEnviv,
    TexGend, TexGenf, TexGenfv, TexGeni, TexGeniv, TexImage1D, TexImage2D,
    TexParameterf, TexParameterfv, TexParameteri, TexParameteriv, TexSubImage2D,
    Translated, Translatef, UnlockArraysEXT, VertexPointer, Viewport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlFunction {
    Vertex {
        size: u8,
        kind: AttribType,
        vector: bool,
    },
    Color {
        size: u8,
        kind: AttribType,
        vector: bool,
    },
    TexCoord {
        size: u8,
        kind: AttribType,
        vector: bool,
    },
    MultiTexCoord {
        size: u8,
        kind: AttribType,
        vector: bool,
    },
    Normal {
        kind: AttribType,
        vector: bool,
    },
    Rect {
        kind: AttribType,
        vector: bool,
    },
    Named(GlNamed),
}

fn parse_family(rest: &str, sizes: &[u8]) -> Option<(u8, AttribType, bool)> {
    let size = rest.chars().next()?.to_digit(10)? as u8;
    if !sizes.contains(&size) {
        return None;
    }
    let (kind, tail) = AttribType::parse(&rest[1..])?;
    match tail {
        "" => Some((size, kind, false)),
        "v" => Some((size, kind, true)),
        _ => None,
    }
}

impl GlFunction {
    /// Recognise an `OpenGLLibrary` export by name.
    pub fn from_name(name: &str) -> Option<Self> {
        let suffix = name.strip_prefix("gl")?;
        if let Some(rest) = suffix.strip_prefix("MultiTexCoord") {
            let rest = rest.strip_suffix("ARB")?;
            let (size, kind, vector) = parse_family(rest, &[1, 2, 3, 4])?;
            if matches!(
                kind,
                AttribType::Byte
                    | AttribType::UnsignedByte
                    | AttribType::UnsignedShort
                    | AttribType::UnsignedInt
            ) {
                return None;
            }
            return Some(Self::MultiTexCoord { size, kind, vector });
        }
        if let Some(rest) = suffix.strip_prefix("TexCoord") {
            if let Some((size, kind, vector)) = parse_family(rest, &[1, 2, 3, 4]) {
                if matches!(
                    kind,
                    AttribType::Short | AttribType::Int | AttribType::Float | AttribType::Double
                ) {
                    return Some(Self::TexCoord { size, kind, vector });
                }
            }
        }
        if let Some(rest) = suffix.strip_prefix("Vertex") {
            if let Some((size, kind, vector)) = parse_family(rest, &[2, 3, 4]) {
                if matches!(
                    kind,
                    AttribType::Short | AttribType::Int | AttribType::Float | AttribType::Double
                ) {
                    return Some(Self::Vertex { size, kind, vector });
                }
            }
        }
        if let Some(rest) = suffix.strip_prefix("Color") {
            if let Some((size, kind, vector)) = parse_family(rest, &[3, 4]) {
                return Some(Self::Color { size, kind, vector });
            }
        }
        if let Some(rest) = suffix.strip_prefix("Normal") {
            if let Some((3, kind, vector)) = parse_family(rest, &[3]) {
                if !matches!(
                    kind,
                    AttribType::UnsignedByte | AttribType::UnsignedShort | AttribType::UnsignedInt
                ) {
                    return Some(Self::Normal { kind, vector });
                }
            }
        }
        if let Some(rest) = suffix.strip_prefix("Rect") {
            if let Some((kind, tail)) = AttribType::parse(rest) {
                if matches!(
                    kind,
                    AttribType::Short | AttribType::Int | AttribType::Float | AttribType::Double
                ) {
                    match tail {
                        "" => {
                            return Some(Self::Rect {
                                kind,
                                vector: false,
                            })
                        }
                        "v" => return Some(Self::Rect { kind, vector: true }),
                        _ => {}
                    }
                }
            }
        }
        GlNamed::from_suffix(suffix).map(Self::Named)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_attribute_families_and_named_entry_points() {
        assert_eq!(
            GlFunction::from_name("glVertex3fv"),
            Some(GlFunction::Vertex {
                size: 3,
                kind: AttribType::Float,
                vector: true
            })
        );
        assert_eq!(
            GlFunction::from_name("glColor4ub"),
            Some(GlFunction::Color {
                size: 4,
                kind: AttribType::UnsignedByte,
                vector: false
            })
        );
        assert_eq!(
            GlFunction::from_name("glMultiTexCoord2fARB"),
            Some(GlFunction::MultiTexCoord {
                size: 2,
                kind: AttribType::Float,
                vector: false
            })
        );
        assert_eq!(
            GlFunction::from_name("glTexImage2D"),
            Some(GlFunction::Named(GlNamed::TexImage2D))
        );
        assert_eq!(GlFunction::from_name("glVertex5f"), None);
        assert_eq!(GlFunction::from_name("glColor4ubx"), None);
        assert_eq!(GlFunction::from_name("aglSwapBuffers"), None);
    }
}
