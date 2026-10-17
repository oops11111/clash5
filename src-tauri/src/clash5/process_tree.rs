use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ProcessKey {
    pid: u32,
    creation_time_100ns: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImageFileIdentity {
    volume_serial_number: u32,
    file_index: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProcessIdentity {
    key: ProcessKey,
    image_path: String,
    image_file: ImageFileIdentity,
}

impl ProcessIdentity {
    fn is_valid(&self) -> bool {
        self.key.pid != 0 && self.key.creation_time_100ns != 0 && !self.image_path.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessTreeError {
    InvalidIdentity,
    SelfParent,
    ConflictingProcessInstance,
}

#[derive(Default)]
struct ProcessTree {
    members: HashMap<ProcessKey, ProcessIdentity>,
    parents: HashMap<ProcessKey, Option<ProcessKey>>,
    active_by_pid: BTreeMap<u32, ProcessKey>,
}

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

        if let Some(existing) = self.members.get(&identity.key) {
            return if existing == &identity && self.parents.get(&identity.key) == Some(&Some(parent)) {
                Ok(false)
            } else {
                Err(ProcessTreeError::ConflictingProcessInstance)
            };
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
mod tests {
    use super::*;

    fn process(pid: u32, creation_time_100ns: u64, name: &str) -> ProcessIdentity {
        ProcessIdentity {
            key: ProcessKey {
                pid,
                creation_time_100ns,
            },
            image_path: format!(r"C:\Apps\{name}.exe"),
            image_file: ImageFileIdentity {
                volume_serial_number: 7,
                file_index: u64::from(pid),
            },
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
    }
}
