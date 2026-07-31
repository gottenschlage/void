use super::*;
fn test_paths() -> VoidPaths {
    VoidPaths::from_data_dir("/data/Void")
}

#[test]
fn persists_the_approved_hierarchy_and_sort_order() {
    pollster::block_on(async {
        let db = WorkspaceDb::open_test().await?;
        let workspace_id = db.create_workspace("Void".into()).await?;
        assert_eq!(
            db.first_workspace()?,
            Some(Workspace {
                id: workspace_id,
                name: "Void".into(),
            })
        );
        let repository_id = db
            .add_repository(NewRepository {
                workspace_id,
                name: "app".into(),
                path: "/code/app".into(),
                position: 4,
                is_pinned: true,
            })
            .await?;
        let branch = db
            .reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "feature/auth".into(),
                    base_ref: "refs/heads/main".into(),
                    position: 2,
                    is_pinned: true,
                },
                &test_paths(),
            )
            .await?;

        assert_eq!(branch.number, 1);
        assert_eq!(branch.name, "feature/auth");
        assert_eq!(
            branch.path,
            Path::new("/data/Void/worktrees/app/feature-auth")
        );
        assert_eq!(db.repositories(workspace_id)?[0].sequence, 1);
        assert_eq!(db.branches(repository_id)?, vec![branch]);
        Ok::<_, anyhow::Error>(())
    })
    .unwrap();
}

#[test]
fn archived_branch_names_are_never_reused() {
    pollster::block_on(async {
        let db = WorkspaceDb::open_test().await?;
        let workspace_id = db.create_workspace("Void".into()).await?;
        let repository_id = db
            .add_repository(NewRepository {
                workspace_id,
                name: "app".into(),
                path: "/code/app".into(),
                position: 0,
                is_pinned: false,
            })
            .await?;
        let first = db
            .reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "fix-auth".into(),
                    base_ref: "refs/heads/main".into(),
                    position: 0,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;
        db.archive_branch(first.id).await?;
        let second = db
            .reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "fix-auth".into(),
                    base_ref: "refs/heads/main".into(),
                    position: 1,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;

        assert_eq!(first.number, 1);
        assert_eq!(second.number, 2);
        assert_eq!(second.name, "fix-auth-2");
        assert_eq!(
            second.path,
            Path::new("/data/Void/worktrees/app/fix-auth-2")
        );
        Ok::<_, anyhow::Error>(())
    })
    .unwrap();
}

#[test]
fn deleting_a_branch_removes_its_record_and_frees_its_name() {
    pollster::block_on(async {
        let db = WorkspaceDb::open_test().await?;
        let workspace_id = db.create_workspace("Void".into()).await?;
        let repository_id = db
            .add_repository(NewRepository {
                workspace_id,
                name: "app".into(),
                path: "/code/app".into(),
                position: 0,
                is_pinned: false,
            })
            .await?;
        let branch = db
            .reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "fix-auth".into(),
                    base_ref: "refs/heads/main".into(),
                    position: 0,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;

        db.delete_branch(branch.id).await?;
        assert_eq!(db.branches(repository_id)?, vec![]);

        let recreated = db
            .reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "fix-auth".into(),
                    base_ref: "refs/heads/main".into(),
                    position: 0,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;
        assert_eq!(recreated.name, "fix-auth");
        Ok::<_, anyhow::Error>(())
    })
    .unwrap();
}

#[test]
fn repository_menu_state_is_persisted_without_deleting_the_repository() {
    pollster::block_on(async {
        let db = WorkspaceDb::open_test().await?;
        let workspace_id = db.create_workspace("Void".into()).await?;
        let repository_id = db
            .add_repository(NewRepository {
                workspace_id,
                name: "app".into(),
                path: "/code/app".into(),
                position: 0,
                is_pinned: false,
            })
            .await?;

        db.set_repository_pinned(repository_id, true).await?;
        assert!(db.repositories(workspace_id)?[0].is_pinned);

        db.archive_repository(repository_id).await?;
        let repository = &db.repositories(workspace_id)?[0];
        assert!(!repository.is_pinned);
        assert!(repository.archived_at.is_some());

        db.unarchive_repository(repository_id).await?;
        assert!(db.repositories(workspace_id)?[0].archived_at.is_none());
        Ok::<_, anyhow::Error>(())
    })
    .unwrap();
}

#[test]
fn persists_repository_and_branch_drag_order() {
    pollster::block_on(async {
        let db = WorkspaceDb::open_test().await?;
        let workspace_id = db.create_workspace("Void".into()).await?;
        let first_repository = db
            .add_repository(NewRepository {
                workspace_id,
                name: "first".into(),
                path: "/code/first".into(),
                position: 0,
                is_pinned: false,
            })
            .await?;
        let second_repository = db
            .add_repository(NewRepository {
                workspace_id,
                name: "second".into(),
                path: "/code/second".into(),
                position: 1,
                is_pinned: false,
            })
            .await?;
        db.reorder_repositories(workspace_id, vec![second_repository, first_repository])
            .await?;
        assert_eq!(db.repositories(workspace_id)?[0].name, "second");

        let first_branch = db
            .reserve_branch(
                NewBranch {
                    repository_id: first_repository,
                    requested_name: "first-branch".into(),
                    base_ref: "main".into(),
                    position: 0,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;
        let second_branch = db
            .reserve_branch(
                NewBranch {
                    repository_id: first_repository,
                    requested_name: "second-branch".into(),
                    base_ref: "main".into(),
                    position: 1,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;
        db.reorder_branches(first_repository, vec![second_branch.id, first_branch.id])
            .await?;
        assert_eq!(db.branches(first_repository)?[0].name, "second-branch");
        Ok::<_, anyhow::Error>(())
    })
    .unwrap();
}

#[test]
fn flattened_path_collisions_use_the_same_suffix_rule() {
    pollster::block_on(async {
        let db = WorkspaceDb::open_test().await?;
        let workspace_id = db.create_workspace("Void".into()).await?;
        let repository_id = db
            .add_repository(NewRepository {
                workspace_id,
                name: "app".into(),
                path: "/code/app".into(),
                position: 0,
                is_pinned: false,
            })
            .await?;

        db.reserve_branch(
            NewBranch {
                repository_id,
                requested_name: "feature/auth".into(),
                base_ref: "refs/heads/main".into(),
                position: 0,
                is_pinned: false,
            },
            &test_paths(),
        )
        .await?;
        let collision = db
            .reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "feature-auth".into(),
                    base_ref: "refs/heads/main".into(),
                    position: 1,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;

        assert_eq!(collision.name, "feature-auth-2");
        assert_eq!(
            collision.path,
            Path::new("/data/Void/worktrees/app/feature-auth-2")
        );
        Ok::<_, anyhow::Error>(())
    })
    .unwrap();
}

#[test]
fn worktree_provenance_is_persisted_and_cannot_be_replaced() {
    pollster::block_on(async {
        let db = WorkspaceDb::open_test().await?;
        let workspace_id = db.create_workspace("Void".into()).await?;
        let repository_id = db
            .add_repository(NewRepository {
                workspace_id,
                name: "app".into(),
                path: "/code/app".into(),
                position: 0,
                is_pinned: false,
            })
            .await?;
        let branch = db
            .reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "feature/provenance".into(),
                    base_ref: "main".into(),
                    position: 0,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;
        let provenance = WorktreeProvenance {
            git_dir: "/code/app/.git/worktrees/provenance".into(),
            git_dir_created_at_ns: 42,
        };

        db.record_worktree_provenance(branch.id, provenance.clone())
            .await?;
        assert_eq!(
            db.branches(repository_id)?[0].worktree_provenance,
            Some(provenance)
        );

        let replacement = WorktreeProvenance {
            git_dir: "/code/app/.git/worktrees/recreated".into(),
            git_dir_created_at_ns: 43,
        };
        assert!(
            db.record_worktree_provenance(branch.id, replacement)
                .await
                .is_err()
        );
        Ok::<_, anyhow::Error>(())
    })
    .unwrap();
}
