use super::*;

pub(super) struct PpcQ3SubmitDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) q3_objects: &'a [PpcQ3ObjectRecord],
    pub(super) q3_group_memberships: &'a [PpcQ3GroupMembershipRecord],
    pub(super) q3_views: &'a mut Vec<PpcQ3ViewStateRecord>,
    pub(super) q3_view_transforms: &'a mut Vec<PpcQ3ViewTransformRecord>,
    pub(super) q3_submissions: &'a mut Vec<PpcQ3SubmissionRecord>,
    pub(super) q3_submission_transforms: &'a mut Vec<PpcQ3SubmissionTransformRecord>,
    pub(super) q3_view_materials: &'a mut Vec<PpcQ3ViewMaterialRecord>,
    pub(super) q3_submission_materials: &'a mut Vec<PpcQ3SubmissionMaterialRecord>,
    pub(super) q3_submission_lights: &'a mut Vec<PpcQ3SubmissionLightRecord>,
    pub(super) q3_view_state_stack: &'a mut Vec<PpcQ3ViewStateSnapshotRecord>,
    pub(super) q3_attributes: &'a [PpcQ3AttributeRecord],
    pub(super) q3_styles: &'a [PpcQ3StyleRecord],
    pub(super) q3_shader_boundaries: &'a [PpcQ3ShaderBoundaryRecord],
    pub(super) q3_shader_uv_transforms: &'a [PpcQ3ShaderUvTransformRecord],
    pub(super) q3_texture_shaders: &'a [PpcQ3TextureShaderRecord],
    pub(super) q3_mipmap_textures: &'a [PpcQ3MipmapTextureRecord],
    pub(super) q3_trimeshes: &'a [PpcQ3TriMeshRecord],
    pub(super) q3_fog_styles: &'a mut Vec<PpcQ3FogStyleRecord>,
    pub(super) q3_lights: &'a [PpcQ3LightRecord],
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
}

pub(super) fn dispatch_q3_submit_import_fast(
    context: PpcQ3SubmitDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3SubmitDispatchContext {
        target,
        cpu,
        memory,
        q3_objects,
        q3_group_memberships,
        q3_views,
        q3_view_transforms,
        q3_submissions,
        q3_submission_transforms,
        q3_view_materials,
        q3_submission_materials,
        q3_submission_lights,
        q3_view_state_stack,
        q3_attributes,
        q3_styles,
        q3_shader_boundaries,
        q3_shader_uv_transforms,
        q3_texture_shaders,
        q3_mipmap_textures,
        q3_trimeshes,
        q3_fog_styles,
        q3_lights,
        q3_error_state,
    } = context;
    let kind = match target {
        PpcImportDispatcherTarget::Q3ShaderSubmit => PpcQ3SubmissionKind::Shader,
        PpcImportDispatcherTarget::Q3StyleSubmit => PpcQ3SubmissionKind::Style,
        PpcImportDispatcherTarget::Q3TriMeshSubmit => PpcQ3SubmissionKind::TriMesh,
        PpcImportDispatcherTarget::Q3ObjectSubmit => PpcQ3SubmissionKind::Object,
        PpcImportDispatcherTarget::Q3FogStyleSubmit => {
            return Some(PpcImportAction::Return(u32::from(ppc_q3_fog_style_submit(
                cpu,
                memory,
                q3_group_memberships,
                q3_views,
                q3_view_transforms,
                q3_submissions,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_fog_styles,
                q3_submission_lights,
                q3_shader_boundaries,
                q3_shader_uv_transforms,
                q3_texture_shaders,
                q3_mipmap_textures,
                q3_lights,
                q3_objects,
                q3_error_state,
            ))));
        }
        _ => return None,
    };
    Some(PpcImportAction::Return(u32::from(ppc_q3_submit(
        cpu,
        memory,
        q3_objects,
        q3_group_memberships,
        q3_views,
        q3_view_transforms,
        q3_submissions,
        q3_submission_transforms,
        q3_view_materials,
        q3_submission_materials,
        q3_submission_lights,
        q3_view_state_stack,
        q3_attributes,
        q3_styles,
        q3_shader_boundaries,
        q3_shader_uv_transforms,
        q3_texture_shaders,
        q3_mipmap_textures,
        q3_trimeshes,
        q3_lights,
        q3_error_state,
        kind,
    ))))
}

pub(super) struct PpcQ3MathDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) q3_objects: &'a mut Vec<PpcQ3ObjectRecord>,
    pub(super) next_q3_object: &'a mut u32,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
}

pub(super) fn dispatch_q3_math_import_fast(
    context: PpcQ3MathDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3MathDispatchContext {
        target,
        cpu,
        memory,
        q3_objects,
        next_q3_object,
        q3_error_state,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3Vector3DNormalize => Some(PpcImportAction::Return(
            ppc_q3_vector3d_normalize(cpu, memory, cpu.gpr[3], cpu.gpr[4]),
        )),
        PpcImportDispatcherTarget::Q3Vector2DNormalize => Some(PpcImportAction::Return(
            ppc_q3_vector2d_normalize(memory, cpu.gpr[3], cpu.gpr[4]),
        )),
        PpcImportDispatcherTarget::Q3Vector3DCross => Some(PpcImportAction::Return(
            ppc_q3_vector3d_cross(cpu, memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]),
        )),
        PpcImportDispatcherTarget::Q3Point2DDistance => {
            cpu.fpr[1] =
                f64::from(ppc_q3_point2d_distance(memory, cpu.gpr[3], cpu.gpr[4])).to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Q3Point3DDistance => {
            cpu.fpr[1] =
                f64::from(ppc_q3_point3d_distance(memory, cpu.gpr[3], cpu.gpr[4])).to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Q3Point3DCrossProductTri => {
            Some(PpcImportAction::Return(ppc_q3_point3d_cross_product_tri(
                memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6],
            )))
        }
        PpcImportDispatcherTarget::Q3Matrix3x3SetTranslate => {
            Some(PpcImportAction::Return(ppc_q3_matrix3x3_set_translate(
                memory,
                cpu.gpr[3],
                ppc_fpr_as_f32(cpu, 1),
                ppc_fpr_as_f32(cpu, 2),
            )))
        }
        PpcImportDispatcherTarget::Q3Matrix4x4SetIdentity => Some(PpcImportAction::Return(
            ppc_q3_matrix4x4_set_identity(memory, cpu.gpr[3]),
        )),
        PpcImportDispatcherTarget::Q3Matrix4x4SetTranslate => {
            Some(PpcImportAction::Return(ppc_q3_matrix4x4_set_translate(
                memory,
                cpu.gpr[3],
                ppc_fpr_as_f32(cpu, 1),
                ppc_fpr_as_f32(cpu, 2),
                ppc_fpr_as_f32(cpu, 3),
            )))
        }
        PpcImportDispatcherTarget::Q3Matrix4x4SetScale => {
            Some(PpcImportAction::Return(ppc_q3_matrix4x4_set_scale(
                memory,
                cpu.gpr[3],
                ppc_fpr_as_f32(cpu, 1),
                ppc_fpr_as_f32(cpu, 2),
                ppc_fpr_as_f32(cpu, 3),
            )))
        }
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateX => Some(PpcImportAction::Return(
            ppc_q3_matrix4x4_set_rotate_x(memory, cpu.gpr[3], ppc_fpr_as_f32(cpu, 1)),
        )),
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateY => Some(PpcImportAction::Return(
            ppc_q3_matrix4x4_set_rotate_y(memory, cpu.gpr[3], ppc_fpr_as_f32(cpu, 1)),
        )),
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateZ => Some(PpcImportAction::Return(
            ppc_q3_matrix4x4_set_rotate_z(memory, cpu.gpr[3], ppc_fpr_as_f32(cpu, 1)),
        )),
        PpcImportDispatcherTarget::Q3Matrix4x4SetRotateXyz => {
            Some(PpcImportAction::Return(ppc_q3_matrix4x4_set_rotate_xyz(
                memory,
                cpu.gpr[3],
                ppc_fpr_as_f32(cpu, 1),
                ppc_fpr_as_f32(cpu, 2),
                ppc_fpr_as_f32(cpu, 3),
            )))
        }
        PpcImportDispatcherTarget::Q3Matrix4x4Multiply => Some(PpcImportAction::Return(
            ppc_q3_matrix4x4_multiply(memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]),
        )),
        PpcImportDispatcherTarget::Q3Matrix4x4Transpose => Some(PpcImportAction::Return(
            ppc_q3_matrix4x4_transpose(memory, cpu.gpr[3], cpu.gpr[4]),
        )),
        PpcImportDispatcherTarget::Q3Matrix4x4Invert => Some(PpcImportAction::Return(
            ppc_q3_matrix4x4_invert(memory, cpu.gpr[3], cpu.gpr[4]),
        )),
        PpcImportDispatcherTarget::Q3Point3DTransform => Some(PpcImportAction::Return(
            ppc_q3_point3d_transform(memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]),
        )),
        PpcImportDispatcherTarget::Q3Point3DTo3DTransformArray => Some(PpcImportAction::Return(
            u32::from(ppc_q3_point3d_transform_array(
                memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8],
                false,
            )),
        )),
        PpcImportDispatcherTarget::Q3Point3DTo4DTransformArray => Some(PpcImportAction::Return(
            u32::from(ppc_q3_point3d_transform_array(
                memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7], cpu.gpr[8],
                true,
            )),
        )),
        PpcImportDispatcherTarget::Q3Vector3DTransform => Some(PpcImportAction::Return(
            ppc_q3_vector3d_transform(memory, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]),
        )),
        PpcImportDispatcherTarget::Q3MatrixTransformNew => Some(PpcImportAction::Return(
            ppc_q3_matrix_transform_new(cpu, q3_objects, next_q3_object),
        )),
        PpcImportDispatcherTarget::Q3MatrixTransformSet => Some(PpcImportAction::Return(
            u32::from(ppc_q3_matrix_transform_set(cpu, q3_objects, q3_error_state)),
        )),
        PpcImportDispatcherTarget::Q3TransformGetMatrix => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_transform_get_matrix(cpu, memory, q3_objects, q3_error_state),
            )))
        }
        _ => None,
    }
}

pub(super) struct PpcQ3ShaderStyleDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) q3_objects: &'a mut Vec<PpcQ3ObjectRecord>,
    pub(super) q3_object_refs: &'a mut Vec<PpcQ3ObjectReferenceRecord>,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
    pub(super) next_q3_object: &'a mut u32,
    pub(super) q3_texture_shaders: &'a mut Vec<PpcQ3TextureShaderRecord>,
    pub(super) q3_mipmap_textures: &'a mut Vec<PpcQ3MipmapTextureRecord>,
    pub(super) q3_shader_uv_transforms: &'a mut Vec<PpcQ3ShaderUvTransformRecord>,
    pub(super) q3_shader_boundaries: &'a mut Vec<PpcQ3ShaderBoundaryRecord>,
    pub(super) q3_styles: &'a mut Vec<PpcQ3StyleRecord>,
}

pub(super) fn dispatch_q3_shader_style_import_fast(
    context: PpcQ3ShaderStyleDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3ShaderStyleDispatchContext {
        target,
        cpu,
        memory,
        q3_objects,
        q3_object_refs,
        q3_error_state,
        next_q3_object,
        q3_texture_shaders,
        q3_mipmap_textures,
        q3_shader_uv_transforms,
        q3_shader_boundaries,
        q3_styles,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3TextureShaderNew => {
            Some(PpcImportAction::Return(ppc_q3_texture_shader_new(
                cpu,
                q3_objects,
                q3_object_refs,
                q3_error_state,
                next_q3_object,
                q3_texture_shaders,
            )))
        }
        PpcImportDispatcherTarget::Q3LambertIlluminationNew => {
            Some(PpcImportAction::Return(ppc_q3_illumination_shader_new(
                q3_objects,
                next_q3_object,
                PPC_Q3_ILLUMINATION_TYPE_LAMBERT,
            )))
        }
        PpcImportDispatcherTarget::Q3NullIlluminationNew => {
            Some(PpcImportAction::Return(ppc_q3_illumination_shader_new(
                q3_objects,
                next_q3_object,
                PPC_Q3_ILLUMINATION_TYPE_NULL,
            )))
        }
        PpcImportDispatcherTarget::Q3PhongIlluminationNew => {
            Some(PpcImportAction::Return(ppc_q3_illumination_shader_new(
                q3_objects,
                next_q3_object,
                PPC_Q3_ILLUMINATION_TYPE_PHONG,
            )))
        }
        PpcImportDispatcherTarget::Q3TextureShaderGetTexture => Some(PpcImportAction::Return(
            u32::from(ppc_q3_texture_shader_get_texture(
                cpu,
                memory,
                q3_objects,
                q3_object_refs,
                q3_error_state,
                q3_texture_shaders,
            )),
        )),
        PpcImportDispatcherTarget::Q3MipmapTextureNew => {
            Some(PpcImportAction::Return(ppc_q3_mipmap_texture_new(
                cpu,
                memory,
                q3_objects,
                q3_object_refs,
                q3_error_state,
                next_q3_object,
                q3_mipmap_textures,
            )))
        }
        PpcImportDispatcherTarget::Q3MipmapTextureGetMipmap => Some(PpcImportAction::Return(
            u32::from(ppc_q3_mipmap_texture_get_mipmap(
                cpu,
                memory,
                q3_objects,
                q3_object_refs,
                q3_error_state,
                q3_mipmap_textures,
            )),
        )),
        PpcImportDispatcherTarget::Q3ShaderGetUVTransform => Some(PpcImportAction::Return(
            u32::from(ppc_q3_shader_get_uv_transform(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_shader_uv_transforms,
            )),
        )),
        PpcImportDispatcherTarget::Q3ShaderSetUVTransform => Some(PpcImportAction::Return(
            u32::from(ppc_q3_shader_set_uv_transform(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_shader_uv_transforms,
            )),
        )),
        PpcImportDispatcherTarget::Q3ShaderGetUBoundary => Some(PpcImportAction::Return(
            u32::from(ppc_q3_shader_get_boundary(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_shader_boundaries,
                PpcQ3ShaderBoundaryAxis::U,
            )),
        )),
        PpcImportDispatcherTarget::Q3ShaderSetUBoundary => Some(PpcImportAction::Return(
            u32::from(ppc_q3_shader_set_boundary(
                cpu,
                q3_objects,
                q3_error_state,
                q3_shader_boundaries,
                PpcQ3ShaderBoundaryAxis::U,
            )),
        )),
        PpcImportDispatcherTarget::Q3ShaderGetVBoundary => Some(PpcImportAction::Return(
            u32::from(ppc_q3_shader_get_boundary(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_shader_boundaries,
                PpcQ3ShaderBoundaryAxis::V,
            )),
        )),
        PpcImportDispatcherTarget::Q3ShaderSetVBoundary => Some(PpcImportAction::Return(
            u32::from(ppc_q3_shader_set_boundary(
                cpu,
                q3_objects,
                q3_error_state,
                q3_shader_boundaries,
                PpcQ3ShaderBoundaryAxis::V,
            )),
        )),
        PpcImportDispatcherTarget::Q3BackfacingStyleNew
        | PpcImportDispatcherTarget::Q3InterpolationStyleNew
        | PpcImportDispatcherTarget::Q3FillStyleNew
        | PpcImportDispatcherTarget::Q3OrientationStyleNew => {
            let (kind, object_type) = match target {
                PpcImportDispatcherTarget::Q3BackfacingStyleNew => {
                    (PpcQ3StyleKind::Backfacing, PPC_Q3_STYLE_TYPE_BACKFACING)
                }
                PpcImportDispatcherTarget::Q3InterpolationStyleNew => (
                    PpcQ3StyleKind::Interpolation,
                    PPC_Q3_STYLE_TYPE_INTERPOLATION,
                ),
                PpcImportDispatcherTarget::Q3FillStyleNew => {
                    (PpcQ3StyleKind::Fill, PPC_Q3_STYLE_TYPE_FILL)
                }
                PpcImportDispatcherTarget::Q3OrientationStyleNew => {
                    (PpcQ3StyleKind::Orientation, PPC_Q3_STYLE_TYPE_ORIENTATION)
                }
                _ => unreachable!(),
            };
            Some(PpcImportAction::Return(ppc_q3_style_new(
                cpu,
                q3_objects,
                next_q3_object,
                q3_styles,
                kind,
                object_type,
            )))
        }
        PpcImportDispatcherTarget::Q3BackfacingStyleGet
        | PpcImportDispatcherTarget::Q3InterpolationStyleGet
        | PpcImportDispatcherTarget::Q3FillStyleGet
        | PpcImportDispatcherTarget::Q3OrientationStyleGet => {
            let kind = match target {
                PpcImportDispatcherTarget::Q3BackfacingStyleGet => PpcQ3StyleKind::Backfacing,
                PpcImportDispatcherTarget::Q3InterpolationStyleGet => PpcQ3StyleKind::Interpolation,
                PpcImportDispatcherTarget::Q3FillStyleGet => PpcQ3StyleKind::Fill,
                PpcImportDispatcherTarget::Q3OrientationStyleGet => PpcQ3StyleKind::Orientation,
                _ => unreachable!(),
            };
            Some(PpcImportAction::Return(u32::from(ppc_q3_style_get(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_styles,
                kind,
            ))))
        }
        PpcImportDispatcherTarget::Q3BackfacingStyleSet
        | PpcImportDispatcherTarget::Q3InterpolationStyleSet
        | PpcImportDispatcherTarget::Q3FillStyleSet
        | PpcImportDispatcherTarget::Q3OrientationStyleSet => {
            let kind = match target {
                PpcImportDispatcherTarget::Q3BackfacingStyleSet => PpcQ3StyleKind::Backfacing,
                PpcImportDispatcherTarget::Q3InterpolationStyleSet => PpcQ3StyleKind::Interpolation,
                PpcImportDispatcherTarget::Q3FillStyleSet => PpcQ3StyleKind::Fill,
                PpcImportDispatcherTarget::Q3OrientationStyleSet => PpcQ3StyleKind::Orientation,
                _ => unreachable!(),
            };
            Some(PpcImportAction::Return(u32::from(ppc_q3_style_set(
                cpu,
                q3_objects,
                q3_error_state,
                q3_styles,
                kind,
            ))))
        }
        _ => None,
    }
}

pub(super) struct PpcQ3ObjectGroupDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) q3_objects: &'a [PpcQ3ObjectRecord],
    pub(super) q3_object_refs: &'a mut Vec<PpcQ3ObjectReferenceRecord>,
    pub(super) q3_group_memberships: &'a [PpcQ3GroupMembershipRecord],
    pub(super) q3_file_groups: &'a [PpcQ3FileGroupRecord],
    pub(super) q3_lights: &'a [PpcQ3LightRecord],
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
}

pub(super) fn dispatch_q3_object_group_import_fast(
    context: PpcQ3ObjectGroupDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3ObjectGroupDispatchContext {
        target,
        cpu,
        memory,
        q3_objects,
        q3_object_refs,
        q3_group_memberships,
        q3_file_groups,
        q3_lights,
        q3_error_state,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3ObjectIsType => Some(PpcImportAction::Return(u32::from(
            ppc_q3_object_is_type(cpu, q3_objects, q3_error_state),
        ))),
        PpcImportDispatcherTarget::Q3GroupGetFirstPosition => Some(PpcImportAction::Return(
            u32::from(ppc_q3_group_get_first_position(
                cpu,
                memory,
                q3_group_memberships,
                q3_objects,
                q3_file_groups,
                q3_error_state,
            )),
        )),
        PpcImportDispatcherTarget::Q3GroupGetNextPosition => Some(PpcImportAction::Return(
            u32::from(ppc_q3_group_get_next_position(
                cpu,
                memory,
                q3_group_memberships,
                q3_objects,
                q3_file_groups,
                q3_error_state,
            )),
        )),
        PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType => Some(PpcImportAction::Return(
            u32::from(ppc_q3_group_get_first_position_of_type(
                cpu,
                memory,
                q3_group_memberships,
                q3_objects,
                q3_file_groups,
                q3_error_state,
            )),
        )),
        PpcImportDispatcherTarget::Q3GroupGetPositionObject => Some(PpcImportAction::Return(
            u32::from(ppc_q3_group_get_position_object(
                cpu,
                memory,
                q3_group_memberships,
                q3_objects,
                q3_file_groups,
                q3_object_refs,
                q3_lights,
                &[],
                q3_error_state,
            )),
        )),
        _ => None,
    }
}
