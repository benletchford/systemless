//! PowerPC Virtual File System (VFS) seeding and dirty state export methods on [`PpcLoadedApp`].

use super::*;

impl PpcLoadedApp {
    pub fn seed_vfs_directories(
        &mut self,
        directories: Vec<PpcVfsDirectory>,
        default_dir_id: u32,
        next_dir_id: u32,
    ) {
        let max_seeded_dir_id = directories
            .iter()
            .map(|directory| directory.dir_id)
            .max()
            .unwrap_or(PPC_ROOT_DIR_ID);
        self.vfs_directories.replace(directories);
        self.default_dir_id
            .with_mut(|current_dir_id| *current_dir_id = default_dir_id);
        let _ = self
            .memory
            .write_u32_be(crate::memory::globals::addr::CUR_DIR_STORE, default_dir_id);
        let next_dir_id = next_dir_id
            .max(max_seeded_dir_id.saturating_add(1))
            .max(PPC_FIRST_DYNAMIC_DIR_ID);
        self.next_vfs_dir_id
            .with_mut(|cursor| *cursor = next_dir_id);
    }

    pub fn seed_vfs_volumes(&mut self, volumes: Vec<PpcVfsVolumeRecord>) {
        let next_ref_num = volumes
            .iter()
            .map(|volume| volume.ref_num)
            .min()
            .unwrap_or(PPC_BOOT_VOLUME_REF_NUM)
            .saturating_sub(1)
            .min(-2);
        self.next_vfs_volume_ref_num
            .with_mut(|cursor| *cursor = next_ref_num);
        self.vfs_volumes.replace(volumes);
    }

    pub fn set_launched_app_path(&mut self, path: impl Into<String>) {
        let path = ppc_normalize_vfs_path(&path.into());
        self.process_file_system
            .with_mut(|file_system| file_system.launched_app_path = Some(path));
        self.refresh_apple_event_launch_capability();
    }

    pub fn launched_app_path(&self) -> Option<&str> {
        self.process_file_system.launched_app_path.as_deref()
    }

    pub fn seed_cfm_library_fragments(&mut self, fragments: Vec<PpcCfmLibraryFragment>) {
        self.cfm
            .as_mut()
            .expect("seed CFM libraries before process installation")
            .library_fragments = fragments;
    }

    pub fn seed_vfs_files_and_resources(
        &mut self,
        files: Vec<PpcVfsFileRecord>,
        resource_files: Vec<PpcVfsResourceFileRecord>,
        resources: Vec<PpcVfsResourceRecord>,
    ) {
        ppc_register_vfs_resource_fonts(&resources);
        self.process_file_system.with_mut(|file_system| {
            file_system.vfs_files.replace(files);
            file_system.deleted_vfs_file_paths.clear();
            file_system.with_resource_manager_mut(|resource_manager| {
                resource_manager.vfs_resource_files.replace(resource_files);
                resource_manager.vfs_resources = resources;
                ppc_publish_resource_fork_bytes(
                    &mut resource_manager.vfs_resource_files,
                    &resource_manager.vfs_resources,
                    false,
                );
            });
        });
        self.refresh_apple_event_launch_capability();
    }

    pub(crate) fn launch_size_resource(&self) -> Option<ApplicationSizeResource> {
        self.launched_app_path().and_then(|path| {
            [0, -1].into_iter().find_map(|id| {
                self.vfs_resources
                    .iter()
                    .find(|resource| {
                        resource.path.eq_ignore_ascii_case(path)
                            && resource.res_type == u32::from_be_bytes(*b"SIZE")
                            && resource.res_id == id
                    })
                    .and_then(|resource| ApplicationSizeResource::parse(&resource.data))
                    .filter(|size| size.preferred_partition_size().is_some())
            })
        })
    }

    fn refresh_apple_event_launch_capability(&mut self) {
        let size_resource = self.launch_size_resource();
        self.application_size.with_mut(|current| *current = size_resource);
        let high_level_event_aware =
            size_resource.is_some_and(ApplicationSizeResource::is_high_level_event_aware);
        self.apple_events
            .apple_event_launch_state
            .set_high_level_event_aware(high_level_event_aware);
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] launch AppleEvents aware={} path={:?} SIZE={:?}",
                high_level_event_aware,
                self.launched_app_path(),
                size_resource,
            );
        }
    }

    /// Prepare resource forks extracted by InstallerMaker for native launch.
    ///
    /// InstallerMaker can leave an ordinary resource fork on the destination
    /// file while storing additional resources in a shared `qDir`/data-fork
    /// pair. Its installed forks can also contain compressed resources that
    /// rely on the installer to decode them. The in-process Resource Manager
    /// handles both cases lazily; native launchers need complete, decoded
    /// forks before handing the tree to a real Mac environment.
    pub fn prepare_vfs_resource_forks_for_native_export(&mut self) -> usize {
        self.process_file_system.with_mut(|file_system| {
            let ProcessFileSystemState {
                vfs_files,
                resource_manager,
                ..
            } = file_system;
            resource_manager.with_mut(|resource_manager| {
                let ProcessResourceManagerState {
                    vfs_resource_files,
                    vfs_resources,
                    ..
                } = resource_manager;
                let paths = vfs_resource_files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>();
                for path in &paths {
                    ppc_materialize_resource_records_for_path(
                        vfs_resource_files,
                        vfs_resources,
                        path,
                    );
                }
                for path in &paths {
                    ppc_materialize_quilt_resources_for_existing_path(
                        vfs_files,
                        vfs_resource_files,
                        vfs_resources,
                        path,
                    );
                }

                let mut prepared_count = 0usize;
                for path in paths {
                    let has_parseable_fork = vfs_resource_files
                        .iter()
                        .find(|file| file.path.eq_ignore_ascii_case(&path))
                        .and_then(|file| file.raw_data.as_deref())
                        .and_then(|bytes| ResourceFork::parse(bytes))
                        .is_some();
                    let mut path_resource_count = 0usize;
                    for resource in vfs_resources
                        .iter_mut()
                        .filter(|resource| resource.path.eq_ignore_ascii_case(&path))
                    {
                        resource.raw_data = None;
                        resource.raw_attrs = None;
                        path_resource_count += 1;
                    }
                    if !has_parseable_fork && path_resource_count == 0 {
                        continue;
                    }
                    ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
                    prepared_count += 1;
                }
                prepared_count
            })
        })
    }

    pub fn take_deleted_vfs_file_paths(&mut self) -> Vec<String> {
        self.process_file_system.with_mut(|file_system| {
            file_system.publish_native_vfs_catalogue();
            std::mem::take(&mut file_system.deleted_vfs_file_paths)
        })
    }

    pub fn take_dirty_vfs_files(&mut self) -> Vec<PpcVfsFileExport> {
        self.process_file_system.with_mut(|file_system| {
            let mut exports = Vec::new();
            for file in file_system.vfs_files.iter_mut().filter(|file| {
                file.dirty
                    && !file.path.is_empty()
                    && !file.path.starts_with(PPC_OPEN_RESOURCE_FORK_PREFIX)
            }) {
                exports.push(PpcVfsFileExport {
                    path: file.path.clone(),
                    data: file.data.shared_handle(),
                    creator: file.creator,
                    file_type: file.file_type,
                    finder_flags: file.finder_flags,
                });
                file.dirty = false;
            }
            exports
        })
    }

    pub fn take_dirty_vfs_directories(&mut self) -> Vec<PpcVfsDirectoryExport> {
        self.vfs_directories.with_mut(|directories| {
            let mut exports = Vec::new();
            for directory in directories
                .iter_mut()
                .filter(|directory| directory.dirty && !directory.path.is_empty())
            {
                exports.push(PpcVfsDirectoryExport {
                    path: directory.path.clone(),
                    creator: directory.creator,
                    file_type: directory.file_type,
                    finder_flags: directory.finder_flags,
                });
                directory.dirty = false;
            }
            exports
        })
    }

    pub fn take_dirty_vfs_resource_forks(&mut self) -> Vec<PpcVfsResourceForkExport> {
        self.process_file_system
            .with_resource_manager_mut(|resource_manager| {
                let mut exports = Vec::new();
                let ProcessResourceManagerState {
                    vfs_resource_files,
                    vfs_resources,
                    ..
                } = resource_manager;
                ppc_publish_resource_fork_bytes(vfs_resource_files, vfs_resources, true);
                let dirty_indices = vfs_resource_files
                    .iter()
                    .enumerate()
                    .filter_map(|(index, file)| {
                        (file.dirty && !file.path.is_empty()).then_some(index)
                    })
                    .collect::<Vec<_>>();
                for index in dirty_indices {
                    let file = &vfs_resource_files[index];
                    let path = file.path.clone();
                    let data = vfs_resource_files.fork(&path).map(|bytes| bytes.to_vec());
                    if let Some(data) = data {
                        exports.push(PpcVfsResourceForkExport {
                            path,
                            data,
                            creator: file.creator,
                            file_type: file.file_type,
                            finder_flags: file.finder_flags,
                        });
                        vfs_resource_files[index].dirty = false;
                    }
                }
                exports
            })
    }
}

#[cfg(test)]
impl PpcLoadedApp {
    pub(crate) fn push_test_vfs_file(&mut self, file: PpcVfsFileRecord) {
        self.process_file_system
            .with_mut(|file_system| file_system.vfs_files.push(file));
    }

    pub(crate) fn push_test_open_file(&mut self, file: PpcFileRecord) {
        self.process_file_system
            .with_mut(|file_system| file_system.files.push(file));
    }

    pub(crate) fn push_test_deleted_vfs_file_path(&mut self, path: String) {
        self.process_file_system
            .with_mut(|file_system| file_system.deleted_vfs_file_paths.push(path));
    }

    pub(crate) fn set_test_next_file_ref_num(&mut self, next_file_ref_num: i16) {
        self.process_file_system.with_mut(|file_system| {
            file_system.next_file_ref_num = next_file_ref_num;
        });
    }

    pub(crate) fn with_test_vfs_file_mut<R>(
        &mut self,
        index: usize,
        operation: impl FnOnce(&mut PpcVfsFileRecord) -> R,
    ) -> Option<R> {
        self.process_file_system
            .with_mut(|file_system| file_system.vfs_files.get_mut(index).map(operation))
    }

    pub(crate) fn with_test_open_file_mut<R>(
        &mut self,
        index: usize,
        operation: impl FnOnce(&mut PpcFileRecord) -> R,
    ) -> Option<R> {
        self.process_file_system
            .with_mut(|file_system| file_system.files.with_record_mut(index, operation))
    }

    pub(crate) fn insert_test_stdio_stream(
        &mut self,
        address: u32,
        stream: crate::process_context::ProcessStdioStreamRecord,
    ) {
        self.process_file_system
            .with_mut(|file_system| file_system.stdio_streams.insert(address, stream));
    }

    pub(crate) fn publish_test_native_vfs_catalogue(&mut self) {
        self.process_file_system
            .with_mut(ProcessFileSystemState::publish_native_vfs_catalogue);
    }
}
