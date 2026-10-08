//! QuickDraw 3D scene command decoding, software rasterization, render target
//! resolution, frame completion, and replay memory capture on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    fn q3_render_target(&mut self, commands: &[PpcQ3SceneCommand]) -> Option<PpcQ3RenderTarget> {
        commands
            .iter()
            .filter_map(|command| match command {
                PpcQ3SceneCommand::TriMesh(command) => Some(command.view_state.draw_context),
                PpcQ3SceneCommand::Submission(command) => Some(command.view_state.draw_context),
            })
            .filter(|draw_context| *draw_context != 0)
            .find_map(|draw_context| self.q3_draw_context_render_target(draw_context))
            .or_else(|| {
                self.current_front_buffer()
                    .map(|front_buffer| PpcQ3RenderTarget {
                        front_buffer,
                        viewport: None,
                        clear_color: None,
                        source: PpcQ3RenderTargetSource::CurrentGWorld,
                        draw_context: None,
                        gworld: Some(*self.current_gworld),
                    })
            })
    }

    #[cfg(test)]
    pub(crate) fn q3_draw_context_front_buffer(
        &mut self,
        draw_context: u32,
    ) -> Option<PpcFrontBuffer> {
        self.q3_draw_context_render_target(draw_context)
            .map(|target| target.front_buffer)
    }

    fn q3_draw_context_render_target(&mut self, draw_context: u32) -> Option<PpcQ3RenderTarget> {
        ppc_q3_live_draw_context_render_target(
            &mut self.memory,
            &self.q3_objects,
            &self.q3_draw_contexts,
            &self.gworlds,
            draw_context,
        )
    }

    pub fn q3_scene_commands(&mut self) -> Vec<PpcQ3SceneCommand> {
        let submissions = self.q3_submissions.clone();
        self.q3_scene_commands_from_slices(
            &submissions,
            &self.q3_submission_transforms.clone(),
            &self.q3_submission_materials.clone(),
            &self.q3_submission_lights.clone(),
            &[],
        )
    }

    fn q3_scene_commands_from_slices(
        &mut self,
        submissions: &[PpcQ3SubmissionRecord],
        transforms: &[PpcQ3SubmissionTransformRecord],
        materials: &[PpcQ3SubmissionMaterialRecord],
        lights: &[PpcQ3SubmissionLightRecord],
        retained_trimeshes: &[PpcQ3TriMeshRecord],
    ) -> Vec<PpcQ3SceneCommand> {
        submissions
            .iter()
            .enumerate()
            .filter_map(|(index, submission)| {
                self.q3_scene_command_from_slices(
                    index,
                    submission,
                    transforms,
                    materials,
                    lights,
                    retained_trimeshes,
                )
            })
            .collect()
    }

    pub fn capture_q3_scene_replay(
        &mut self,
        commands: Vec<PpcQ3SceneCommand>,
    ) -> PpcQ3SceneReplay {
        let mut memory_regions = Vec::new();
        for command in &commands {
            match command {
                PpcQ3SceneCommand::TriMesh(command) => {
                    self.capture_q3_trimesh_replay_memory(&mut memory_regions, command);
                    self.capture_q3_material_replay_memory(&mut memory_regions, &command.material);
                }
                PpcQ3SceneCommand::Submission(command) => {
                    self.capture_q3_material_replay_memory(&mut memory_regions, &command.material);
                }
            }
        }
        PpcQ3SceneReplay {
            commands,
            memory_regions,
        }
    }

    pub fn render_q3_scene_commands_to_front_buffer(&mut self) -> PpcQ3SoftwareRenderStats {
        let commands = self.q3_scene_commands();
        self.render_q3_scene_commands_to_front_buffer_inner(commands)
    }

    /// Render a captured QD3D scene-command stream without reading the live
    /// QD3D submission queues. The replay still uses this app's memory and
    /// current render-target state for guest-buffer-backed geometry/texture data.
    pub fn replay_q3_scene_commands_to_front_buffer(
        &mut self,
        commands: Vec<PpcQ3SceneCommand>,
    ) -> PpcQ3SoftwareRenderStats {
        self.render_q3_scene_commands_to_front_buffer_inner(commands)
    }

    pub fn replay_q3_scene_replay_to_front_buffer(
        &mut self,
        replay: &PpcQ3SceneReplay,
    ) -> PpcQ3SoftwareRenderStats {
        for region in &replay.memory_regions {
            self.install_q3_scene_replay_memory_region(region);
        }
        self.replay_q3_scene_commands_to_front_buffer(replay.commands.clone())
    }

    pub fn render_completed_q3_frames_to_front_buffer(&mut self) -> PpcQ3SoftwareRenderStats {
        self.render_completed_q3_frames_to_front_buffer_fast()
    }

    pub fn render_completed_q3_frames_to_front_buffer_fast(&mut self) -> PpcQ3SoftwareRenderStats {
        let state_only_batches = std::mem::take(&mut self.q3_state_only_completed_frame_batches);
        let frames = std::mem::take(&mut self.q3_completed_frames);
        let mut stats = PpcQ3SoftwareRenderStats::default();
        stats.add_state_only_batches(&state_only_batches);
        let mut saw_missing_target = false;
        let last_trimesh_frame = frames.iter().rposition(|frame| {
            frame
                .submissions
                .iter()
                .any(|submission| submission.kind == PpcQ3SubmissionKind::TriMesh)
        });
        for (frame_index, frame) in frames.into_iter().enumerate() {
            if frame.submissions.is_empty() {
                continue;
            }
            if !frame
                .submissions
                .iter()
                .any(|submission| submission.kind == PpcQ3SubmissionKind::TriMesh)
                || Some(frame_index) != last_trimesh_frame
            {
                if let Some(target) = self.q3_completed_frame_render_target(&frame) {
                    stats.record_target(target);
                }
                stats.add_empty_frames(1);
                continue;
            }
            let commands = self.q3_completed_frame_scene_commands(&frame);
            let frame_stats = self.render_q3_scene_commands_to_front_buffer_inner(commands);
            stats.merge_frame_stats(frame_stats, &mut saw_missing_target);
        }
        self.prune_q3_immediate_trimeshes();
        stats
    }

    pub fn render_completed_q3_frames_to_front_buffer_with_observer<F>(
        &mut self,
        mut observer: F,
    ) -> PpcQ3SoftwareRenderStats
    where
        F: FnMut(&PpcQ3CompletedFrameRecord, &PpcQ3SceneReplay),
    {
        let state_only_batches = std::mem::take(&mut self.q3_state_only_completed_frame_batches);
        let frames = std::mem::take(&mut self.q3_completed_frames);
        let mut stats = PpcQ3SoftwareRenderStats::default();
        stats.add_state_only_batches(&state_only_batches);
        let mut saw_missing_target = false;
        for frame in frames {
            if frame.submissions.is_empty() {
                continue;
            }
            let commands = self.q3_completed_frame_scene_commands(&frame);
            let replay = self.capture_q3_scene_replay(commands);
            observer(&frame, &replay);
            let frame_stats = self.replay_q3_scene_replay_to_front_buffer(&replay);
            stats.merge_frame_stats(frame_stats, &mut saw_missing_target);
        }
        self.prune_q3_immediate_trimeshes();
        stats
    }

    /// Drain the newest completed triangle frame as GPU-ready primitives.
    /// Unsupported QD3D state leaves the queue untouched so callers can use
    /// `render_completed_q3_frames_to_front_buffer_fast` as an exact fallback.
    pub fn take_completed_q3_gpu_frame(&mut self) -> Option<PpcQ3GpuFrame> {
        let frame = self
            .q3_completed_frames
            .iter()
            .rfind(|frame| {
                frame
                    .submissions
                    .iter()
                    .any(|submission| submission.kind == PpcQ3SubmissionKind::TriMesh)
            })?
            .clone();
        let commands = self.q3_completed_frame_scene_commands(&frame);
        let gpu_frame = self.prepare_q3_gpu_frame(commands)?;
        self.q3_completed_frames.clear();
        self.q3_state_only_completed_frame_batches.clear();
        self.prune_q3_immediate_trimeshes();
        Some(gpu_frame)
    }

    fn prepare_q3_gpu_frame(&mut self, commands: Vec<PpcQ3SceneCommand>) -> Option<PpcQ3GpuFrame> {
        let target = self.q3_render_target(&commands)?;
        let front_buffer = target.front_buffer;
        if front_buffer.depth != 16 || front_buffer.width == 0 || front_buffer.height == 0 {
            return None;
        }
        let viewport = target
            .viewport
            .unwrap_or_else(|| PpcQ3ViewportRect::full(front_buffer));
        let viewport_bounds = viewport.inclusive_bounds()?;
        let mut textures = Vec::new();
        let mut draws = Vec::new();

        for command in commands {
            let PpcQ3SceneCommand::TriMesh(command) = command else {
                continue;
            };
            if ppc_q3_software_fill_style(&command.material) != PPC_Q3_FILL_STYLE_FILLED {
                return None;
            }
            let material = ppc_q3_software_material_sample(
                &self.q3_objects,
                &self.q3_memory_storages,
                &command.material,
            );
            if material.fog_style.is_some()
                || command
                    .lights
                    .lights
                    .iter()
                    .any(|light| light.data.is_on != 0)
            {
                return None;
            }
            let transparency = material.transparency;
            if (transparency.0 - transparency.1).abs() > 0.001
                || (transparency.0 - transparency.2).abs() > 0.001
            {
                return None;
            }
            let points = self.q3_software_trimesh_points(&command)?;
            let triangles = self.q3_software_trimesh_triangles(&command, points.len());
            if points.is_empty() || triangles.is_empty() {
                continue;
            }
            let vertex_uvs = if material.texture.is_some() {
                self.q3_software_trimesh_vertex_uvs(&command, points.len())?
            } else {
                vec![(0.0, 0.0); points.len()]
            };
            let vertex_diffuse =
                self.q3_software_trimesh_vertex_diffuse_colors(&command, points.len());
            let vertex_alpha = self.q3_software_trimesh_vertex_alphas(&command, points.len());
            let triangle_diffuse = self.q3_software_trimesh_triangle_diffuse_colors(&command);
            let triangle_alpha = self.q3_software_trimesh_triangle_alphas(&command);
            let mut projected = points
                .iter()
                .copied()
                .map(|point| ppc_q3_software_project_point(&command, front_buffer, viewport, point))
                .collect::<Vec<_>>();
            for (index, projected) in projected.iter_mut().enumerate() {
                let Some(projected) = projected else {
                    continue;
                };
                projected.uv = vertex_uvs.get(index).copied();
                projected.diffuse = vertex_diffuse
                    .as_ref()
                    .and_then(|colors| colors.get(index).copied());
                projected.vertex_alpha = vertex_alpha
                    .as_ref()
                    .and_then(|alphas| alphas.get(index).copied());
            }

            let texture_index = match material.texture {
                Some(texture) => {
                    let texture = ppc_q3_gpu_texture_from_software(
                        &mut self.memory,
                        texture,
                        material.shader_boundary,
                    )?;
                    let index = textures.len();
                    textures.push(texture);
                    Some(index)
                }
                None => None,
            };
            let interpolation_style = ppc_q3_software_interpolation_style(&command.material);
            let backfacing_style = ppc_q3_software_backfacing_style(&command.material);
            let orientation_style = ppc_q3_software_orientation_style(&command.material);
            let mut vertices = Vec::with_capacity(triangles.len().saturating_mul(3));
            let mut blend =
                transparency.0 < 0.999 || ppc_q3_software_texture_may_blend(material.texture);

            for triangle in triangles {
                let mut triangle_vertices = [
                    projected.get(triangle.points[0]).copied().flatten()?,
                    projected.get(triangle.points[1]).copied().flatten()?,
                    projected.get(triangle.points[2]).copied().flatten()?,
                ];
                if let Some(diffuse) = triangle_diffuse
                    .as_ref()
                    .and_then(|colors| colors.get(triangle.index).copied())
                {
                    for vertex in &mut triangle_vertices {
                        if vertex.diffuse.is_none() {
                            vertex.diffuse = Some(diffuse);
                        }
                    }
                }
                if let Some(alpha) = triangle_alpha
                    .as_ref()
                    .and_then(|alphas| alphas.get(triangle.index).copied())
                {
                    for vertex in &mut triangle_vertices {
                        if vertex.vertex_alpha.is_none() {
                            vertex.vertex_alpha = Some(alpha);
                        }
                    }
                }
                let clipped = ppc_q3_software_clip_projected_triangle(
                    command.camera,
                    front_buffer,
                    viewport,
                    triangle_vertices,
                );
                if clipped.len() < 3 {
                    continue;
                }
                let mut clipped = clipped
                    .into_iter()
                    .map(ppc_q3_software_projected_vertex_to_point)
                    .collect::<Vec<_>>();
                if ppc_q3_software_culls_backfacing(backfacing_style, orientation_style, &clipped) {
                    continue;
                }
                ppc_q3_software_flip_backfacing_normals(
                    backfacing_style,
                    orientation_style,
                    &mut clipped,
                );
                let flat_diffuse = (interpolation_style == PPC_Q3_INTERPOLATION_STYLE_NONE)
                    .then(|| clipped.first().and_then(|vertex| vertex.diffuse))
                    .flatten();
                let flat_alpha = (interpolation_style == PPC_Q3_INTERPOLATION_STYLE_NONE)
                    .then(|| clipped.first().and_then(|vertex| vertex.vertex_alpha))
                    .flatten();
                for index in 1..clipped.len().saturating_sub(1) {
                    for point in [clipped[0], clipped[index], clipped[index + 1]] {
                        let diffuse = flat_diffuse.or(point.diffuse).unwrap_or(material.diffuse);
                        let vertex_alpha = flat_alpha.or(point.vertex_alpha).unwrap_or(1.0);
                        if vertex_alpha < 0.999 {
                            blend = true;
                        }
                        let uv = ppc_q3_software_transform_uv(
                            point.uv.unwrap_or((0.0, 0.0)),
                            material.uv_transform,
                        )?;
                        vertices.push(PpcQ3GpuVertex {
                            screen_x: point.x as f32,
                            screen_y: point.y as f32,
                            depth: point.z,
                            reciprocal_w: point.reciprocal_w,
                            uv: [uv.0, uv.1],
                            color: [
                                diffuse.0,
                                diffuse.1,
                                diffuse.2,
                                transparency.0 * vertex_alpha,
                            ],
                        });
                    }
                }
            }
            if !vertices.is_empty() {
                draws.push(PpcQ3GpuDraw {
                    texture: texture_index,
                    vertices,
                    blend,
                    write_depth: !blend,
                });
            }
        }
        if draws.is_empty() {
            return None;
        }
        Some(PpcQ3GpuFrame {
            width: front_buffer.width,
            height: front_buffer.height,
            viewport: [
                viewport_bounds.0,
                viewport_bounds.1,
                viewport_bounds.2,
                viewport_bounds.3,
            ],
            clear_color: target.clear_color.map(|color| {
                let (red, green, blue) = ppc_q3_rgb555_to_color(color);
                [red, green, blue, 1.0]
            }),
            textures,
            draws,
        })
    }

    fn render_q3_scene_commands_to_front_buffer_inner(
        &mut self,
        commands: Vec<PpcQ3SceneCommand>,
    ) -> PpcQ3SoftwareRenderStats {
        let Some(target) = self.q3_render_target(&commands) else {
            return PpcQ3SoftwareRenderStats::default();
        };
        let front_buffer = target.front_buffer;
        let viewport = target
            .viewport
            .unwrap_or_else(|| PpcQ3ViewportRect::full(front_buffer));
        if !matches!(front_buffer.depth, 8 | 16)
            || front_buffer.width == 0
            || front_buffer.height == 0
        {
            return PpcQ3SoftwareRenderStats::default();
        }

        let mut stats = PpcQ3SoftwareRenderStats::default();
        stats.record_target(target);
        let indexed_clut = (front_buffer.depth == 8).then(|| {
            target
                .gworld
                .map(|gworld| {
                    ppc_live_gworld_clut(
                        &mut self.memory,
                        &self.gworlds,
                        gworld,
                        &self.screen_clut,
                        &self.color_manager_clut,
                    )
                })
                .unwrap_or(*self.color_manager_clut)
        });
        let surface =
            PpcQ3SoftwareFrontBufferSurface::new(&mut self.memory, front_buffer, indexed_clut);
        let mut depth_buffer = PpcQ3SoftwareDepthBuffer::new(front_buffer);
        if let (Some(clear_color), Some((left, top, right, bottom))) =
            (target.clear_color, viewport.inclusive_bounds())
        {
            for y in top..=bottom {
                for x in left..=right {
                    let _ = surface.write_pixel(&mut self.memory, (x, y), clear_color);
                }
            }
        }
        let mut deferred_primitives = Vec::new();
        for command in commands {
            let command = match command {
                PpcQ3SceneCommand::TriMesh(command) => command,
                PpcQ3SceneCommand::Submission(_) => continue,
            };
            let Some(points) = self.q3_software_trimesh_points(&command) else {
                continue;
            };
            if points.is_empty() {
                continue;
            }
            let material = ppc_q3_software_material_sample(
                &self.q3_objects,
                &self.q3_memory_storages,
                &command.material,
            );
            let uses_ambient_coefficients =
                ppc_q3_software_uses_ambient_coefficients(&material, &command.lights);
            let uses_normals = ppc_q3_software_uses_normals(&material, &command.lights);
            let uses_specular_attributes =
                ppc_q3_software_uses_specular_attributes(&material, &command.lights);
            let vertex_uvs = if material.texture.is_some() {
                self.q3_software_trimesh_vertex_uvs(&command, points.len())
            } else {
                None
            };
            let vertex_diffuse_colors =
                self.q3_software_trimesh_vertex_diffuse_colors(&command, points.len());
            let vertex_ambient_coefficients = if uses_ambient_coefficients {
                self.q3_software_trimesh_vertex_ambient_coefficients(&command, points.len())
            } else {
                None
            };
            let vertex_specular_colors = if uses_specular_attributes {
                self.q3_software_trimesh_vertex_specular_colors(&command, points.len())
            } else {
                None
            };
            let vertex_specular_controls = if uses_specular_attributes {
                self.q3_software_trimesh_vertex_specular_controls(&command, points.len())
            } else {
                None
            };
            let vertex_highlight_states = if uses_specular_attributes {
                self.q3_software_trimesh_vertex_highlight_states(&command, points.len())
            } else {
                None
            };
            let vertex_normals = if uses_normals {
                self.q3_software_trimesh_vertex_normals(&command, points.len())
            } else {
                None
            };
            let mut projected: Vec<Option<PpcQ3SoftwareProjectedVertex>> = points
                .iter()
                .copied()
                .map(|point| ppc_q3_software_project_point(&command, front_buffer, viewport, point))
                .collect();
            if let Some(vertex_uvs) = vertex_uvs {
                for (projected, uv) in projected.iter_mut().zip(vertex_uvs) {
                    if let Some(projected) = projected {
                        projected.uv = Some(uv);
                    }
                }
            }
            if let Some(vertex_diffuse_colors) = vertex_diffuse_colors {
                for (projected, diffuse) in projected.iter_mut().zip(vertex_diffuse_colors) {
                    if let Some(projected) = projected {
                        projected.diffuse = Some(diffuse);
                    }
                }
            }
            if let Some(vertex_ambient_coefficients) = vertex_ambient_coefficients {
                for (projected, ambient_coefficient) in
                    projected.iter_mut().zip(vertex_ambient_coefficients)
                {
                    if let Some(projected) = projected {
                        projected.ambient_coefficient = Some(ambient_coefficient);
                    }
                }
            }
            if let Some(vertex_specular_colors) = vertex_specular_colors {
                for (projected, specular_color) in projected.iter_mut().zip(vertex_specular_colors)
                {
                    if let Some(projected) = projected {
                        projected.specular_color = Some(specular_color);
                    }
                }
            }
            if let Some(vertex_specular_controls) = vertex_specular_controls {
                for (projected, specular_control) in
                    projected.iter_mut().zip(vertex_specular_controls)
                {
                    if let Some(projected) = projected {
                        projected.specular_control = Some(specular_control);
                    }
                }
            }
            if let Some(vertex_highlight_states) = vertex_highlight_states {
                for (projected, highlight_state) in
                    projected.iter_mut().zip(vertex_highlight_states)
                {
                    if let Some(projected) = projected {
                        projected.highlight_state = Some(highlight_state);
                    }
                }
            }
            if let Some(vertex_normals) = vertex_normals {
                for (projected, normal) in projected.iter_mut().zip(vertex_normals) {
                    if let Some(projected) = projected {
                        projected.normal =
                            ppc_q3_software_transform_normal(normal, command.local_to_world);
                    }
                }
            }
            if let Some(vertex_alphas) =
                self.q3_software_trimesh_vertex_alphas(&command, points.len())
            {
                for (projected, alpha) in projected.iter_mut().zip(vertex_alphas) {
                    if let Some(projected) = projected {
                        projected.vertex_alpha = Some(alpha);
                    }
                }
            }
            let triangles = self.q3_software_trimesh_triangles(&command, points.len());
            let depth_clip = command.camera.is_some();
            let backfacing_style = ppc_q3_software_backfacing_style(&command.material);
            let orientation_style = ppc_q3_software_orientation_style(&command.material);
            let interpolation_style = ppc_q3_software_interpolation_style(&command.material);
            let fill_style = ppc_q3_software_fill_style(&command.material);

            stats.commands = stats.commands.saturating_add(1);
            stats.vertices = stats.vertices.saturating_add(points.len());
            if triangles.is_empty() {
                for vertex in projected.into_iter().flatten() {
                    if !ppc_q3_software_projected_vertex_depth_visible(depth_clip, vertex) {
                        continue;
                    }
                    let point = ppc_q3_software_projected_vertex_to_point(vertex);
                    if ppc_q3_software_primitive_deferred_for_transparency(&material, &[point]) {
                        deferred_primitives.push(PpcQ3SoftwareDeferredPrimitive::Point {
                            point,
                            material,
                            lights: command.lights.clone(),
                            sort_depth: point.z,
                        });
                    } else if ppc_q3_write_software_depth_material_pixel(
                        &mut self.memory,
                        surface,
                        &mut depth_buffer,
                        point,
                        &material,
                        &command.lights,
                    ) {
                        stats.pixels = stats.pixels.saturating_add(1);
                    }
                }
            } else {
                let triangle_diffuse_colors =
                    self.q3_software_trimesh_triangle_diffuse_colors(&command);
                let triangle_ambient_coefficients = if uses_ambient_coefficients {
                    self.q3_software_trimesh_triangle_ambient_coefficients(&command)
                } else {
                    None
                };
                let triangle_specular_colors = if uses_specular_attributes {
                    self.q3_software_trimesh_triangle_specular_colors(&command)
                } else {
                    None
                };
                let triangle_specular_controls = if uses_specular_attributes {
                    self.q3_software_trimesh_triangle_specular_controls(&command)
                } else {
                    None
                };
                let triangle_highlight_states = if uses_specular_attributes {
                    self.q3_software_trimesh_triangle_highlight_states(&command)
                } else {
                    None
                };
                let triangle_normals = if uses_normals {
                    self.q3_software_trimesh_triangle_normals(&command)
                } else {
                    None
                };
                let triangle_alphas = self.q3_software_trimesh_triangle_alphas(&command);
                let edge_data = if fill_style == PPC_Q3_FILL_STYLE_EDGES {
                    let edges = self.q3_software_trimesh_edges(&command, points.len());
                    let edge_indices = ppc_q3_software_edge_index_map(&edges);
                    Some((
                        edge_indices,
                        self.q3_software_trimesh_edge_diffuse_colors(&command),
                        if uses_ambient_coefficients {
                            self.q3_software_trimesh_edge_ambient_coefficients(&command)
                        } else {
                            None
                        },
                        if uses_specular_attributes {
                            self.q3_software_trimesh_edge_specular_colors(&command)
                        } else {
                            None
                        },
                        if uses_specular_attributes {
                            self.q3_software_trimesh_edge_specular_controls(&command)
                        } else {
                            None
                        },
                        if uses_specular_attributes {
                            self.q3_software_trimesh_edge_highlight_states(&command)
                        } else {
                            None
                        },
                        if uses_normals {
                            self.q3_software_trimesh_edge_normals(&command)
                                .and_then(|normals| {
                                    normals
                                        .into_iter()
                                        .map(|normal| {
                                            ppc_q3_software_transform_normal(
                                                normal,
                                                command.local_to_world,
                                            )
                                        })
                                        .collect::<Option<Vec<_>>>()
                                })
                        } else {
                            None
                        },
                        self.q3_software_trimesh_edge_alphas(&command),
                    ))
                } else {
                    None
                };
                for triangle in triangles {
                    let Some(a) = projected.get(triangle.points[0]).copied().flatten() else {
                        continue;
                    };
                    let Some(b) = projected.get(triangle.points[1]).copied().flatten() else {
                        continue;
                    };
                    let Some(c) = projected.get(triangle.points[2]).copied().flatten() else {
                        continue;
                    };
                    let mut vertices = [a, b, c];
                    if let Some(diffuse) = triangle_diffuse_colors
                        .as_ref()
                        .and_then(|colors| colors.get(triangle.index).copied())
                    {
                        for vertex in &mut vertices {
                            if vertex.diffuse.is_none() {
                                vertex.diffuse = Some(diffuse);
                            }
                        }
                    }
                    if let Some(alpha) = triangle_alphas
                        .as_ref()
                        .and_then(|alphas| alphas.get(triangle.index).copied())
                    {
                        for vertex in &mut vertices {
                            if vertex.vertex_alpha.is_none() {
                                vertex.vertex_alpha = Some(alpha);
                            }
                        }
                    }
                    if let Some(ambient_coefficient) = triangle_ambient_coefficients
                        .as_ref()
                        .and_then(|coefficients| coefficients.get(triangle.index).copied())
                    {
                        for vertex in &mut vertices {
                            if vertex.ambient_coefficient.is_none() {
                                vertex.ambient_coefficient = Some(ambient_coefficient);
                            }
                        }
                    }
                    if let Some(normal) = triangle_normals
                        .as_ref()
                        .and_then(|normals| normals.get(triangle.index).copied())
                        .and_then(|normal| {
                            ppc_q3_software_transform_normal(normal, command.local_to_world)
                        })
                    {
                        for vertex in &mut vertices {
                            if vertex.normal.is_none() {
                                vertex.normal = Some(normal);
                            }
                        }
                    }
                    if let Some(specular_color) = triangle_specular_colors
                        .as_ref()
                        .and_then(|colors| colors.get(triangle.index).copied())
                    {
                        for vertex in &mut vertices {
                            if vertex.specular_color.is_none() {
                                vertex.specular_color = Some(specular_color);
                            }
                        }
                    }
                    if let Some(specular_control) = triangle_specular_controls
                        .as_ref()
                        .and_then(|controls| controls.get(triangle.index).copied())
                    {
                        for vertex in &mut vertices {
                            if vertex.specular_control.is_none() {
                                vertex.specular_control = Some(specular_control);
                            }
                        }
                    }
                    if let Some(highlight_state) = triangle_highlight_states
                        .as_ref()
                        .and_then(|states| states.get(triangle.index).copied())
                    {
                        for vertex in &mut vertices {
                            if vertex.highlight_state.is_none() {
                                vertex.highlight_state = Some(highlight_state);
                            }
                        }
                    }
                    if uses_normals && vertices.iter().any(|vertex| vertex.normal.is_none()) {
                        // An unannotated TriMesh still has a geometric surface normal.
                        // Generate it in object space so it follows the same orientation
                        // and inverse-transpose transform as explicit normal attributes.
                        let normal = ppc_q3_software_geometric_normal(
                            triangle.points.map(|index| points[index]),
                            ppc_q3_software_orientation_style(&command.material),
                        )
                        .and_then(|normal| {
                            ppc_q3_software_transform_normal(normal, command.local_to_world)
                        });
                        for vertex in &mut vertices {
                            if vertex.normal.is_none() {
                                vertex.normal = normal;
                            }
                        }
                    }
                    let clipped = ppc_q3_software_clip_projected_triangle(
                        command.camera,
                        front_buffer,
                        viewport,
                        vertices,
                    );
                    if clipped.len() < 3 {
                        continue;
                    }
                    let mut clipped_points: Vec<PpcQ3SoftwareProjectedPoint> = clipped
                        .iter()
                        .copied()
                        .map(ppc_q3_software_projected_vertex_to_point)
                        .collect();
                    if ppc_q3_software_culls_backfacing(
                        backfacing_style,
                        orientation_style,
                        &clipped_points,
                    ) {
                        continue;
                    }
                    ppc_q3_software_flip_backfacing_normals(
                        backfacing_style,
                        orientation_style,
                        &mut clipped_points,
                    );
                    match fill_style {
                        PPC_Q3_FILL_STYLE_POINTS => {
                            for point in clipped_points {
                                if ppc_q3_software_primitive_deferred_for_transparency(
                                    &material,
                                    &[point],
                                ) {
                                    deferred_primitives.push(
                                        PpcQ3SoftwareDeferredPrimitive::Point {
                                            point,
                                            material,
                                            lights: command.lights.clone(),
                                            sort_depth: point.z,
                                        },
                                    );
                                } else if ppc_q3_write_software_depth_material_pixel(
                                    &mut self.memory,
                                    surface,
                                    &mut depth_buffer,
                                    point,
                                    &material,
                                    &command.lights,
                                ) {
                                    stats.pixels = stats.pixels.saturating_add(1);
                                }
                            }
                        }
                        PPC_Q3_FILL_STYLE_EDGES => {
                            let Some((
                                edge_indices,
                                edge_diffuse_colors,
                                edge_ambient_coefficients,
                                edge_specular_colors,
                                edge_specular_controls,
                                edge_highlight_states,
                                edge_normals,
                                edge_alphas,
                            )) = edge_data.as_ref()
                            else {
                                continue;
                            };
                            let edge_materials = ppc_q3_software_triangle_edge_materials(
                                edge_indices,
                                edge_diffuse_colors.as_deref(),
                                edge_ambient_coefficients.as_deref(),
                                edge_specular_colors.as_deref(),
                                edge_specular_controls.as_deref(),
                                edge_highlight_states.as_deref(),
                                edge_normals.as_deref(),
                                edge_alphas.as_deref(),
                                triangle.points,
                            );
                            stats.pixels = stats.pixels.saturating_add(
                                ppc_q3_draw_or_defer_software_edge_loop(
                                    &mut self.memory,
                                    surface,
                                    &clipped_points,
                                    &material,
                                    &command.lights,
                                    &edge_materials,
                                    &mut deferred_primitives,
                                ),
                            );
                        }
                        _ => {
                            for index in 1..clipped_points.len().saturating_sub(1) {
                                let fan_vertices = [
                                    clipped_points[0],
                                    clipped_points[index],
                                    clipped_points[index + 1],
                                ];
                                if ppc_q3_software_primitive_deferred_for_transparency(
                                    &material,
                                    &fan_vertices,
                                ) {
                                    deferred_primitives.push(
                                        PpcQ3SoftwareDeferredPrimitive::Triangle {
                                            vertices: fan_vertices,
                                            material,
                                            lights: command.lights.clone(),
                                            interpolation_style,
                                            sort_depth: ppc_q3_software_triangle_sort_depth(
                                                fan_vertices,
                                            ),
                                        },
                                    );
                                } else {
                                    stats.pixels =
                                        stats.pixels.saturating_add(ppc_q3_draw_software_triangle(
                                            &mut self.memory,
                                            surface,
                                            &mut depth_buffer,
                                            fan_vertices,
                                            &material,
                                            &command.lights,
                                            interpolation_style,
                                            viewport,
                                            true,
                                        ));
                                }
                            }
                        }
                    }
                    stats.triangles = stats.triangles.saturating_add(1);
                }
            }
        }
        deferred_primitives.sort_by(|a, b| {
            b.sort_depth()
                .partial_cmp(&a.sort_depth())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for primitive in deferred_primitives {
            stats.pixels = stats.pixels.saturating_add(match primitive {
                PpcQ3SoftwareDeferredPrimitive::Triangle {
                    vertices,
                    material,
                    lights,
                    interpolation_style,
                    ..
                } => ppc_q3_draw_software_triangle(
                    &mut self.memory,
                    surface,
                    &mut depth_buffer,
                    vertices,
                    &material,
                    &lights,
                    interpolation_style,
                    viewport,
                    false,
                ),
                PpcQ3SoftwareDeferredPrimitive::Point {
                    point,
                    material,
                    lights,
                    ..
                } => usize::from(ppc_q3_draw_deferred_software_point(
                    &mut self.memory,
                    surface,
                    &mut depth_buffer,
                    point,
                    &material,
                    &lights,
                )),
                PpcQ3SoftwareDeferredPrimitive::Edge {
                    start,
                    end,
                    material,
                    lights,
                    ..
                } => ppc_q3_draw_deferred_software_material_line(
                    &mut self.memory,
                    surface,
                    &depth_buffer,
                    start,
                    end,
                    &material,
                    &lights,
                ),
            });
        }
        stats
    }

    fn q3_completed_frame_scene_commands(
        &mut self,
        frame: &PpcQ3CompletedFrameRecord,
    ) -> Vec<PpcQ3SceneCommand> {
        self.q3_scene_commands_from_slices(
            &frame.submissions,
            &frame.submission_transforms,
            &frame.submission_materials,
            &frame.submission_lights,
            &frame.retained_trimeshes,
        )
    }

    fn q3_completed_frame_render_target(
        &self,
        frame: &PpcQ3CompletedFrameRecord,
    ) -> Option<PpcQ3RenderTarget> {
        ppc_q3_completed_frame_render_target(
            &self.q3_objects,
            &self.q3_views,
            &self.q3_draw_contexts,
            &self.gworlds,
            *self.current_gworld,
            frame,
        )
    }

    fn q3_scene_command_from_slices(
        &mut self,
        index: usize,
        submission: &PpcQ3SubmissionRecord,
        transforms: &[PpcQ3SubmissionTransformRecord],
        materials: &[PpcQ3SubmissionMaterialRecord],
        lights: &[PpcQ3SubmissionLightRecord],
        retained_trimeshes: &[PpcQ3TriMeshRecord],
    ) -> Option<PpcQ3SceneCommand> {
        let view_state = self
            .q3_views
            .iter()
            .find(|record| record.view == submission.view)
            .copied()?;
        let transform = Self::q3_submission_transform_from_slice(transforms, index, submission)?;
        let material = Self::q3_submission_material_from_slice(materials, index, submission)?;
        let lights = Self::q3_submission_lights_from_slice(lights, index, submission)?;
        match submission.kind {
            PpcQ3SubmissionKind::TriMesh => {
                let geometry = self.q3_scene_trimesh_geometry(
                    submission.primary,
                    submission.secondary,
                    retained_trimeshes,
                )?;
                let camera = if view_state.camera == 0 {
                    None
                } else {
                    self.q3_cameras
                        .iter()
                        .find(|record| record.camera == view_state.camera)
                        .copied()
                };
                Some(PpcQ3SceneCommand::TriMesh(PpcQ3SceneTriMeshCommand {
                    submission_index: index,
                    view_state,
                    camera,
                    geometry,
                    local_to_world: transform.local_to_world,
                    material,
                    lights,
                }))
            }
            _ => Some(PpcQ3SceneCommand::Submission(PpcQ3SceneSubmissionCommand {
                submission_index: index,
                view_state,
                submission: *submission,
                local_to_world: transform.local_to_world,
                material,
                lights,
            })),
        }
    }

    fn q3_submission_transform_from_slice(
        transforms: &[PpcQ3SubmissionTransformRecord],
        index: usize,
        submission: &PpcQ3SubmissionRecord,
    ) -> Option<PpcQ3SubmissionTransformRecord> {
        transforms
            .get(index)
            .filter(|record| ppc_q3_submission_transform_matches(record, submission))
            .copied()
            .or_else(|| {
                transforms
                    .iter()
                    .find(|record| ppc_q3_submission_transform_matches(record, submission))
                    .copied()
            })
    }

    fn q3_submission_material_from_slice(
        materials: &[PpcQ3SubmissionMaterialRecord],
        index: usize,
        submission: &PpcQ3SubmissionRecord,
    ) -> Option<PpcQ3SubmissionMaterialRecord> {
        materials
            .get(index)
            .filter(|record| ppc_q3_submission_material_matches(record, submission))
            .cloned()
            .or_else(|| {
                materials
                    .iter()
                    .find(|record| ppc_q3_submission_material_matches(record, submission))
                    .cloned()
            })
    }

    fn q3_submission_lights_from_slice(
        lights: &[PpcQ3SubmissionLightRecord],
        index: usize,
        submission: &PpcQ3SubmissionRecord,
    ) -> Option<PpcQ3SubmissionLightRecord> {
        lights
            .get(index)
            .filter(|record| ppc_q3_submission_light_matches(record, submission))
            .cloned()
            .or_else(|| {
                lights
                    .iter()
                    .find(|record| ppc_q3_submission_light_matches(record, submission))
                    .cloned()
            })
    }

    fn q3_scene_trimesh_geometry(
        &mut self,
        primary: u32,
        secondary: u32,
        retained_trimeshes: &[PpcQ3TriMeshRecord],
    ) -> Option<PpcQ3SceneTriMeshGeometry> {
        if let Some(record) = self.q3_immediate_trimeshes.get(secondary, primary) {
            return Some(PpcQ3SceneTriMeshGeometry {
                source: PpcQ3SceneTriMeshSource::DataPtr(primary),
                data: record.data.clone(),
                memory_regions: record.memory_regions.clone(),
            });
        }
        if let Some(object) = self
            .q3_objects
            .iter()
            .find(|record| record.object == primary && record.object_type == PPC_Q3_TYPE_TRIMESH)
            .copied()
        {
            let data = self
                .q3_trimeshes
                .iter()
                .find(|record| record.trimesh == primary)
                .map(|record| record.data.clone())
                .or_else(|| {
                    if object.data_ptr != 0 && object.data_size >= PPC_Q3_TRIMESH_DATA_SIZE {
                        self.q3_scene_read_bytes(object.data_ptr, PPC_Q3_TRIMESH_DATA_SIZE)
                    } else {
                        None
                    }
                })?;
            return Some(PpcQ3SceneTriMeshGeometry {
                source: PpcQ3SceneTriMeshSource::Object(primary),
                data,
                memory_regions: Vec::new(),
            });
        }
        if let Some(record) = retained_trimeshes
            .iter()
            .find(|record| record.trimesh == primary)
        {
            return Some(PpcQ3SceneTriMeshGeometry {
                source: PpcQ3SceneTriMeshSource::Object(primary),
                data: record.data.clone(),
                memory_regions: Vec::new(),
            });
        }
        self.q3_scene_read_bytes(primary, PPC_Q3_TRIMESH_DATA_SIZE)
            .map(|data| PpcQ3SceneTriMeshGeometry {
                source: PpcQ3SceneTriMeshSource::DataPtr(primary),
                data,
                memory_regions: Vec::new(),
            })
    }

    fn q3_scene_read_bytes(&mut self, source_ptr: u32, byte_count: u32) -> Option<Vec<u8>> {
        let mut data = Vec::with_capacity(byte_count as usize);
        for offset in 0..byte_count {
            data.push(self.memory.read_u8(source_ptr.checked_add(offset)?)?);
        }
        Some(data)
    }

    fn capture_q3_trimesh_replay_memory(
        &mut self,
        memory_regions: &mut Vec<PpcQ3SceneReplayMemoryRegion>,
        command: &PpcQ3SceneTriMeshCommand,
    ) {
        if !command.geometry.memory_regions.is_empty() {
            // The command already carries its submit-time arrays.
            return;
        }
        for region in
            ppc_q3_capture_trimesh_memory_regions(&mut self.memory, &command.geometry.data)
        {
            if !memory_regions.iter().any(|existing| {
                existing.base_addr == region.base_addr && existing.data.len() >= region.data.len()
            }) {
                memory_regions.retain(|existing| existing.base_addr != region.base_addr);
                memory_regions.push(region);
            }
        }
    }

    fn capture_q3_material_replay_memory(
        &mut self,
        memory_regions: &mut Vec<PpcQ3SceneReplayMemoryRegion>,
        material: &PpcQ3SubmissionMaterialRecord,
    ) {
        let Some(texture) =
            ppc_q3_software_material_texture(&self.q3_objects, &self.q3_memory_storages, material)
        else {
            return;
        };
        let Some(last_row_offset) = texture
            .height
            .checked_sub(1)
            .and_then(|height| height.checked_mul(texture.row_bytes))
        else {
            return;
        };
        let Some(used_row_bytes) = texture.width.checked_mul(texture.pixel_bytes) else {
            return;
        };
        let Some(byte_count) = last_row_offset.checked_add(used_row_bytes) else {
            return;
        };
        let Some(base_addr) = texture.image_base.checked_add(texture.image_offset) else {
            return;
        };
        self.capture_q3_replay_memory_region(memory_regions, base_addr, byte_count);
    }

    fn capture_q3_replay_memory_region(
        &mut self,
        memory_regions: &mut Vec<PpcQ3SceneReplayMemoryRegion>,
        base_addr: u32,
        byte_count: u32,
    ) {
        ppc_q3_capture_memory_region(&mut self.memory, memory_regions, base_addr, byte_count);
    }

    fn install_q3_scene_replay_memory_region(&mut self, region: &PpcQ3SceneReplayMemoryRegion) {
        if region.base_addr == 0 || region.data.is_empty() {
            return;
        }
        if ppc_memory_can_write_bytes(
            &mut self.memory,
            region.base_addr,
            u32::try_from(region.data.len()).unwrap_or(u32::MAX),
        ) && ppc_q3_write_bytes(&mut self.memory, region.base_addr, &region.data)
        {
            return;
        }
        self.memory
            .add_region(region.base_addr, region.data.clone());
    }

    /// Read trimesh geometry from the submit-time copy when the command has
    /// one, and from live guest memory otherwise.
    fn q3_geometry_read_u32(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        addr: u32,
    ) -> Option<u32> {
        ppc_q3_captured_u32(regions, addr).or_else(|| self.memory.read_u32_be(addr))
    }

    fn q3_geometry_read_f32(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        addr: u32,
    ) -> Option<f32> {
        self.q3_geometry_read_u32(regions, addr).map(f32::from_bits)
    }

    fn q3_geometry_read_vector2d(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        addr: u32,
    ) -> Option<(f32, f32)> {
        Some((
            self.q3_geometry_read_f32(regions, addr)?,
            self.q3_geometry_read_f32(regions, addr.checked_add(4)?)?,
        ))
    }

    fn q3_geometry_read_vector3d(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        addr: u32,
    ) -> Option<(f32, f32, f32)> {
        Some((
            self.q3_geometry_read_f32(regions, addr)?,
            self.q3_geometry_read_f32(regions, addr.checked_add(4)?)?,
            self.q3_geometry_read_f32(regions, addr.checked_add(8)?)?,
        ))
    }

    /// Drop submit-time trimesh copies once no queued, completed, or
    /// retained frame names them.
    fn prune_q3_immediate_trimeshes(&mut self) {
        if self.q3_immediate_trimeshes.is_empty() {
            return;
        }
        let submissions = self
            .q3_submissions
            .iter()
            .chain(
                self.q3_completed_frames
                    .iter()
                    .flat_map(|frame| frame.submissions.iter()),
            )
            .chain(
                self.q3_retained_frames
                    .iter()
                    .flat_map(|record| record.frame.submissions.iter()),
            );
        self.q3_immediate_trimeshes.retain_referenced(submissions);
    }

    fn q3_software_trimesh_points(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_POINTS_OFFSET)?;
        let points_ptr =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_POINTS_OFFSET)?;
        if count == 0 || count > PPC_Q3_SOFTWARE_RENDER_MAX_POINTS || points_ptr == 0 {
            return None;
        }
        let mut points = Vec::with_capacity(count as usize);
        for index in 0..count {
            let point_ptr = points_ptr.checked_add(index.checked_mul(PPC_Q3_POINT3D_SIZE)?)?;
            point_ptr.checked_add(PPC_Q3_POINT3D_SIZE.saturating_sub(1))?;
            let point = self.q3_geometry_read_vector3d(regions, point_ptr)?;
            if !ppc_q3_point_finite(point) {
                return None;
            }
            points.push(point);
        }
        Some(points)
    }

    fn q3_software_trimesh_triangles(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Vec<PpcQ3SoftwareTriangle> {
        let regions = &command.geometry.memory_regions;
        let Some(count) =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)
        else {
            return Vec::new();
        };
        let Some(triangles_ptr) =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_TRIANGLES_OFFSET)
        else {
            return Vec::new();
        };
        if count == 0 || count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES || triangles_ptr == 0 {
            return Vec::new();
        }
        let mut triangles = Vec::with_capacity(count as usize);
        for index in 0..count {
            let Some(triangle_offset) = index.checked_mul(PPC_Q3_TRIMESH_TRIANGLE_DATA_SIZE) else {
                break;
            };
            let Some(triangle_ptr) = triangles_ptr.checked_add(triangle_offset) else {
                break;
            };
            let Some(a) = self.q3_geometry_read_u32(regions, triangle_ptr) else {
                continue;
            };
            let Some(b_ptr) = triangle_ptr.checked_add(4) else {
                continue;
            };
            let Some(c_ptr) = triangle_ptr.checked_add(8) else {
                continue;
            };
            let Some(b) = self.q3_geometry_read_u32(regions, b_ptr) else {
                continue;
            };
            let Some(c) = self.q3_geometry_read_u32(regions, c_ptr) else {
                continue;
            };
            let Ok(a) = usize::try_from(a) else {
                continue;
            };
            let Ok(b) = usize::try_from(b) else {
                continue;
            };
            let Ok(c) = usize::try_from(c) else {
                continue;
            };
            if a < point_count && b < point_count && c < point_count {
                let Ok(index) = usize::try_from(index) else {
                    continue;
                };
                triangles.push(PpcQ3SoftwareTriangle {
                    index,
                    points: [a, b, c],
                });
            }
        }
        triangles
    }

    fn q3_software_trimesh_edges(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Vec<PpcQ3SoftwareEdge> {
        let regions = &command.geometry.memory_regions;
        let Some(count) =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)
        else {
            return Vec::new();
        };
        let Some(edges_ptr) =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_EDGES_OFFSET)
        else {
            return Vec::new();
        };
        if count == 0 || count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES || edges_ptr == 0 {
            return Vec::new();
        }
        let mut edges = Vec::with_capacity(count as usize);
        for index in 0..count {
            let Some(edge_offset) = index.checked_mul(PPC_Q3_TRIMESH_EDGE_DATA_SIZE) else {
                break;
            };
            let Some(edge_ptr) = edges_ptr.checked_add(edge_offset) else {
                break;
            };
            let Some(a) = self.q3_geometry_read_u32(regions, edge_ptr) else {
                continue;
            };
            let Some(b_ptr) = edge_ptr.checked_add(4) else {
                continue;
            };
            let Some(b) = self.q3_geometry_read_u32(regions, b_ptr) else {
                continue;
            };
            let Ok(a) = usize::try_from(a) else {
                continue;
            };
            let Ok(b) = usize::try_from(b) else {
                continue;
            };
            if a < point_count && b < point_count {
                let Ok(index) = usize::try_from(index) else {
                    continue;
                };
                edges.push(PpcQ3SoftwareEdge {
                    index,
                    points: [a, b],
                });
            }
        }
        edges
    }

    fn q3_software_trimesh_triangle_diffuse_colors(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let triangle_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if triangle_count == 0
            || triangle_count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR && data_ptr != 0 {
                let triangle_count = usize::try_from(triangle_count).ok()?;
                return self.q3_software_read_vertex_colors(regions, data_ptr, triangle_count);
            }
        }
        None
    }

    fn q3_software_trimesh_triangle_ambient_coefficients(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let triangle_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if triangle_count == 0
            || triangle_count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT && data_ptr != 0 {
                let triangle_count = usize::try_from(triangle_count).ok()?;
                return self.q3_software_read_vertex_unit_scalars(
                    regions,
                    data_ptr,
                    triangle_count,
                );
            }
        }
        None
    }

    fn q3_software_trimesh_triangle_normals(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let triangle_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if triangle_count == 0
            || triangle_count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_NORMAL && data_ptr != 0 {
                let triangle_count = usize::try_from(triangle_count).ok()?;
                return self.q3_software_read_vertex_normals(regions, data_ptr, triangle_count);
            }
        }
        None
    }

    fn q3_software_trimesh_triangle_highlight_states(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<bool>> {
        let regions = &command.geometry.memory_regions;
        let triangle_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if triangle_count == 0
            || triangle_count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE && data_ptr != 0 {
                let triangle_count = usize::try_from(triangle_count).ok()?;
                return self.q3_software_read_vertex_switches(regions, data_ptr, triangle_count);
            }
        }
        None
    }

    fn q3_software_trimesh_edge_diffuse_colors(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let edge_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if edge_count == 0
            || edge_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR && data_ptr != 0 {
                let edge_count = usize::try_from(edge_count).ok()?;
                return self.q3_software_read_vertex_colors(regions, data_ptr, edge_count);
            }
        }
        None
    }

    fn q3_software_trimesh_edge_ambient_coefficients(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let edge_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if edge_count == 0
            || edge_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT && data_ptr != 0 {
                let edge_count = usize::try_from(edge_count).ok()?;
                return self.q3_software_read_vertex_unit_scalars(regions, data_ptr, edge_count);
            }
        }
        None
    }

    fn q3_software_trimesh_edge_specular_colors(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let edge_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if edge_count == 0
            || edge_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR && data_ptr != 0 {
                let edge_count = usize::try_from(edge_count).ok()?;
                return self.q3_software_read_vertex_colors(regions, data_ptr, edge_count);
            }
        }
        None
    }

    fn q3_software_trimesh_edge_specular_controls(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let edge_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if edge_count == 0
            || edge_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL && data_ptr != 0 {
                let edge_count = usize::try_from(edge_count).ok()?;
                return self.q3_software_read_vertex_scalars(regions, data_ptr, edge_count);
            }
        }
        None
    }

    fn q3_software_trimesh_edge_normals(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let edge_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if edge_count == 0
            || edge_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_NORMAL && data_ptr != 0 {
                let edge_count = usize::try_from(edge_count).ok()?;
                return self.q3_software_read_vertex_normals(regions, data_ptr, edge_count);
            }
        }
        None
    }

    fn q3_software_trimesh_edge_highlight_states(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<bool>> {
        let regions = &command.geometry.memory_regions;
        let edge_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if edge_count == 0
            || edge_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE && data_ptr != 0 {
                let edge_count = usize::try_from(edge_count).ok()?;
                return self.q3_software_read_vertex_switches(regions, data_ptr, edge_count);
            }
        }
        None
    }

    fn q3_software_trimesh_triangle_specular_colors(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let triangle_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if triangle_count == 0
            || triangle_count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR && data_ptr != 0 {
                let triangle_count = usize::try_from(triangle_count).ok()?;
                return self.q3_software_read_vertex_colors(regions, data_ptr, triangle_count);
            }
        }
        None
    }

    fn q3_software_trimesh_triangle_specular_controls(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let triangle_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if triangle_count == 0
            || triangle_count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL && data_ptr != 0 {
                let triangle_count = usize::try_from(triangle_count).ok()?;
                return self.q3_software_read_vertex_scalars(regions, data_ptr, triangle_count);
            }
        }
        None
    }

    fn q3_software_trimesh_triangle_alphas(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let triangle_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_TRIANGLES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_TRIANGLE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if triangle_count == 0
            || triangle_count > PPC_Q3_SOFTWARE_RENDER_MAX_TRIANGLES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR && data_ptr != 0 {
                let triangle_count = usize::try_from(triangle_count).ok()?;
                return self.q3_software_read_vertex_alphas(regions, data_ptr, triangle_count);
            }
        }
        None
    }

    fn q3_software_trimesh_edge_alphas(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let edge_count =
            ppc_q3_trimesh_header_u32(&command.geometry.data, PPC_Q3_TRIMESH_NUM_EDGES_OFFSET)?;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_EDGE_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if edge_count == 0
            || edge_count > PPC_Q3_SOFTWARE_RENDER_MAX_EDGES
            || attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR && data_ptr != 0 {
                let edge_count = usize::try_from(edge_count).ok()?;
                return self.q3_software_read_vertex_alphas(regions, data_ptr, edge_count);
            }
        }
        None
    }

    fn q3_software_trimesh_vertex_uvs(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<(f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        let mut shading_uvs = None;
        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if data_ptr == 0 {
                continue;
            }
            match attr_type {
                PPC_Q3_ATTRIBUTE_TYPE_SURFACE_UV => {
                    return self.q3_software_read_vertex_uvs(regions, data_ptr, point_count);
                }
                PPC_Q3_ATTRIBUTE_TYPE_SHADING_UV if shading_uvs.is_none() => {
                    shading_uvs = self.q3_software_read_vertex_uvs(regions, data_ptr, point_count);
                }
                _ => {}
            }
        }
        shading_uvs
    }

    fn q3_software_read_vertex_uvs(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        data_ptr: u32,
        point_count: usize,
    ) -> Option<Vec<(f32, f32)>> {
        let count = u32::try_from(point_count).ok()?;
        let mut uvs = Vec::with_capacity(point_count);
        for index in 0..count {
            let uv_ptr = data_ptr.checked_add(index.checked_mul(PPC_Q3_VECTOR2D_SIZE)?)?;
            let uv = self.q3_geometry_read_vector2d(regions, uv_ptr)?;
            if !uv.0.is_finite() || !uv.1.is_finite() {
                return None;
            }
            uvs.push(uv);
        }
        Some(uvs)
    }

    fn q3_software_trimesh_vertex_diffuse_colors(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_DIFFUSE_COLOR && data_ptr != 0 {
                return self.q3_software_read_vertex_colors(regions, data_ptr, point_count);
            }
        }
        None
    }

    fn q3_software_trimesh_vertex_ambient_coefficients(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_AMBIENT_COEFFICIENT && data_ptr != 0 {
                return self.q3_software_read_vertex_unit_scalars(regions, data_ptr, point_count);
            }
        }
        None
    }

    fn q3_software_read_vertex_colors(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        data_ptr: u32,
        point_count: usize,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let count = u32::try_from(point_count).ok()?;
        let mut colors = Vec::with_capacity(point_count);
        for index in 0..count {
            let color_ptr = data_ptr.checked_add(index.checked_mul(PPC_Q3_POINT3D_SIZE)?)?;
            let color = self.q3_geometry_read_vector3d(regions, color_ptr)?;
            if !ppc_q3_color_finite(color) {
                return None;
            }
            colors.push(color);
        }
        Some(colors)
    }

    fn q3_software_trimesh_vertex_specular_colors(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_COLOR && data_ptr != 0 {
                return self.q3_software_read_vertex_colors(regions, data_ptr, point_count);
            }
        }
        None
    }

    fn q3_software_trimesh_vertex_specular_controls(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_SPECULAR_CONTROL && data_ptr != 0 {
                return self.q3_software_read_vertex_scalars(regions, data_ptr, point_count);
            }
        }
        None
    }

    fn q3_software_trimesh_vertex_highlight_states(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<bool>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_HIGHLIGHT_STATE && data_ptr != 0 {
                return self.q3_software_read_vertex_switches(regions, data_ptr, point_count);
            }
        }
        None
    }

    fn q3_software_read_vertex_scalars(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        data_ptr: u32,
        point_count: usize,
    ) -> Option<Vec<f32>> {
        let count = u32::try_from(point_count).ok()?;
        let mut values = Vec::with_capacity(point_count);
        for index in 0..count {
            let scalar_ptr = data_ptr.checked_add(index.checked_mul(4)?)?;
            let value = self.q3_geometry_read_f32(regions, scalar_ptr)?;
            if !value.is_finite() {
                return None;
            }
            values.push(value.max(0.0));
        }
        Some(values)
    }

    fn q3_software_read_vertex_unit_scalars(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        data_ptr: u32,
        point_count: usize,
    ) -> Option<Vec<f32>> {
        self.q3_software_read_vertex_scalars(regions, data_ptr, point_count)
            .map(|mut values| {
                for value in &mut values {
                    *value = value.clamp(0.0, 1.0);
                }
                values
            })
    }

    fn q3_software_read_vertex_switches(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        data_ptr: u32,
        point_count: usize,
    ) -> Option<Vec<bool>> {
        let count = u32::try_from(point_count).ok()?;
        let mut values = Vec::with_capacity(point_count);
        for index in 0..count {
            let scalar_ptr = data_ptr.checked_add(index.checked_mul(4)?)?;
            let value = self.q3_geometry_read_u32(regions, scalar_ptr)?;
            values.push(value != 0);
        }
        Some(values)
    }

    fn q3_software_trimesh_vertex_normals(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_NORMAL && data_ptr != 0 {
                return self.q3_software_read_vertex_normals(regions, data_ptr, point_count);
            }
        }
        None
    }

    fn q3_software_read_vertex_normals(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        data_ptr: u32,
        point_count: usize,
    ) -> Option<Vec<(f32, f32, f32)>> {
        let count = u32::try_from(point_count).ok()?;
        let mut normals = Vec::with_capacity(point_count);
        for index in 0..count {
            let normal_ptr = data_ptr.checked_add(index.checked_mul(PPC_Q3_POINT3D_SIZE)?)?;
            let normal = self.q3_geometry_read_vector3d(regions, normal_ptr)?;
            normals.push(ppc_q3_vector3d_normalized_value(normal)?);
        }
        Some(normals)
    }

    fn q3_software_trimesh_vertex_alphas(
        &mut self,
        command: &PpcQ3SceneTriMeshCommand,
        point_count: usize,
    ) -> Option<Vec<f32>> {
        let regions = &command.geometry.memory_regions;
        let attr_count = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_NUM_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        let attr_ptr = ppc_q3_trimesh_header_u32(
            &command.geometry.data,
            PPC_Q3_TRIMESH_VERTEX_ATTRIBUTE_TYPES_OFFSET,
        )?;
        if attr_count == 0
            || attr_count > PPC_Q3_SOFTWARE_RENDER_MAX_ATTRIBUTE_TYPES
            || attr_ptr == 0
        {
            return None;
        }

        for index in 0..attr_count {
            let entry_ptr =
                attr_ptr.checked_add(index.checked_mul(PPC_Q3_TRIMESH_ATTRIBUTE_DATA_SIZE)?)?;
            let attr_type = self.q3_geometry_read_u32(regions, entry_ptr)?;
            let data_ptr = self.q3_geometry_read_u32(regions, entry_ptr.checked_add(4)?)?;
            if attr_type == PPC_Q3_ATTRIBUTE_TYPE_TRANSPARENCY_COLOR && data_ptr != 0 {
                return self.q3_software_read_vertex_alphas(regions, data_ptr, point_count);
            }
        }
        None
    }

    fn q3_software_read_vertex_alphas(
        &mut self,
        regions: &[PpcQ3SceneReplayMemoryRegion],
        data_ptr: u32,
        point_count: usize,
    ) -> Option<Vec<f32>> {
        let count = u32::try_from(point_count).ok()?;
        let mut alphas = Vec::with_capacity(point_count);
        for index in 0..count {
            let color_ptr = data_ptr.checked_add(index.checked_mul(PPC_Q3_POINT3D_SIZE)?)?;
            let color = self.q3_geometry_read_vector3d(regions, color_ptr)?;
            alphas.push(ppc_q3_software_transparency_color_alpha(color)?);
        }
        Some(alphas)
    }
}
