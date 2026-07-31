//! Active-branch identity and live uncommitted diff summary.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result};
use gpui::{Context, Entity, Render, Subscription, Task, Window, div, prelude::*, px};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use theme::ActiveTheme;
use workspace::{
    Branch, BranchId, DiffStat, GitWatchPaths, git_watch_paths, head_to_worktree_diff_stat,
};

const HEADER_HEIGHT: f32 = 37.5;
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(100);
const WATCH_RETRY_MAX_EXPONENT: u32 = 5;

#[derive(Default)]
struct RefreshState {
    running: bool,
    dirty: bool,
}

impl RefreshState {
    fn request(&mut self) -> bool {
        if self.running {
            self.dirty = true;
            false
        } else {
            self.running = true;
            true
        }
    }

    fn complete(&mut self) -> bool {
        if self.dirty {
            self.dirty = false;
            true
        } else {
            self.running = false;
            false
        }
    }
}

struct BranchDiff {
    worktree_path: PathBuf,
    git_dir: Option<PathBuf>,
    stat: Option<DiffStat>,
    error: Option<String>,
    refresh: RefreshState,
    refresh_task: Option<Task<()>>,
}

impl BranchDiff {
    fn new(worktree_path: PathBuf) -> Self {
        Self {
            worktree_path,
            git_dir: None,
            stat: None,
            error: None,
            refresh: RefreshState::default(),
            refresh_task: None,
        }
    }
}

enum WatchMessage {
    Event(Event),
    Error(notify::Error),
}

/// Repository-scoped live diff state shared by every active branch.
///
/// One watcher covers the repository's shared Git metadata and each registered
/// managed worktree. Dropping this entity cancels watcher setup, retries,
/// event processing, and in-flight Git commands.
pub(crate) struct RepositoryLiveDiff {
    branches: HashMap<BranchId, BranchDiff>,
    common_dir: Option<PathBuf>,
    watcher: Option<RecommendedWatcher>,
    watch_generation: usize,
    watch_failures: u32,
    setup_task: Option<Task<()>>,
    events_task: Option<Task<()>>,
    retry_task: Option<Task<()>>,
}

impl RepositoryLiveDiff {
    pub(crate) fn new() -> Self {
        Self {
            branches: HashMap::new(),
            common_dir: None,
            watcher: None,
            watch_generation: 0,
            watch_failures: 0,
            setup_task: None,
            events_task: None,
            retry_task: None,
        }
    }

    pub(crate) fn register(&mut self, branch: &Branch, cx: &mut Context<Self>) {
        if self.branches.contains_key(&branch.id) {
            return;
        }

        self.branches
            .insert(branch.id, BranchDiff::new(branch.path.clone()));
        self.request_refresh(branch.id, cx);
        self.restart_watcher(cx);
    }

    pub(crate) fn unregister(&mut self, branch_id: BranchId, cx: &mut Context<Self>) {
        if self.branches.remove(&branch_id).is_none() {
            return;
        }

        if self.branches.is_empty() {
            self.stop_watcher();
        } else {
            self.restart_watcher(cx);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }

    pub(crate) fn stat(&self, branch_id: BranchId) -> Option<DiffStat> {
        self.branches
            .get(&branch_id)
            .and_then(|branch| branch.stat)
            .filter(|stat| *stat != DiffStat::default())
    }

    fn request_refresh(&mut self, branch_id: BranchId, cx: &mut Context<Self>) {
        let Some(branch) = self.branches.get_mut(&branch_id) else {
            return;
        };
        if !branch.refresh.request() {
            return;
        }
        self.start_refresh(branch_id, cx);
    }

    fn start_refresh(&mut self, branch_id: BranchId, cx: &mut Context<Self>) {
        let Some(branch) = self.branches.get(&branch_id) else {
            return;
        };
        let worktree_path = branch.worktree_path.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = head_to_worktree_diff_stat(&worktree_path).await;
            let _ = this.update(cx, |this, cx| {
                this.finish_refresh(branch_id, result, cx);
            });
        });
        if let Some(branch) = self.branches.get_mut(&branch_id) {
            branch.refresh_task = Some(task);
        }
    }

    fn finish_refresh(
        &mut self,
        branch_id: BranchId,
        result: Result<DiffStat>,
        cx: &mut Context<Self>,
    ) {
        let Some(branch) = self.branches.get_mut(&branch_id) else {
            return;
        };

        let mut changed = false;
        match result {
            Ok(stat) => {
                changed = branch.stat != Some(stat);
                branch.stat = Some(stat);
                branch.error = None;
            }
            Err(error) => {
                let error = format!("{error:#}");
                if branch.error.as_deref() != Some(&error) {
                    eprintln!(
                        "could not refresh live diff for {}: {error}",
                        branch.worktree_path.display()
                    );
                }
                branch.error = Some(error);
            }
        }

        let refresh_again = branch.refresh.complete();
        if changed {
            cx.notify();
        }
        if refresh_again {
            self.start_refresh(branch_id, cx);
        }
    }

    fn restart_watcher(&mut self, cx: &mut Context<Self>) {
        self.watcher = None;
        self.events_task = None;
        self.retry_task = None;
        self.watch_generation = self.watch_generation.wrapping_add(1);
        let generation = self.watch_generation;
        let worktrees = self
            .branches
            .iter()
            .map(|(&id, branch)| (id, branch.worktree_path.clone()))
            .collect::<Vec<_>>();
        let executor = cx.background_executor().clone();

        self.setup_task = Some(cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { prepare_watcher(worktrees) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.watch_generation != generation {
                    return;
                }
                match result {
                    Ok(prepared) => this.install_watcher(prepared, cx),
                    Err(error) => this.watcher_failed(format!("{error:#}"), cx),
                }
            });
        }));
    }

    fn install_watcher(&mut self, prepared: PreparedWatcher, cx: &mut Context<Self>) {
        for (branch_id, paths) in prepared.paths {
            if let Some(branch) = self.branches.get_mut(&branch_id) {
                branch.git_dir = Some(paths.git_dir);
            }
            self.common_dir = Some(paths.common_dir);
        }
        self.watcher = Some(prepared.watcher);
        self.watch_failures = 0;
        self.retry_task = None;

        for branch_id in self.branches.keys().copied().collect::<Vec<_>>() {
            self.request_refresh(branch_id, cx);
        }

        let executor = cx.background_executor().clone();
        let events = prepared.events;
        self.events_task = Some(cx.spawn(async move |this, cx| {
            while let Ok(first) = events.recv().await {
                executor.timer(REFRESH_DEBOUNCE).await;
                let mut messages = vec![first];
                while let Ok(message) = events.try_recv() {
                    messages.push(message);
                }
                let has_error = messages
                    .iter()
                    .any(|message| matches!(message, WatchMessage::Error(_)));
                if this
                    .update(cx, |this, cx| this.handle_watch_messages(messages, cx))
                    .is_err()
                    || has_error
                {
                    break;
                }
            }
        }));
    }

    fn handle_watch_messages(&mut self, messages: Vec<WatchMessage>, cx: &mut Context<Self>) {
        let mut paths = Vec::new();
        for message in messages {
            match message {
                WatchMessage::Event(event) => paths.extend(event.paths),
                WatchMessage::Error(error) => {
                    self.watcher_failed(error.to_string(), cx);
                    return;
                }
            }
        }

        for branch_id in affected_branches(&self.branches, self.common_dir.as_deref(), &paths) {
            self.request_refresh(branch_id, cx);
        }
    }

    fn watcher_failed(&mut self, error: String, cx: &mut Context<Self>) {
        eprintln!("live diff filesystem watcher failed: {error}");
        self.watcher = None;
        self.watch_failures = self.watch_failures.saturating_add(1);
        let exponent = self.watch_failures.min(WATCH_RETRY_MAX_EXPONENT);
        let delay = Duration::from_secs(1_u64 << (exponent - 1));
        self.retry_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if !this.branches.is_empty() {
                    this.restart_watcher(cx);
                }
            });
        }));
    }

    fn stop_watcher(&mut self) {
        self.watch_generation = self.watch_generation.wrapping_add(1);
        self.common_dir = None;
        self.watcher = None;
        self.setup_task = None;
        self.events_task = None;
        self.retry_task = None;
    }
}

struct PreparedWatcher {
    paths: Vec<(BranchId, GitWatchPaths)>,
    watcher: RecommendedWatcher,
    events: async_channel::Receiver<WatchMessage>,
}

fn prepare_watcher(worktrees: Vec<(BranchId, PathBuf)>) -> Result<PreparedWatcher> {
    let mut paths = Vec::with_capacity(worktrees.len());
    for (branch_id, worktree_path) in &worktrees {
        paths.push((*branch_id, git_watch_paths(worktree_path)?));
    }
    let common_dir = paths
        .first()
        .map(|(_, paths)| paths.common_dir.as_path())
        .context("cannot watch a repository without an active branch")?;
    if paths
        .iter()
        .any(|(_, paths)| paths.common_dir != common_dir)
    {
        anyhow::bail!("active branches do not share one Git repository");
    }

    let (events_tx, events_rx) = async_channel::unbounded();
    let mut watcher = notify::recommended_watcher(move |result| {
        let message = match result {
            Ok(event) => WatchMessage::Event(event),
            Err(error) => WatchMessage::Error(error),
        };
        let _ = events_tx.try_send(message);
    })
    .context("could not create filesystem watcher")?;

    watcher
        .watch(common_dir, RecursiveMode::Recursive)
        .with_context(|| format!("could not watch {}", common_dir.display()))?;
    for (_, worktree_path) in &worktrees {
        watcher
            .watch(worktree_path, RecursiveMode::Recursive)
            .with_context(|| format!("could not watch {}", worktree_path.display()))?;
    }

    Ok(PreparedWatcher {
        paths,
        watcher,
        events: events_rx,
    })
}

fn affected_branches(
    branches: &HashMap<BranchId, BranchDiff>,
    common_dir: Option<&Path>,
    paths: &[PathBuf],
) -> HashSet<BranchId> {
    if paths.is_empty() {
        return branches.keys().copied().collect();
    }

    let mut affected = HashSet::new();
    for path in paths {
        let mut matched_branch = false;
        for (&branch_id, branch) in branches {
            if path_affects_branch(path, branch) {
                affected.insert(branch_id);
                matched_branch = true;
            }
        }
        if !matched_branch && is_shared_git_event(path, common_dir) {
            affected.extend(branches.keys().copied());
        }
    }
    affected
}

fn path_affects_branch(path: &Path, branch: &BranchDiff) -> bool {
    path.starts_with(&branch.worktree_path)
        || branch
            .git_dir
            .as_deref()
            .is_some_and(|git_dir| path.starts_with(git_dir))
}

fn is_shared_git_event(path: &Path, common_dir: Option<&Path>) -> bool {
    common_dir.is_some_and(|common_dir| path.starts_with(common_dir))
}

pub(crate) struct BranchContextHeader {
    branch: Branch,
    live_diff: Entity<RepositoryLiveDiff>,
    _live_diff_subscription: Subscription,
}

impl BranchContextHeader {
    pub(crate) fn new(
        branch: Branch,
        live_diff: Entity<RepositoryLiveDiff>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&live_diff, |_, _, cx| cx.notify());
        Self {
            branch,
            live_diff,
            _live_diff_subscription: subscription,
        }
    }
}

impl Render for BranchContextHeader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stat = self.live_diff.read(cx).stat(self.branch.id);
        div()
            .flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .items_center()
            .justify_between()
            .px_4()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .child(
                        div()
                            .text_color(cx.theme().colors().text_muted)
                            .child(format!("#{} {}/", self.branch.number, self.branch.base_ref)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_color(cx.theme().colors().text)
                            .child(self.branch.name.clone()),
                    ),
            )
            .when_some(stat, |header, stat| {
                header.child(
                    div()
                        .flex()
                        .flex_none()
                        .gap_2()
                        .child(
                            div()
                                .text_color(cx.theme().colors().version_control_added)
                                .child(format!("+{}", stat.added)),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().colors().version_control_deleted)
                                .child(format!("-{}", stat.deleted)),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch_diff(worktree: &str, git_dir: &str) -> BranchDiff {
        let mut branch = BranchDiff::new(PathBuf::from(worktree));
        branch.git_dir = Some(PathBuf::from(git_dir));
        branch
    }

    #[test]
    fn refresh_requests_coalesce_to_one_follow_up() {
        let mut state = RefreshState::default();
        assert!(state.request());
        assert!(!state.request());
        assert!(!state.request());
        assert!(state.complete());
        assert!(!state.complete());
        assert!(state.request());
    }

    #[test]
    fn worktree_and_linked_git_events_affect_the_matching_branch() {
        let branch = branch_diff("/repo/one", "/repo/.git/worktrees/one");
        assert!(path_affects_branch(
            Path::new("/repo/one/src/main.rs"),
            &branch
        ));
        assert!(path_affects_branch(
            Path::new("/repo/.git/worktrees/one/index"),
            &branch
        ));
        assert!(!path_affects_branch(
            Path::new("/repo/two/src/main.rs"),
            &branch
        ));
    }

    #[test]
    fn common_git_events_are_shared() {
        assert!(is_shared_git_event(
            Path::new("/repo/.git/refs/heads/main"),
            Some(Path::new("/repo/.git"))
        ));
        assert!(!is_shared_git_event(
            Path::new("/repo/one/src/main.rs"),
            Some(Path::new("/repo/.git"))
        ));
    }
}
