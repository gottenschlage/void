use crate::{Branch, BranchId, Repository, RepositoryId, Workspace, WorktreeProvenance};

/// Window-local workspace state shared by workspace projections.
///
/// Persisted records and open-tab state have one owner here. GPUI entities such
/// as terminal panels and live-diff models remain owned by the view coordinator.
#[derive(Debug)]
pub struct WorkspaceModel {
    workspace: Workspace,
    repositories: Vec<Repository>,
    branches: Vec<Branch>,
    open_branch_ids: Vec<BranchId>,
    active_branch_id: Option<BranchId>,
}

impl WorkspaceModel {
    /// Creates a model from persisted records. No branches are open initially.
    pub fn new(
        workspace: Workspace,
        mut repositories: Vec<Repository>,
        mut branches: Vec<Branch>,
    ) -> Self {
        sort_repositories(&mut repositories);
        sort_branches(&mut branches);
        Self {
            workspace,
            repositories,
            branches,
            open_branch_ids: Vec::new(),
            active_branch_id: None,
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    pub fn branches(&self) -> &[Branch] {
        &self.branches
    }

    pub fn active_branches(&self) -> impl Iterator<Item = &Branch> {
        self.branches.iter().filter(|branch| {
            branch.archived_at.is_none() && self.repository_is_active(branch.repository_id)
        })
    }

    pub fn branch(&self, branch_id: BranchId) -> Option<&Branch> {
        self.branches.iter().find(|branch| branch.id == branch_id)
    }

    pub fn record_worktree_provenance(
        &mut self,
        branch_id: BranchId,
        provenance: WorktreeProvenance,
    ) -> bool {
        let Some(branch) = self
            .branches
            .iter_mut()
            .find(|branch| branch.id == branch_id)
        else {
            return false;
        };
        match &branch.worktree_provenance {
            Some(recorded) => recorded == &provenance,
            None => {
                branch.worktree_provenance = Some(provenance);
                true
            }
        }
    }

    pub fn open_branch_ids(&self) -> &[BranchId] {
        &self.open_branch_ids
    }

    pub fn open_branches(&self) -> impl Iterator<Item = &Branch> {
        self.open_branch_ids
            .iter()
            .filter_map(|branch_id| self.branch(*branch_id))
    }

    pub fn active_branch_id(&self) -> Option<BranchId> {
        self.active_branch_id
    }

    /// Opens and activates an active branch.
    pub fn open_branch(&mut self, branch_id: BranchId) -> bool {
        if !self.branch_is_active(branch_id) {
            return false;
        }
        if !self.open_branch_ids.contains(&branch_id) {
            self.open_branch_ids.push(branch_id);
        }
        self.active_branch_id = Some(branch_id);
        true
    }

    /// Activates an already open branch.
    pub fn activate_branch(&mut self, branch_id: BranchId) -> bool {
        if !self.open_branch_ids.contains(&branch_id) {
            return false;
        }
        self.active_branch_id = Some(branch_id);
        true
    }

    /// Closes a tab without archiving its branch.
    pub fn close_branch(&mut self, branch_id: BranchId) -> bool {
        self.remove_open_branches(|candidate| candidate == branch_id)
    }

    /// Moves an open branch to the target tab index.
    pub fn move_open_branch(&mut self, branch_id: BranchId, target_index: usize) -> bool {
        let Some(source_index) = self
            .open_branch_ids
            .iter()
            .position(|candidate| *candidate == branch_id)
        else {
            return false;
        };
        if source_index == target_index || target_index >= self.open_branch_ids.len() {
            return false;
        }

        let branch_id = self.open_branch_ids.remove(source_index);
        self.open_branch_ids.insert(target_index, branch_id);
        true
    }

    pub fn add_repository(&mut self, repository: Repository) -> bool {
        if repository.workspace_id != self.workspace.id
            || self
                .repositories
                .iter()
                .any(|existing| existing.id == repository.id)
        {
            return false;
        }
        self.repositories.push(repository);
        sort_repositories(&mut self.repositories);
        true
    }

    pub fn set_repository_pinned(&mut self, repository_id: RepositoryId, is_pinned: bool) -> bool {
        let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|repository| repository.id == repository_id && repository.archived_at.is_none())
        else {
            return false;
        };
        repository.is_pinned = is_pinned;
        sort_repositories(&mut self.repositories);
        true
    }

    pub fn add_branch(&mut self, branch: Branch) -> bool {
        if self.branch(branch.id).is_some()
            || !self
                .repositories
                .iter()
                .any(|repository| repository.id == branch.repository_id)
        {
            return false;
        }
        self.branches.push(branch);
        sort_branches(&mut self.branches);
        true
    }

    /// Archives a repository and closes all of its open branches.
    pub fn archive_repository(&mut self, repository_id: RepositoryId) -> bool {
        let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|repository| repository.id == repository_id && repository.archived_at.is_none())
        else {
            return false;
        };
        repository.archived_at = Some(String::new());
        repository.is_pinned = false;
        let branch_ids = self
            .branches
            .iter()
            .filter(|branch| branch.repository_id == repository_id)
            .map(|branch| branch.id)
            .collect::<Vec<_>>();
        self.remove_open_branches(|branch_id| branch_ids.contains(&branch_id));
        sort_repositories(&mut self.repositories);
        true
    }

    pub fn restore_repository(&mut self, repository_id: RepositoryId) -> bool {
        let Some(repository) = self
            .repositories
            .iter_mut()
            .find(|repository| repository.id == repository_id && repository.archived_at.is_some())
        else {
            return false;
        };
        repository.archived_at = None;
        sort_repositories(&mut self.repositories);
        true
    }

    /// Archives a branch and closes its tab if open.
    pub fn archive_branch(&mut self, branch_id: BranchId) -> bool {
        let Some(branch) = self
            .branches
            .iter_mut()
            .find(|branch| branch.id == branch_id && branch.archived_at.is_none())
        else {
            return false;
        };
        branch.archived_at = Some(String::new());
        branch.is_pinned = false;
        self.close_branch(branch_id);
        sort_branches(&mut self.branches);
        true
    }

    /// Removes a branch record and closes its tab if open.
    pub fn delete_branch(&mut self, branch_id: BranchId) -> Option<Branch> {
        let index = self
            .branches
            .iter()
            .position(|branch| branch.id == branch_id)?;
        self.close_branch(branch_id);
        Some(self.branches.remove(index))
    }

    pub fn reorder_repositories(&mut self, repository_ids: &[RepositoryId]) -> bool {
        let active_ids = self
            .repositories
            .iter()
            .filter(|repository| repository.archived_at.is_none())
            .map(|repository| repository.id)
            .collect::<Vec<_>>();
        if !same_ids(&active_ids, repository_ids) {
            return false;
        }

        for (position, repository_id) in repository_ids.iter().copied().enumerate() {
            if let Some(repository) = self
                .repositories
                .iter_mut()
                .find(|repository| repository.id == repository_id)
            {
                repository.position = position as i64;
            }
        }
        sort_repositories(&mut self.repositories);
        true
    }

    pub fn reorder_branches(
        &mut self,
        repository_id: RepositoryId,
        branch_ids: &[BranchId],
    ) -> bool {
        let active_ids = self
            .branches
            .iter()
            .filter(|branch| branch.repository_id == repository_id && branch.archived_at.is_none())
            .map(|branch| branch.id)
            .collect::<Vec<_>>();
        if !same_ids(&active_ids, branch_ids) {
            return false;
        }

        for (position, branch_id) in branch_ids.iter().copied().enumerate() {
            if let Some(branch) = self
                .branches
                .iter_mut()
                .find(|branch| branch.id == branch_id)
            {
                branch.position = position as i64;
            }
        }
        sort_branches(&mut self.branches);
        true
    }

    fn branch_is_active(&self, branch_id: BranchId) -> bool {
        self.branch(branch_id).is_some_and(|branch| {
            branch.archived_at.is_none() && self.repository_is_active(branch.repository_id)
        })
    }

    fn repository_is_active(&self, repository_id: RepositoryId) -> bool {
        self.repositories
            .iter()
            .any(|repository| repository.id == repository_id && repository.archived_at.is_none())
    }

    fn remove_open_branches(&mut self, mut should_remove: impl FnMut(BranchId) -> bool) -> bool {
        let active_branch_id = self.active_branch_id;
        let mut fallback_index = None;
        let mut removed_any = false;
        let mut retained = Vec::with_capacity(self.open_branch_ids.len());

        for branch_id in self.open_branch_ids.drain(..) {
            if should_remove(branch_id) {
                removed_any = true;
                if active_branch_id == Some(branch_id) {
                    fallback_index = Some(retained.len());
                }
            } else {
                retained.push(branch_id);
            }
        }
        self.open_branch_ids = retained;

        if let Some(index) = fallback_index {
            self.active_branch_id = self
                .open_branch_ids
                .get(index)
                .or_else(|| self.open_branch_ids.last())
                .copied();
        }
        removed_any
    }
}

fn sort_repositories(repositories: &mut [Repository]) {
    repositories.sort_by_key(|repository| {
        (
            repository.archived_at.is_some(),
            !repository.is_pinned,
            repository.position,
            repository.id,
        )
    });
}

fn sort_branches(branches: &mut [Branch]) {
    branches.sort_by_key(|branch| {
        (
            branch.repository_id,
            branch.archived_at.is_some(),
            !branch.is_pinned,
            branch.position,
            branch.id,
        )
    });
}

fn same_ids<T: Copy + Eq>(actual: &[T], requested: &[T]) -> bool {
    actual.len() == requested.len()
        && actual.iter().all(|id| {
            requested
                .iter()
                .filter(|candidate| *candidate == id)
                .count()
                == 1
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{BranchId, RepositoryId, WorkspaceId};

    fn model() -> WorkspaceModel {
        let workspace_id = WorkspaceId::from_i64(1);
        WorkspaceModel::new(
            Workspace {
                id: workspace_id,
                name: "Void".into(),
            },
            vec![repository(1, workspace_id), repository(2, workspace_id)],
            vec![branch(1, 1), branch(2, 2), branch(3, 1), branch(4, 2)],
        )
    }

    fn repository(id: i64, workspace_id: WorkspaceId) -> Repository {
        Repository {
            id: RepositoryId::from_i64(id),
            workspace_id,
            name: format!("repository-{id}"),
            path: PathBuf::from(format!("/repository-{id}")),
            position: id,
            is_pinned: false,
            sequence: 0,
            archived_at: None,
        }
    }

    fn branch(id: i64, repository_id: i64) -> Branch {
        Branch {
            id: BranchId::from_i64(id),
            repository_id: RepositoryId::from_i64(repository_id),
            number: id,
            name: format!("branch-{id}"),
            path: PathBuf::from(format!("/branch-{id}")),
            base_ref: "main".into(),
            position: id,
            is_pinned: false,
            archived_at: None,
            worktree_provenance: None,
        }
    }

    fn branch_id(id: i64) -> BranchId {
        BranchId::from_i64(id)
    }

    #[test]
    fn worktree_provenance_can_only_be_recorded_once() {
        let mut model = model();
        let provenance = WorktreeProvenance {
            git_dir: "/repository-1/.git/worktrees/branch-1".into(),
            git_dir_created_at_ns: 42,
        };

        assert!(model.record_worktree_provenance(branch_id(1), provenance.clone()));
        assert_eq!(
            model.branch(branch_id(1)).unwrap().worktree_provenance,
            Some(provenance)
        );
        assert!(!model.record_worktree_provenance(
            branch_id(1),
            WorktreeProvenance {
                git_dir: "/recreated".into(),
                git_dir_created_at_ns: 43,
            }
        ));
    }

    #[test]
    fn closing_active_branch_uses_right_neighbor_then_left_neighbor() {
        let mut model = model();
        for id in 1..=3 {
            assert!(model.open_branch(branch_id(id)));
        }

        assert!(model.close_branch(branch_id(2)));
        assert_eq!(model.active_branch_id(), Some(branch_id(3)));
        assert!(model.close_branch(branch_id(3)));
        assert_eq!(model.active_branch_id(), Some(branch_id(1)));
    }

    #[test]
    fn reopening_branch_appends_and_activates_it() {
        let mut model = model();
        assert!(model.open_branch(branch_id(1)));
        assert!(model.open_branch(branch_id(2)));
        assert!(model.close_branch(branch_id(1)));

        assert!(model.open_branch(branch_id(1)));
        assert_eq!(model.open_branch_ids(), &[branch_id(2), branch_id(1)]);
        assert_eq!(model.active_branch_id(), Some(branch_id(1)));
    }

    #[test]
    fn archiving_active_branch_closes_it_and_selects_fallback() {
        let mut model = model();
        assert!(model.open_branch(branch_id(1)));
        assert!(model.open_branch(branch_id(2)));

        assert!(model.archive_branch(branch_id(2)));
        assert!(!model.archive_branch(branch_id(2)));
        assert_eq!(model.open_branch_ids(), &[branch_id(1)]);
        assert_eq!(model.active_branch_id(), Some(branch_id(1)));
        assert!(model.branch(branch_id(2)).unwrap().archived_at.is_some());
    }

    #[test]
    fn archiving_repository_closes_all_of_its_interleaved_tabs() {
        let mut model = model();
        for id in 1..=4 {
            assert!(model.open_branch(branch_id(id)));
        }
        assert!(model.activate_branch(branch_id(3)));

        assert!(model.archive_repository(RepositoryId::from_i64(1)));
        assert_eq!(model.open_branch_ids(), &[branch_id(2), branch_id(4)]);
        assert_eq!(model.active_branch_id(), Some(branch_id(4)));
    }

    #[test]
    fn archived_repository_branches_cannot_be_opened_until_restore() {
        let mut model = model();
        let repository_id = RepositoryId::from_i64(1);
        assert!(model.archive_repository(repository_id));
        assert!(!model.open_branch(branch_id(1)));

        assert!(model.restore_repository(repository_id));
        assert!(model.open_branch(branch_id(1)));
    }

    #[test]
    fn moving_open_branch_preserves_active_identity() {
        let mut model = model();
        for id in 1..=3 {
            assert!(model.open_branch(branch_id(id)));
        }

        assert!(model.move_open_branch(branch_id(1), 2));
        assert_eq!(
            model.open_branch_ids(),
            &[branch_id(2), branch_id(3), branch_id(1)]
        );
        assert_eq!(model.active_branch_id(), Some(branch_id(3)));
    }

    #[test]
    fn deleting_branch_removes_its_record_and_open_tab() {
        let mut model = model();
        assert!(model.open_branch(branch_id(1)));

        let deleted = model.delete_branch(branch_id(1));
        assert_eq!(deleted.map(|branch| branch.id), Some(branch_id(1)));
        assert!(model.branch(branch_id(1)).is_none());
        assert!(model.open_branch_ids().is_empty());
    }

    #[test]
    fn reorder_rejects_incomplete_identity_sets() {
        let mut model = model();
        let repository_id = RepositoryId::from_i64(1);

        assert!(!model.reorder_branches(repository_id, &[branch_id(1)]));
        assert!(model.reorder_branches(repository_id, &[branch_id(3), branch_id(1)]));
        assert_eq!(model.branch(branch_id(3)).unwrap().position, 0);
        assert_eq!(model.branch(branch_id(1)).unwrap().position, 1);
        assert_eq!(
            model
                .branches()
                .iter()
                .filter(|branch| branch.repository_id == repository_id)
                .map(|branch| branch.id)
                .collect::<Vec<_>>(),
            [branch_id(3), branch_id(1)]
        );
    }
}
