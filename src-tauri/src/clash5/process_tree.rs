#[cfg(test)]
use std::collections::{BTreeMap, HashMap};

#[cfg(windows)]
pub(crate) fn selected_process_target(pid: u32) -> anyhow::Result<clash_verge_service_ipc::ProcessProxyTarget> {
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    if pid == 0 {
        anyhow::bail!("C5-PROXY-002");
    }
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    if unsafe { GetProcessTimes(handle.as_raw_handle(), &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let created_at = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    if created_at == 0 {
        anyhow::bail!("C5-PROXY-002");
    }
    Ok(clash_verge_service_ipc::ProcessProxyTarget { pid, created_at })
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ProcessKey {
    pid: u32,
    creation_time_100ns: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImageFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ProcessIdentity {
    key: ProcessKey,
    image_path: String,
    image_file: Option<ImageFileIdentity>,
}

#[cfg(test)]
impl ProcessIdentity {
    fn is_valid(&self) -> bool {
        self.key.pid != 0
            && self.key.creation_time_100ns != 0
            && !self.image_path.is_empty()
            && self.image_file.is_some()
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessTreeError {
    InvalidIdentity,
    SelfParent,
    InvalidLineage,
    ConflictingProcessInstance,
}

#[cfg(test)]
#[derive(Default)]
struct ProcessTree {
    members: HashMap<ProcessKey, ProcessIdentity>,
    parents: HashMap<ProcessKey, Option<ProcessKey>>,
    active_by_pid: BTreeMap<u32, ProcessKey>,
}

#[cfg(test)]
impl ProcessTree {
    pub fn new(root: ProcessIdentity) -> Result<Self, ProcessTreeError> {
        if !root.is_valid() {
            return Err(ProcessTreeError::InvalidIdentity);
        }

        let key = root.key;
        let mut tree = Self::default();
        tree.members.insert(key, root);
        tree.parents.insert(key, None);
        tree.active_by_pid.insert(key.pid, key);
        Ok(tree)
    }

    pub fn process_started(&mut self, identity: ProcessIdentity, parent: ProcessKey) -> Result<bool, ProcessTreeError> {
        if !identity.is_valid() {
            return Err(ProcessTreeError::InvalidIdentity);
        }
        if identity.key == parent {
            return Err(ProcessTreeError::SelfParent);
        }
        if !self.parents.contains_key(&parent) || self.active_by_pid.get(&parent.pid) != Some(&parent) {
            return Ok(false);
        }
        if identity.key.creation_time_100ns < parent.creation_time_100ns {
            return Err(ProcessTreeError::InvalidLineage);
        }

        if let Some(existing) = self.members.get(&identity.key) {
            return if existing == &identity && self.parents.get(&identity.key) == Some(&Some(parent)) {
                Ok(false)
            } else {
                Err(ProcessTreeError::ConflictingProcessInstance)
            };
        }

        if self.active_by_pid.contains_key(&identity.key.pid) {
            return Err(ProcessTreeError::ConflictingProcessInstance);
        }

        self.members.insert(identity.key, identity.clone());
        self.parents.insert(identity.key, Some(parent));
        self.active_by_pid.insert(identity.key.pid, identity.key);
        Ok(true)
    }

    pub fn process_exited(&mut self, key: ProcessKey) -> bool {
        if self.active_by_pid.get(&key.pid) != Some(&key) {
            return false;
        }
        self.active_by_pid.remove(&key.pid);
        true
    }

    pub fn active_targets(&self) -> Vec<&ProcessIdentity> {
        self.active_by_pid
            .values()
            .filter_map(|key| self.members.get(key))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "tests assert by panicking")]
mod tests {
    use super::*;

    fn process(pid: u32, creation_time_100ns: u64, name: &str) -> ProcessIdentity {
        ProcessIdentity {
            key: ProcessKey {
                pid,
                creation_time_100ns,
            },
            image_path: format!(r"C:\Apps\{name}.exe"),
            image_file: Some(ImageFileIdentity {
                volume_serial_number: 7,
                file_id: u128::from(pid).to_le_bytes(),
            }),
        }
    }

    #[test]
    fn includes_only_the_selected_process_and_its_descendants() {
        let root = process(100, 1_000, "selected");
        let root_key = root.key;
        let mut tree = ProcessTree::new(root).expect("valid root");
        let child = process(101, 1_100, "child");
        let child_key = child.key;
        assert!(tree.process_started(child, root_key).expect("child accepted"));
        assert!(
            tree.process_started(process(102, 1_200, "grandchild"), child_key)
                .expect("grandchild accepted")
        );
        assert!(
            !tree
                .process_started(
                    process(200, 1_300, "unrelated"),
                    ProcessKey {
                        pid: 999,
                        creation_time_100ns: 1,
                    }
                )
                .expect("unrelated process ignored")
        );

        let pids: Vec<_> = tree.active_targets().iter().map(|target| target.key.pid).collect();
        assert_eq!(pids, [100, 101, 102]);
    }

    #[test]
    fn pid_reuse_does_not_retarget_a_stale_process_instance() {
        let root = process(100, 1_000, "selected");
        let root_key = root.key;
        let mut tree = ProcessTree::new(root).expect("valid root");
        let first = process(101, 1_100, "first-child");
        let first_key = first.key;
        assert!(tree.process_started(first, root_key).expect("child accepted"));
        assert!(tree.process_exited(first_key));

        let reused = process(101, 2_100, "reused-pid");
        let reused_key = reused.key;
        assert!(tree.process_started(reused, root_key).expect("new child accepted"));
        assert!(!tree.process_exited(first_key));
        assert_eq!(tree.active_targets()[1].key, reused_key);
    }

    #[test]
    fn refuses_two_active_instances_with_the_same_pid() {
        let root = process(100, 1_000, "selected");
        let root_key = root.key;
        let mut tree = ProcessTree::new(root).expect("valid root");
        let first = process(101, 1_100, "first-child");
        assert!(tree.process_started(first, root_key).expect("child accepted"));

        assert!(matches!(
            tree.process_started(process(101, 2_100, "reused-pid"), root_key),
            Err(ProcessTreeError::ConflictingProcessInstance)
        ));
        assert_eq!(tree.active_targets().len(), 2);
    }

    #[test]
    fn exited_parent_remains_in_lineage_but_not_in_active_targets() {
        let root = process(100, 1_000, "selected");
        let root_key = root.key;
        let mut tree = ProcessTree::new(root).expect("valid root");
        let child = process(101, 1_100, "child");
        let child_key = child.key;
        assert!(tree.process_started(child, root_key).expect("child accepted"));
        assert!(tree.process_exited(root_key));

        assert!(
            tree.process_started(process(102, 1_200, "grandchild"), child_key)
                .expect("descendant accepted")
        );
        let pids: Vec<_> = tree.active_targets().iter().map(|target| target.key.pid).collect();
        assert_eq!(pids, [101, 102]);
    }

    #[test]
    fn a_reused_pid_cannot_be_used_as_a_parent_by_stale_events() {
        let root = process(100, 1_000, "selected");
        let root_key = root.key;
        let mut tree = ProcessTree::new(root).expect("valid root");
        let old_parent = process(101, 1_100, "old-parent");
        let old_parent_key = old_parent.key;
        assert!(tree.process_started(old_parent, root_key).expect("parent accepted"));
        assert!(tree.process_exited(old_parent_key));
        assert!(
            tree.process_started(process(101, 2_100, "new-parent"), root_key)
                .expect("reused parent accepted")
        );

        assert!(
            !tree
                .process_started(process(102, 1_200, "stale-child"), old_parent_key)
                .expect("stale child event ignored")
        );
        assert_eq!(tree.active_targets().len(), 2);
    }

    #[test]
    fn root_identity_must_be_complete() {
        let mut invalid = process(0, 0, "");
        invalid.image_path.clear();
        assert!(matches!(
            ProcessTree::new(invalid),
            Err(ProcessTreeError::InvalidIdentity)
        ));

        let mut missing_file_id = process(100, 1_000, "selected");
        missing_file_id.image_file = None;
        assert!(matches!(
            ProcessTree::new(missing_file_id),
            Err(ProcessTreeError::InvalidIdentity)
        ));
    }

    #[test]
    fn child_cannot_predate_the_selected_parent_instance() {
        let root = process(100, 2_000, "selected");
        let root_key = root.key;
        let mut tree = ProcessTree::new(root).expect("valid root");
        assert!(matches!(
            tree.process_started(process(101, 1_000, "stale-child"), root_key),
            Err(ProcessTreeError::InvalidLineage)
        ));
        assert_eq!(tree.active_targets().len(), 1);
    }
}
