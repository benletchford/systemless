use super::*;

pub(super) struct PpcQ3CoreDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) q3_objects: &'a mut Vec<PpcQ3ObjectRecord>,
    pub(super) next_q3_object: &'a mut u32,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
    pub(super) q3_lifecycle: &'a mut PpcQ3LifecycleState,
}

pub(super) fn dispatch_q3_core_import(
    context: PpcQ3CoreDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3CoreDispatchContext {
        target,
        cpu,
        memory,
        q3_objects,
        next_q3_object,
        q3_error_state,
        q3_lifecycle,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3Initialize => {
            q3_lifecycle.initialize_count = q3_lifecycle.initialize_count.saturating_add(1);
            q3_lifecycle.initialized_depth = q3_lifecycle.initialized_depth.saturating_add(1);
            Some(PpcImportAction::Return(1))
        }
        PpcImportDispatcherTarget::Q3Exit => {
            q3_lifecycle.exit_count = q3_lifecycle.exit_count.saturating_add(1);
            q3_lifecycle.initialized_depth = q3_lifecycle.initialized_depth.saturating_sub(1);
            Some(PpcImportAction::Return(1))
        }
        PpcImportDispatcherTarget::Q3NewObject => {
            Some(PpcImportAction::Return(ppc_q3_alloc_object(
                q3_objects,
                next_q3_object,
                PpcQ3ObjectKind::Generic,
                PPC_Q3_TYPE_NONE,
                cpu.gpr[3],
                0,
            )))
        }
        PpcImportDispatcherTarget::Q3ErrorGet => {
            let first_error_ptr = cpu.gpr[3];
            if first_error_ptr != 0 && !ppc_memory_can_write_bytes(memory, first_error_ptr, 4) {
                Some(PpcImportAction::Return(q3_error_state.last_error))
            } else {
                let (first_error, last_error) = q3_error_state.get();
                if first_error_ptr != 0 {
                    let _ = memory.write_u32_be(first_error_ptr, first_error);
                }
                Some(PpcImportAction::Return(last_error))
            }
        }
        _ => None,
    }
}

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
        PpcImportDispatcherTarget::Q3MatrixTransformSubmit => PpcQ3SubmissionKind::MatrixTransform,
        PpcImportDispatcherTarget::Q3ResetTransformSubmit => PpcQ3SubmissionKind::ResetTransform,
        PpcImportDispatcherTarget::Q3PushSubmit => PpcQ3SubmissionKind::Push,
        PpcImportDispatcherTarget::Q3PopSubmit => PpcQ3SubmissionKind::Pop,
        PpcImportDispatcherTarget::Q3BackfacingStyleSubmit => {
            return Some(PpcImportAction::Return(u32::from(
                ppc_q3_direct_style_submit(
                    cpu,
                    q3_group_memberships,
                    q3_views,
                    q3_view_transforms,
                    q3_submissions,
                    q3_submission_transforms,
                    q3_view_materials,
                    q3_submission_materials,
                    q3_submission_lights,
                    q3_shader_boundaries,
                    q3_shader_uv_transforms,
                    q3_texture_shaders,
                    q3_mipmap_textures,
                    q3_lights,
                    q3_objects,
                    q3_error_state,
                    PpcQ3StyleKind::Backfacing,
                    PPC_Q3_STYLE_TYPE_BACKFACING,
                ),
            )));
        }
        PpcImportDispatcherTarget::Q3InterpolationStyleSubmit => {
            return Some(PpcImportAction::Return(u32::from(
                ppc_q3_direct_style_submit(
                    cpu,
                    q3_group_memberships,
                    q3_views,
                    q3_view_transforms,
                    q3_submissions,
                    q3_submission_transforms,
                    q3_view_materials,
                    q3_submission_materials,
                    q3_submission_lights,
                    q3_shader_boundaries,
                    q3_shader_uv_transforms,
                    q3_texture_shaders,
                    q3_mipmap_textures,
                    q3_lights,
                    q3_objects,
                    q3_error_state,
                    PpcQ3StyleKind::Interpolation,
                    PPC_Q3_STYLE_TYPE_INTERPOLATION,
                ),
            )));
        }
        PpcImportDispatcherTarget::Q3FillStyleSubmit => {
            return Some(PpcImportAction::Return(u32::from(
                ppc_q3_direct_style_submit(
                    cpu,
                    q3_group_memberships,
                    q3_views,
                    q3_view_transforms,
                    q3_submissions,
                    q3_submission_transforms,
                    q3_view_materials,
                    q3_submission_materials,
                    q3_submission_lights,
                    q3_shader_boundaries,
                    q3_shader_uv_transforms,
                    q3_texture_shaders,
                    q3_mipmap_textures,
                    q3_lights,
                    q3_objects,
                    q3_error_state,
                    PpcQ3StyleKind::Fill,
                    PPC_Q3_STYLE_TYPE_FILL,
                ),
            )));
        }
        PpcImportDispatcherTarget::Q3OrientationStyleSubmit => {
            return Some(PpcImportAction::Return(u32::from(
                ppc_q3_direct_style_submit(
                    cpu,
                    q3_group_memberships,
                    q3_views,
                    q3_view_transforms,
                    q3_submissions,
                    q3_submission_transforms,
                    q3_view_materials,
                    q3_submission_materials,
                    q3_submission_lights,
                    q3_shader_boundaries,
                    q3_shader_uv_transforms,
                    q3_texture_shaders,
                    q3_mipmap_textures,
                    q3_lights,
                    q3_objects,
                    q3_error_state,
                    PpcQ3StyleKind::Orientation,
                    PPC_Q3_STYLE_TYPE_ORIENTATION,
                ),
            )));
        }
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
        PpcImportDispatcherTarget::Q3ShaderGetType => {
            Some(PpcImportAction::Return(ppc_q3_class_object_type(
                cpu,
                q3_objects,
                q3_error_state,
                ppc_q3_object_type_is_shader,
            )))
        }
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

pub(super) struct PpcQ3SceneDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) q3_objects: &'a mut Vec<PpcQ3ObjectRecord>,
    pub(super) next_q3_object: &'a mut u32,
    pub(super) q3_cameras: &'a mut Vec<PpcQ3CameraRecord>,
    pub(super) q3_lights: &'a mut Vec<PpcQ3LightRecord>,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
}

pub(super) fn dispatch_q3_scene_import_fast(
    context: PpcQ3SceneDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3SceneDispatchContext {
        target,
        cpu,
        memory,
        q3_objects,
        next_q3_object,
        q3_cameras,
        q3_lights,
        q3_error_state,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3ViewAngleAspectCameraNew => Some(PpcImportAction::Return(
            ppc_q3_view_angle_aspect_camera_new(
                cpu,
                memory,
                q3_objects,
                next_q3_object,
                q3_cameras,
            ),
        )),
        PpcImportDispatcherTarget::Q3OrthographicCameraNew => Some(PpcImportAction::Return(
            ppc_q3_orthographic_camera_new(cpu, memory, q3_objects, next_q3_object, q3_cameras),
        )),
        PpcImportDispatcherTarget::Q3ViewPlaneCameraNew => Some(PpcImportAction::Return(
            ppc_q3_view_plane_camera_new(cpu, memory, q3_objects, next_q3_object, q3_cameras),
        )),
        PpcImportDispatcherTarget::Q3CameraGetPlacement => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_camera_get_placement(cpu, memory, q3_objects, q3_error_state, q3_cameras),
            )))
        }
        PpcImportDispatcherTarget::Q3CameraSetPlacement => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_camera_set_placement(cpu, memory, q3_objects, q3_error_state, q3_cameras),
            )))
        }
        PpcImportDispatcherTarget::Q3CameraGetRange => Some(PpcImportAction::Return(u32::from(
            ppc_q3_camera_get_range(cpu, memory, q3_objects, q3_error_state, q3_cameras),
        ))),
        PpcImportDispatcherTarget::Q3CameraSetRange => Some(PpcImportAction::Return(u32::from(
            ppc_q3_camera_set_range(cpu, memory, q3_objects, q3_error_state, q3_cameras),
        ))),
        PpcImportDispatcherTarget::Q3CameraGetViewPort => Some(PpcImportAction::Return(u32::from(
            ppc_q3_camera_get_viewport(cpu, memory, q3_objects, q3_error_state, q3_cameras),
        ))),
        PpcImportDispatcherTarget::Q3CameraSetViewPort => Some(PpcImportAction::Return(u32::from(
            ppc_q3_camera_set_viewport(cpu, memory, q3_objects, q3_error_state, q3_cameras),
        ))),
        PpcImportDispatcherTarget::Q3CameraGetWorldToView => Some(PpcImportAction::Return(
            u32::from(ppc_q3_camera_get_world_to_view(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_cameras,
            )),
        )),
        PpcImportDispatcherTarget::Q3CameraGetViewToFrustum => Some(PpcImportAction::Return(
            u32::from(ppc_q3_camera_get_view_to_frustum(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_cameras,
            )),
        )),
        PpcImportDispatcherTarget::Q3LightGetType => Some(PpcImportAction::Return(
            ppc_q3_light_get_type(cpu, q3_objects, q3_error_state, q3_lights),
        )),
        PpcImportDispatcherTarget::Q3LightGetState => Some(PpcImportAction::Return(u32::from(
            ppc_q3_light_get_state(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3LightSetState => Some(PpcImportAction::Return(u32::from(
            ppc_q3_light_set_state(cpu, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3LightGetBrightness => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_light_get_brightness(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3LightSetBrightness => Some(PpcImportAction::Return(
            u32::from(ppc_q3_light_set_brightness(
                cpu,
                q3_objects,
                q3_error_state,
                q3_lights,
                ppc_fpr_as_f32(cpu, 1),
            )),
        )),
        PpcImportDispatcherTarget::Q3LightGetColor => Some(PpcImportAction::Return(u32::from(
            ppc_q3_light_get_color(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3LightSetColor => Some(PpcImportAction::Return(u32::from(
            ppc_q3_light_set_color(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3LightGetData => Some(PpcImportAction::Return(u32::from(
            ppc_q3_light_get_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3LightSetData => Some(PpcImportAction::Return(u32::from(
            ppc_q3_light_set_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3AmbientLightNew => Some(PpcImportAction::Return(
            ppc_q3_ambient_light_new(cpu, memory, q3_objects, next_q3_object, q3_lights),
        )),
        PpcImportDispatcherTarget::Q3AmbientLightGetData => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_ambient_light_get_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3AmbientLightSetData => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_ambient_light_set_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3DirectionalLightNew => Some(PpcImportAction::Return(
            ppc_q3_directional_light_new(cpu, memory, q3_objects, next_q3_object, q3_lights),
        )),
        PpcImportDispatcherTarget::Q3DirectionalLightGetCastShadowsState => Some(
            PpcImportAction::Return(u32::from(ppc_q3_directional_light_get_cast_shadows_state(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            ))),
        ),
        PpcImportDispatcherTarget::Q3DirectionalLightSetCastShadowsState => Some(
            PpcImportAction::Return(u32::from(ppc_q3_directional_light_set_cast_shadows_state(
                cpu,
                q3_objects,
                q3_error_state,
                q3_lights,
            ))),
        ),
        PpcImportDispatcherTarget::Q3DirectionalLightGetDirection => Some(PpcImportAction::Return(
            u32::from(ppc_q3_directional_light_get_direction(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3DirectionalLightSetDirection => Some(PpcImportAction::Return(
            u32::from(ppc_q3_directional_light_set_direction(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3DirectionalLightGetData => Some(PpcImportAction::Return(
            u32::from(ppc_q3_directional_light_get_data(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3DirectionalLightSetData => Some(PpcImportAction::Return(
            u32::from(ppc_q3_directional_light_set_data(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3PointLightNew => Some(PpcImportAction::Return(
            ppc_q3_point_light_new(cpu, memory, q3_objects, next_q3_object, q3_lights),
        )),
        PpcImportDispatcherTarget::Q3PointLightGetCastShadowsState => Some(
            PpcImportAction::Return(u32::from(ppc_q3_point_light_get_cast_shadows_state(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            ))),
        ),
        PpcImportDispatcherTarget::Q3PointLightSetCastShadowsState => Some(
            PpcImportAction::Return(u32::from(ppc_q3_point_light_set_cast_shadows_state(
                cpu,
                q3_objects,
                q3_error_state,
                q3_lights,
            ))),
        ),
        PpcImportDispatcherTarget::Q3PointLightGetAttenuation => Some(PpcImportAction::Return(
            u32::from(ppc_q3_point_light_get_attenuation(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3PointLightSetAttenuation => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_point_light_set_attenuation(cpu, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3PointLightGetLocation => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_point_light_get_location(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3PointLightSetLocation => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_point_light_set_location(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3PointLightGetData => Some(PpcImportAction::Return(u32::from(
            ppc_q3_point_light_get_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3PointLightSetData => Some(PpcImportAction::Return(u32::from(
            ppc_q3_point_light_set_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3SpotLightNew => Some(PpcImportAction::Return(
            ppc_q3_spot_light_new(cpu, memory, q3_objects, next_q3_object, q3_lights),
        )),
        PpcImportDispatcherTarget::Q3SpotLightGetCastShadowsState => Some(PpcImportAction::Return(
            u32::from(ppc_q3_spot_light_get_cast_shadows_state(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3SpotLightSetCastShadowsState => Some(PpcImportAction::Return(
            u32::from(ppc_q3_spot_light_set_cast_shadows_state(
                cpu,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3SpotLightGetAttenuation => Some(PpcImportAction::Return(
            u32::from(ppc_q3_spot_light_get_attenuation(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3SpotLightSetAttenuation => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_set_attenuation(cpu, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightGetLocation => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_get_location(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightSetLocation => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_set_location(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightGetDirection => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_get_direction(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightSetDirection => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_set_direction(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightGetHotAngle => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_get_hot_angle(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightSetHotAngle => Some(PpcImportAction::Return(
            u32::from(ppc_q3_spot_light_set_hot_angle(
                cpu,
                q3_objects,
                q3_error_state,
                q3_lights,
                ppc_fpr_as_f32(cpu, 1),
            )),
        )),
        PpcImportDispatcherTarget::Q3SpotLightGetOuterAngle => Some(PpcImportAction::Return(
            u32::from(ppc_q3_spot_light_get_outer_angle(
                cpu,
                memory,
                q3_objects,
                q3_error_state,
                q3_lights,
            )),
        )),
        PpcImportDispatcherTarget::Q3SpotLightSetOuterAngle => Some(PpcImportAction::Return(
            u32::from(ppc_q3_spot_light_set_outer_angle(
                cpu,
                q3_objects,
                q3_error_state,
                q3_lights,
                ppc_fpr_as_f32(cpu, 1),
            )),
        )),
        PpcImportDispatcherTarget::Q3SpotLightGetFallOff => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_get_fall_off(cpu, memory, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightSetFallOff => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_spot_light_set_fall_off(cpu, q3_objects, q3_error_state, q3_lights),
            )))
        }
        PpcImportDispatcherTarget::Q3SpotLightGetData => Some(PpcImportAction::Return(u32::from(
            ppc_q3_spot_light_get_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        PpcImportDispatcherTarget::Q3SpotLightSetData => Some(PpcImportAction::Return(u32::from(
            ppc_q3_spot_light_set_data(cpu, memory, q3_objects, q3_error_state, q3_lights),
        ))),
        _ => None,
    }
}

pub(super) struct PpcQ3StorageFileDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) stores: PpcQ3ObjectStores<'a>,
    pub(super) next_q3_object: &'a mut u32,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) vfs_directories: &'a mut Vec<PpcVfsDirectory>,
    pub(super) vfs_files: &'a mut ProcessVfsFileRecords,
}

pub(super) fn dispatch_q3_storage_file_import(
    context: PpcQ3StorageFileDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3StorageFileDispatchContext {
        target,
        cpu,
        process_memory_manager,
        memory,
        stores,
        next_q3_object,
        q3_error_state,
        heap_cursor,
        heap_limit,
        last_mem_error,
        vfs_directories,
        vfs_files,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3MemoryStorageNew => {
            Some(PpcImportAction::Return(ppc_q3_memory_storage_new(
                cpu,
                process_memory_manager,
                memory,
                stores.q3_objects,
                next_q3_object,
                stores.q3_memory_storages,
                heap_cursor,
                last_mem_error,
            )))
        }
        PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer => {
            Some(PpcImportAction::Return(ppc_q3_memory_storage_new_buffer(
                cpu,
                process_memory_manager,
                memory,
                stores.q3_objects,
                next_q3_object,
                stores.q3_memory_storages,
                heap_cursor,
                last_mem_error,
            )))
        }
        PpcImportDispatcherTarget::Q3FSSpecStorageNew => {
            Some(PpcImportAction::Return(ppc_q3_fsspec_storage_new(
                cpu,
                process_memory_manager,
                memory,
                stores.q3_objects,
                next_q3_object,
                vfs_directories,
                vfs_files,
                heap_cursor,
                last_mem_error,
            )))
        }
        PpcImportDispatcherTarget::Q3FileNew => Some(PpcImportAction::Return(ppc_q3_file_new(
            stores.q3_objects,
            next_q3_object,
            stores.q3_files,
        ))),
        PpcImportDispatcherTarget::Q3StorageGetType => Some(PpcImportAction::Return(
            ppc_q3_storage_get_type(cpu, stores.q3_objects, q3_error_state),
        )),
        PpcImportDispatcherTarget::Q3MemoryStorageSet => Some(PpcImportAction::Return(u32::from(
            ppc_q3_memory_storage_set(
                cpu,
                process_memory_manager,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_memory_storages,
                stores.q3_files,
                heap_cursor,
                last_mem_error,
            ),
        ))),
        PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer => Some(PpcImportAction::Return(
            u32::from(ppc_q3_memory_storage_get_buffer(
                cpu,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_memory_storages,
            )),
        )),
        PpcImportDispatcherTarget::Q3MemoryStorageSetBuffer => Some(PpcImportAction::Return(
            u32::from(ppc_q3_memory_storage_set_buffer(
                cpu,
                process_memory_manager,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_memory_storages,
                stores.q3_files,
                heap_cursor,
                last_mem_error,
            )),
        )),
        PpcImportDispatcherTarget::Q3MemoryStorageGetType => {
            Some(PpcImportAction::Return(ppc_q3_memory_storage_get_type(
                cpu,
                stores.q3_objects,
                q3_error_state,
                stores.q3_memory_storages,
            )))
        }
        PpcImportDispatcherTarget::Q3StorageGetSize => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_storage_get_size(
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_memory_storages,
                cpu.gpr[3],
                cpu.gpr[4],
            ))))
        }
        PpcImportDispatcherTarget::Q3StorageGetData => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_storage_get_data(
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_memory_storages,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[6],
                cpu.gpr[7],
            ))))
        }
        PpcImportDispatcherTarget::Q3StorageSetData => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_storage_set_data(
                process_memory_manager,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_memory_storages,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[6],
                cpu.gpr[7],
                heap_cursor,
                last_mem_error,
            ))))
        }
        PpcImportDispatcherTarget::Q3FileSetStorage => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_file_set_storage(
                cpu,
                stores.q3_files,
                stores.q3_objects,
                q3_error_state,
                stores.q3_object_refs,
                stores.q3_renderer_preferences,
                stores.q3_group_memberships,
                stores.q3_file_groups,
                stores.q3_views,
                stores.q3_submissions,
                stores.q3_view_transforms,
                stores.q3_submission_transforms,
                stores.q3_view_materials,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_view_state_stack,
                stores.q3_completed_frames,
                stores.q3_retained_frames,
                stores.q3_fog_styles,
                stores.q3_memory_storages,
                stores.q3_attributes,
                stores.q3_shader_uv_transforms,
                stores.q3_shader_boundaries,
                stores.q3_mipmap_textures,
                stores.q3_texture_shaders,
                stores.q3_draw_contexts,
                stores.q3_trimeshes,
                stores.q3_styles,
                stores.q3_cameras,
                stores.q3_lights,
            ))))
        }
        PpcImportDispatcherTarget::Q3FileOpenRead => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_file_open_read(
                cpu,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_files,
            ))))
        }
        PpcImportDispatcherTarget::Q3FileReadObject => {
            Some(PpcImportAction::Return(ppc_q3_file_read_object(
                cpu,
                process_memory_manager,
                memory,
                stores.q3_objects,
                next_q3_object,
                heap_cursor,
                heap_limit,
                last_mem_error,
                q3_error_state,
                stores.q3_files,
                stores.q3_memory_storages,
                stores.q3_group_memberships,
                stores.q3_file_groups,
                stores.q3_attributes,
                stores.q3_trimeshes,
                stores.q3_mipmap_textures,
                stores.q3_texture_shaders,
                stores.q3_styles,
            )))
        }
        PpcImportDispatcherTarget::Q3FileIsEndOfFile => {
            Some(PpcImportAction::Return(ppc_q3_file_is_end_of_file(
                cpu,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_files,
            )))
        }
        PpcImportDispatcherTarget::Q3FileClose => Some(PpcImportAction::Return(u32::from(
            ppc_q3_file_close(cpu, stores.q3_objects, q3_error_state, stores.q3_files),
        ))),
        _ => None,
    }
}

pub(super) struct PpcQ3GeometryDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) stores: PpcQ3ObjectStores<'a>,
    pub(super) next_q3_object: &'a mut u32,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) gworlds: &'a [PpcGWorldRecord],
}

pub(super) fn dispatch_q3_geometry_import(
    context: PpcQ3GeometryDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3GeometryDispatchContext {
        target,
        cpu,
        process_memory_manager,
        memory,
        stores,
        next_q3_object,
        q3_error_state,
        heap_cursor,
        last_mem_error,
        gworlds,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3TriMeshNew => {
            Some(PpcImportAction::Return(ppc_q3_trimesh_new(
                cpu,
                process_memory_manager,
                memory,
                stores.q3_objects,
                stores.q3_object_refs,
                next_q3_object,
                heap_cursor,
                last_mem_error,
                stores.q3_trimeshes,
            )))
        }
        PpcImportDispatcherTarget::Q3TriMeshGetData => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_trimesh_get_data(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                stores.q3_objects,
                stores.q3_object_refs,
                q3_error_state,
                stores.q3_attributes,
                stores.q3_texture_shaders,
                stores.q3_trimeshes,
            ))))
        }
        PpcImportDispatcherTarget::Q3TriMeshSetData => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_trimesh_set_data(
                cpu,
                memory,
                stores.q3_objects,
                stores.q3_object_refs,
                stores.q3_renderer_preferences,
                q3_error_state,
                stores.q3_files,
                stores.q3_group_memberships,
                stores.q3_file_groups,
                stores.q3_views,
                stores.q3_submissions,
                stores.q3_view_transforms,
                stores.q3_submission_transforms,
                stores.q3_view_materials,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_view_state_stack,
                stores.q3_completed_frames,
                stores.q3_retained_frames,
                stores.q3_fog_styles,
                stores.q3_memory_storages,
                stores.q3_attributes,
                stores.q3_shader_uv_transforms,
                stores.q3_shader_boundaries,
                stores.q3_mipmap_textures,
                stores.q3_texture_shaders,
                stores.q3_draw_contexts,
                stores.q3_trimeshes,
                stores.q3_styles,
                stores.q3_cameras,
                stores.q3_lights,
            ))))
        }
        PpcImportDispatcherTarget::Q3TriMeshEmptyData => Some(PpcImportAction::Return(u32::from(
            ppc_q3_trimesh_empty_data(cpu, memory),
        ))),
        PpcImportDispatcherTarget::Q3AttributeSetNew => {
            let attribute_set = ppc_q3_alloc_object(
                stores.q3_objects,
                next_q3_object,
                PpcQ3ObjectKind::Generic,
                PPC_Q3_TYPE_ATTRIBUTE_SET,
                0,
                0,
            );
            Some(PpcImportAction::Return(attribute_set))
        }
        PpcImportDispatcherTarget::Q3AttributeSetAdd => Some(PpcImportAction::Return(u32::from(
            ppc_q3_attribute_set_add(
                cpu,
                memory,
                stores.q3_objects,
                stores.q3_object_refs,
                stores.q3_renderer_preferences,
                q3_error_state,
                stores.q3_attributes,
                stores.q3_files,
                stores.q3_group_memberships,
                stores.q3_file_groups,
                stores.q3_views,
                stores.q3_submissions,
                stores.q3_view_transforms,
                stores.q3_submission_transforms,
                stores.q3_view_materials,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_view_state_stack,
                stores.q3_completed_frames,
                stores.q3_retained_frames,
                stores.q3_fog_styles,
                stores.q3_memory_storages,
                stores.q3_shader_uv_transforms,
                stores.q3_shader_boundaries,
                stores.q3_mipmap_textures,
                stores.q3_texture_shaders,
                stores.q3_draw_contexts,
                stores.q3_trimeshes,
                stores.q3_styles,
                stores.q3_cameras,
                stores.q3_lights,
            ),
        ))),
        PpcImportDispatcherTarget::Q3AttributeSetGet => Some(PpcImportAction::Return(u32::from(
            ppc_q3_attribute_set_get(
                cpu,
                memory,
                stores.q3_objects,
                stores.q3_object_refs,
                q3_error_state,
                stores.q3_attributes,
            ),
        ))),
        PpcImportDispatcherTarget::Q3AttributeSetClear => Some(PpcImportAction::Return(u32::from(
            ppc_q3_attribute_set_clear(
                cpu,
                stores.q3_objects,
                stores.q3_object_refs,
                stores.q3_renderer_preferences,
                q3_error_state,
                stores.q3_attributes,
                stores.q3_files,
                stores.q3_group_memberships,
                stores.q3_file_groups,
                stores.q3_views,
                stores.q3_submissions,
                stores.q3_view_transforms,
                stores.q3_submission_transforms,
                stores.q3_view_materials,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_view_state_stack,
                stores.q3_completed_frames,
                stores.q3_retained_frames,
                stores.q3_fog_styles,
                stores.q3_memory_storages,
                stores.q3_shader_uv_transforms,
                stores.q3_shader_boundaries,
                stores.q3_mipmap_textures,
                stores.q3_texture_shaders,
                stores.q3_draw_contexts,
                stores.q3_trimeshes,
                stores.q3_styles,
                stores.q3_cameras,
                stores.q3_lights,
            ),
        ))),
        PpcImportDispatcherTarget::Q3AttributeSetContains => Some(PpcImportAction::Return(
            u32::from(ppc_q3_attribute_set_contains(
                cpu,
                stores.q3_objects,
                q3_error_state,
                stores.q3_attributes,
            )),
        )),
        PpcImportDispatcherTarget::Q3AttributeSetGetNextAttributeType => Some(
            PpcImportAction::Return(u32::from(ppc_q3_attribute_set_get_next_attribute_type(
                cpu,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_attributes,
            ))),
        ),
        PpcImportDispatcherTarget::Q3PixmapDrawContextNew => {
            Some(PpcImportAction::Return(ppc_q3_draw_context_new(
                cpu,
                memory,
                stores.q3_objects,
                next_q3_object,
                stores.q3_draw_contexts,
                PPC_Q3_DRAW_CONTEXT_TYPE_PIXMAP,
                PPC_Q3_PIXMAP_DRAW_CONTEXT_DATA_SIZE,
            )))
        }
        PpcImportDispatcherTarget::Q3MacDrawContextNew => {
            Some(PpcImportAction::Return(ppc_q3_draw_context_new(
                cpu,
                memory,
                stores.q3_objects,
                next_q3_object,
                stores.q3_draw_contexts,
                PPC_Q3_DRAW_CONTEXT_TYPE_MACINTOSH,
                PPC_Q3_MAC_DRAW_CONTEXT_DATA_SIZE,
            )))
        }
        PpcImportDispatcherTarget::Q3DrawContextGetPane => Some(PpcImportAction::Return(
            u32::from(ppc_q3_draw_context_get_pane(
                cpu,
                memory,
                stores.q3_objects,
                q3_error_state,
                stores.q3_draw_contexts,
                gworlds,
            )),
        )),
        _ => None,
    }
}

pub(super) struct PpcQ3GroupViewDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) stores: PpcQ3ObjectStores<'a>,
    pub(super) next_q3_object: &'a mut u32,
    pub(super) q3_state_only_completed_frame_batches:
        &'a mut Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    pub(super) gworlds: &'a mut Vec<PpcGWorldRecord>,
    pub(super) current_gworld: u32,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
    pub(super) input_idle: bool,
}

pub(super) fn dispatch_q3_group_view_import(
    context: PpcQ3GroupViewDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3GroupViewDispatchContext {
        target,
        cpu,
        memory,
        mut stores,
        next_q3_object,
        q3_state_only_completed_frame_batches,
        gworlds,
        current_gworld,
        q3_error_state,
        input_idle,
    } = context;

    macro_rules! set_owned_view_slot {
        ($slot:expr) => {
            ppc_q3_view_set_owned_slot(
                cpu,
                stores.q3_views,
                stores.q3_objects,
                stores.q3_object_refs,
                stores.q3_renderer_preferences,
                stores.q3_files,
                stores.q3_group_memberships,
                stores.q3_file_groups,
                stores.q3_submissions,
                stores.q3_view_transforms,
                stores.q3_submission_transforms,
                stores.q3_view_materials,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_view_state_stack,
                stores.q3_completed_frames,
                stores.q3_retained_frames,
                stores.q3_fog_styles,
                stores.q3_memory_storages,
                stores.q3_attributes,
                stores.q3_shader_uv_transforms,
                stores.q3_shader_boundaries,
                stores.q3_mipmap_textures,
                stores.q3_texture_shaders,
                stores.q3_draw_contexts,
                stores.q3_trimeshes,
                stores.q3_styles,
                stores.q3_cameras,
                stores.q3_lights,
                $slot,
                q3_error_state,
            )
        };
    }

    match target {
        PpcImportDispatcherTarget::Q3DisplayGroupNew => Some(PpcImportAction::Return(
            ppc_q3_display_group_new(stores.q3_objects, next_q3_object),
        )),
        PpcImportDispatcherTarget::Q3LightGroupNew => Some(PpcImportAction::Return(
            ppc_q3_light_group_new(stores.q3_objects, next_q3_object),
        )),
        PpcImportDispatcherTarget::Q3ViewNew => Some(PpcImportAction::Return(ppc_q3_view_new(
            stores.q3_objects,
            next_q3_object,
            stores.q3_views,
        ))),
        PpcImportDispatcherTarget::Q3GroupAddObject => {
            Some(PpcImportAction::Return(ppc_q3_group_add_object(
                cpu.gpr[3],
                cpu.gpr[4],
                None,
                stores.q3_group_memberships,
                stores.q3_objects,
                stores.q3_object_refs,
                stores.q3_lights,
                q3_error_state,
            )))
        }
        PpcImportDispatcherTarget::Q3GroupAddObjectBefore => {
            Some(PpcImportAction::Return(ppc_q3_group_add_object(
                cpu.gpr[3],
                cpu.gpr[5],
                Some(cpu.gpr[4]),
                stores.q3_group_memberships,
                stores.q3_objects,
                stores.q3_object_refs,
                stores.q3_lights,
                q3_error_state,
            )))
        }
        PpcImportDispatcherTarget::Q3GroupCountObjects => Some(PpcImportAction::Return(u32::from(
            ppc_q3_group_count_objects(
                cpu,
                memory,
                stores.q3_group_memberships,
                stores.q3_objects,
                stores.q3_file_groups,
                q3_error_state,
            ),
        ))),
        PpcImportDispatcherTarget::Q3GroupRemovePosition => {
            let removed_object = ppc_q3_group_remove_position(
                cpu,
                stores.q3_group_memberships,
                stores.q3_objects,
                q3_error_state,
            );
            if let Some(object) = removed_object {
                let _ = ppc_q3_object_release_reference(&mut stores, object);
            }
            Some(PpcImportAction::Return(u32::from(removed_object.is_some())))
        }
        PpcImportDispatcherTarget::Q3ViewSetRenderer => Some(PpcImportAction::Return(u32::from(
            set_owned_view_slot!(PpcQ3ViewSlot::Renderer),
        ))),
        PpcImportDispatcherTarget::Q3ViewSetDrawContext => Some(PpcImportAction::Return(
            u32::from(set_owned_view_slot!(PpcQ3ViewSlot::DrawContext)),
        )),
        PpcImportDispatcherTarget::Q3ViewSetCamera => Some(PpcImportAction::Return(u32::from(
            set_owned_view_slot!(PpcQ3ViewSlot::Camera),
        ))),
        PpcImportDispatcherTarget::Q3ViewSetLightGroup => Some(PpcImportAction::Return(u32::from(
            ppc_q3_view_set_light_group(
                cpu,
                stores.q3_views,
                stores.q3_objects,
                stores.q3_object_refs,
                stores.q3_renderer_preferences,
                stores.q3_files,
                stores.q3_group_memberships,
                stores.q3_file_groups,
                stores.q3_submissions,
                stores.q3_view_transforms,
                stores.q3_submission_transforms,
                stores.q3_view_materials,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_view_state_stack,
                stores.q3_completed_frames,
                stores.q3_retained_frames,
                stores.q3_fog_styles,
                stores.q3_memory_storages,
                stores.q3_attributes,
                stores.q3_shader_uv_transforms,
                stores.q3_shader_boundaries,
                stores.q3_mipmap_textures,
                stores.q3_texture_shaders,
                stores.q3_draw_contexts,
                stores.q3_trimeshes,
                stores.q3_styles,
                stores.q3_cameras,
                stores.q3_lights,
                q3_error_state,
            ),
        ))),
        PpcImportDispatcherTarget::Q3ViewGetRenderer
        | PpcImportDispatcherTarget::Q3ViewGetLightGroup
        | PpcImportDispatcherTarget::Q3ViewGetDrawContext
        | PpcImportDispatcherTarget::Q3ViewGetCamera => {
            let slot = match target {
                PpcImportDispatcherTarget::Q3ViewGetRenderer => PpcQ3ViewSlot::Renderer,
                PpcImportDispatcherTarget::Q3ViewGetLightGroup => PpcQ3ViewSlot::LightGroup,
                PpcImportDispatcherTarget::Q3ViewGetDrawContext => PpcQ3ViewSlot::DrawContext,
                PpcImportDispatcherTarget::Q3ViewGetCamera => PpcQ3ViewSlot::Camera,
                _ => unreachable!(),
            };
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_view_get_owned_slot(
                    cpu,
                    memory,
                    stores.q3_views,
                    stores.q3_objects,
                    stores.q3_object_refs,
                    slot,
                    q3_error_state,
                ),
            )))
        }
        PpcImportDispatcherTarget::Q3ViewStartRendering => Some(PpcImportAction::Return(
            u32::from(ppc_q3_view_start_rendering(
                cpu,
                stores.q3_views,
                stores.q3_objects,
                stores.q3_submissions,
                stores.q3_submission_transforms,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                q3_error_state,
            )),
        )),
        PpcImportDispatcherTarget::Q3ViewEndRendering => {
            Some(dispatch_q3_view_end_rendering_import(
                cpu,
                stores.q3_views,
                stores.q3_objects,
                stores.q3_submissions,
                stores.q3_submission_transforms,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_completed_frames,
                stores.q3_retained_frames,
                q3_state_only_completed_frame_batches,
                stores.q3_draw_contexts,
                stores.q3_trimeshes,
                gworlds,
                current_gworld,
                q3_error_state,
                input_idle,
            ))
        }
        PpcImportDispatcherTarget::Q3ViewStartBoundingBox => Some(PpcImportAction::Return(
            u32::from(ppc_q3_view_start_bounding_box(
                cpu,
                stores.q3_views,
                stores.q3_objects,
                q3_error_state,
            )),
        )),
        PpcImportDispatcherTarget::Q3ViewEndBoundingBox => {
            Some(PpcImportAction::Return(ppc_q3_view_end_bounding_box(
                cpu,
                memory,
                stores.q3_views,
                stores.q3_objects,
                stores.q3_submissions,
                stores.q3_submission_transforms,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                stores.q3_retained_frames,
                stores.q3_trimeshes,
                q3_error_state,
            )))
        }
        PpcImportDispatcherTarget::Q3ViewCancel => {
            Some(PpcImportAction::Return(u32::from(ppc_q3_view_cancel(
                cpu,
                stores.q3_views,
                stores.q3_objects,
                stores.q3_submissions,
                stores.q3_submission_transforms,
                stores.q3_submission_materials,
                stores.q3_submission_lights,
                q3_error_state,
            ))))
        }
        _ => None,
    }
}

pub(super) struct PpcQ3ObjectRendererDispatchContext<'a> {
    pub(super) target: &'a PpcImportDispatcherTarget,
    pub(super) cpu: &'a PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) stores: PpcQ3ObjectStores<'a>,
    pub(super) q3_error_state: &'a mut PpcQ3ErrorState,
    pub(super) next_q3_object: &'a mut u32,
}

pub(super) fn dispatch_q3_object_renderer_import_fast(
    context: PpcQ3ObjectRendererDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQ3ObjectRendererDispatchContext {
        target,
        cpu,
        memory,
        mut stores,
        q3_error_state,
        next_q3_object,
    } = context;
    match target {
        PpcImportDispatcherTarget::Q3ObjectDispose => {
            let disposed = ppc_q3_object_release_reference(&mut stores, cpu.gpr[3]);
            if !disposed {
                q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
            }
            Some(PpcImportAction::Return(u32::from(disposed)))
        }
        PpcImportDispatcherTarget::Q3ObjectDuplicate => {
            let object = cpu.gpr[3];
            if ppc_q3_object_retain(stores.q3_objects, stores.q3_object_refs, object) {
                Some(PpcImportAction::Return(object))
            } else {
                q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
                Some(PpcImportAction::Return(0))
            }
        }
        PpcImportDispatcherTarget::Q3SharedGetReference => {
            let object = cpu.gpr[3];
            let object_type = ppc_q3_object_type_for_handle(stores.q3_objects, object);
            if ppc_q3_object_type_is_shared(object_type)
                && ppc_q3_object_retain(stores.q3_objects, stores.q3_object_refs, object)
            {
                Some(PpcImportAction::Return(object))
            } else {
                q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
                Some(PpcImportAction::Return(0))
            }
        }
        PpcImportDispatcherTarget::Q3SharedIsReferenced => {
            let object = cpu.gpr[3];
            let object_type = ppc_q3_object_type_for_handle(stores.q3_objects, object);
            if ppc_q3_object_type_is_shared(object_type) {
                Some(PpcImportAction::Return(u32::from(
                    ppc_q3_object_reference_count(stores.q3_objects, stores.q3_object_refs, object)
                        .is_some_and(|ref_count| ref_count > 1),
                )))
            } else {
                q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
                Some(PpcImportAction::Return(0))
            }
        }
        PpcImportDispatcherTarget::Q3SharedGetType => {
            let shared_type = ppc_q3_shared_type_for_object_type(ppc_q3_object_type_for_handle(
                stores.q3_objects,
                cpu.gpr[3],
            ));
            if shared_type != PPC_Q3_TYPE_NONE {
                Some(PpcImportAction::Return(shared_type))
            } else {
                q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
                Some(PpcImportAction::Return(PPC_Q3_TYPE_NONE))
            }
        }
        PpcImportDispatcherTarget::Q3ShapeGetType => {
            let shape_type = ppc_q3_shape_type_for_object_type(ppc_q3_object_type_for_handle(
                stores.q3_objects,
                cpu.gpr[3],
            ));
            if shape_type != PPC_Q3_TYPE_NONE {
                Some(PpcImportAction::Return(shape_type))
            } else {
                q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
                Some(PpcImportAction::Return(PPC_Q3_TYPE_NONE))
            }
        }
        PpcImportDispatcherTarget::Q3ShapeGetLeafType => {
            let object_type = ppc_q3_object_type_for_handle(stores.q3_objects, cpu.gpr[3]);
            if ppc_q3_shape_type_for_object_type(object_type) != PPC_Q3_TYPE_NONE {
                Some(PpcImportAction::Return(object_type))
            } else {
                q3_error_state.post(PPC_Q3_ERROR_INVALID_OBJECT_PARAMETER);
                Some(PpcImportAction::Return(PPC_Q3_TYPE_NONE))
            }
        }
        PpcImportDispatcherTarget::Q3ObjectIsDrawable => Some(PpcImportAction::Return(u32::from(
            ppc_q3_object_is_drawable(cpu, stores.q3_objects, q3_error_state),
        ))),
        PpcImportDispatcherTarget::Q3ObjectIsType => Some(PpcImportAction::Return(u32::from(
            ppc_q3_object_is_type(cpu, stores.q3_objects, q3_error_state),
        ))),
        PpcImportDispatcherTarget::Q3ObjectGetType
        | PpcImportDispatcherTarget::Q3ObjectGetLeafType => Some(PpcImportAction::Return(
            ppc_q3_object_type(cpu, stores.q3_objects),
        )),
        PpcImportDispatcherTarget::Q3GeometryGetType => {
            Some(PpcImportAction::Return(ppc_q3_class_object_type(
                cpu,
                stores.q3_objects,
                q3_error_state,
                ppc_q3_object_type_is_geometry,
            )))
        }
        PpcImportDispatcherTarget::Q3GroupGetType => {
            Some(PpcImportAction::Return(ppc_q3_class_object_type(
                cpu,
                stores.q3_objects,
                q3_error_state,
                ppc_q3_object_type_is_group,
            )))
        }
        PpcImportDispatcherTarget::Q3RendererNewFromType => Some(PpcImportAction::Return(
            ppc_q3_renderer_new_from_type(cpu, stores.q3_objects, q3_error_state, next_q3_object),
        )),
        PpcImportDispatcherTarget::Q3RendererGetType => Some(PpcImportAction::Return(
            ppc_q3_renderer_get_type(cpu, stores.q3_objects, q3_error_state),
        )),
        PpcImportDispatcherTarget::Q3RendererSync | PpcImportDispatcherTarget::Q3RendererFlush => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_renderer_is_valid(cpu, stores.q3_objects, q3_error_state),
            )))
        }
        PpcImportDispatcherTarget::Q3InteractiveRendererSetDoubleBufferBypass => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_interactive_renderer_set_double_buffer_bypass(
                    cpu,
                    stores.q3_objects,
                    q3_error_state,
                    stores.q3_renderer_preferences,
                ),
            )))
        }
        PpcImportDispatcherTarget::Q3InteractiveRendererSetPreferences => Some(
            PpcImportAction::Return(u32::from(ppc_q3_interactive_renderer_set_preferences(
                cpu,
                stores.q3_objects,
                q3_error_state,
                stores.q3_renderer_preferences,
            ))),
        ),
        PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveContextHints => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_interactive_renderer_set_rave_context_hints(
                    cpu,
                    stores.q3_objects,
                    q3_error_state,
                    stores.q3_renderer_preferences,
                ),
            )))
        }
        PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveContextHints => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_interactive_renderer_get_rave_context_hints(
                    cpu,
                    memory,
                    stores.q3_objects,
                    q3_error_state,
                    stores.q3_renderer_preferences,
                ),
            )))
        }
        PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveDrawContexts => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_interactive_renderer_get_rave_draw_contexts(
                    cpu,
                    memory,
                    stores.q3_objects,
                    q3_error_state,
                ),
            )))
        }
        PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveTextureFilter => {
            Some(PpcImportAction::Return(u32::from(
                ppc_q3_interactive_renderer_set_rave_texture_filter(
                    cpu,
                    stores.q3_objects,
                    q3_error_state,
                    stores.q3_renderer_preferences,
                ),
            )))
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
