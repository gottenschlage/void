# Smoke-test Void

This checklist is for a maintainer to run manually after the automated Cargo
checks pass. Record the platform, Void commit, build kind, and result of each
section. Do not use a repository or worktree containing valuable changes.

## Safety and setup

- Use a disposable local Git repository with at least two local branches and a
  remote-free history you can delete.
- Use a fresh Void application-data directory or disposable OS account when
  checking first launch and persistence. Back up any existing `void.db` first.
- Use a signed, notarized disposable release only for updater checks.
- Do not test deletion against the Void source checkout or another working
  repository.
- A coding-agent process is not expected: the current product implements
  managed branches/worktrees and terminals, but no agent runtime.

## 1. Startup and native window

- [ ] Launch Void and confirm one centered `1300 × 850` window opens without a
      terminal or extra dialog appearing first.
- [ ] Confirm the native window can move, resize, minimize, zoom, close, and
      reopen normally.
- [ ] On macOS, toggle the sidebar and confirm traffic lights hide while
      collapsed, return while open, and remain visible in fullscreen.
- [ ] Double-click empty title-bar space and confirm native title-bar behavior;
      interacting with the sidebar toggle or branch tabs must not move the
      window.
- [ ] Confirm the dark palette, JetBrains Mono chrome, borders, hover states,
      and focus indicators remain legible.

## 2. First launch and text input

Use a fresh application-data directory for this section.

- [ ] Confirm the workspace-name field is focused on first launch.
- [ ] Submit an empty name and confirm no workspace is created.
- [ ] Enter a workspace name with ordinary ASCII text and submit with Enter.
- [ ] Repeat first launch separately and submit with the create button.
- [ ] In every text field, verify Left/Right, Shift+Left/Right, Select All,
      Copy, Cut, Paste, Backspace, and Delete.
- [ ] Paste multiline text and confirm it remains a single line with line
      breaks normalized to spaces.
- [ ] Enter a combining sequence such as `e` plus a combining acute accent and
      a joined emoji such as `👩‍💻`; one arrow or deletion step must treat each
      displayed grapheme as one unit.
- [ ] Verify click placement, Shift+click extension, left-to-right dragging,
      right-to-left dragging, and crossing the original selection anchor.
- [ ] Use a platform IME to compose text and confirm the marked range is
      underlined until committed or cancelled.
- [ ] With VoiceOver or the platform screen reader, confirm each field has the
      expected label, placeholder, value, focus, and editable set-value action.

## 3. Repository onboarding

- [ ] Add the disposable repository through the native directory picker.
- [ ] Confirm a nested directory, non-Git directory, bare repository, and an
      already-added repository are rejected with actionable inline feedback.
- [ ] Confirm the accepted repository name and path are correct and its row can
      expand and collapse.
- [ ] Add a second disposable repository, pin/unpin it, reorder repositories,
      and confirm ordering and pin state survive relaunch.
- [ ] Archive and restore a repository and confirm it moves between active and
      archived menu sections without deleting its Git directory.

## 4. Managed branch and worktree creation

- [ ] Open **Add branch** and confirm local base branches load with the checked
      out branch first.
- [ ] Generate several names and confirm they are editable lowercase
      adjective-animal names.
- [ ] Reject an empty name, a name that can escape the managed path, and a base
      ref that does not exist.
- [ ] Create a branch and confirm Git reports a linked worktree at Void's
      deterministic application-data path and the exact expected branch ref is
      checked out there.
- [ ] Request the same name again and confirm the allocated branch and path use
      `-2` rather than reusing the first identity.
- [ ] Confirm the immutable `#<number>` shown by Void increases and is not
      reused after an archived or failed reservation.
- [ ] Relaunch and confirm repository, branch, pin, archive, and ordering state
      reloads correctly.

## 5. Branch tabs and resource identity

- [ ] Select several branches and confirm tabs open once, activate correctly,
      and keep the matching sidebar row selected.
- [ ] Reorder tabs in both directions, including while the strip is scrolled.
- [ ] Close an inactive tab and confirm active identity does not change.
- [ ] Close an active middle tab and confirm its right neighbor activates;
      close the last tab and confirm the left neighbor activates.
- [ ] Close and reopen a branch and confirm it appends as a fresh session tab
      without archiving or deleting Git state.
- [ ] Collapse and reopen the sidebar while terminals are active; branch tab,
      terminal process, and active selection identity must not reset.

## 6. Terminal interaction and teardown

- [ ] Confirm the first terminal starts in the selected branch's managed
      worktree (`pwd` and `git branch --show-current`).
- [ ] Verify typing, Enter, arrow keys, Option/Alt-as-Meta, copy, paste, IME
      input, URL hover/opening, and platform path dropping.
- [ ] Produce long and wide output, then resize the window repeatedly; text,
      cursor, selection, and PTY dimensions must stay aligned without UI hangs.
- [ ] Verify ANSI standard, bright, 256-color, true-color, bold, italic,
      underline, strikethrough, wide-character, and combining-character output.
- [ ] Create multiple terminal tabs, reorder them, switch among them, and close
      the selected and unselected tabs. Closing the final terminal tab must
      immediately create and focus a replacement.
- [ ] Start a long-running child process, switch branches, and confirm the
      process remains alive while its branch tab remains open.
- [ ] Close that branch tab and confirm its terminal process tree exits,
      including a child that ignores ordinary hangup/termination signals.
- [ ] Rapidly open and close a branch while its terminal is still loading;
      confirm no terminal appears later and no orphan process remains.

## 7. Live diff summaries

- [ ] Modify a tracked file and confirm additions/deletions update in both the
      branch header and sidebar.
- [ ] Stage another tracked change and confirm staged plus unstaged counts are
      represented together.
- [ ] Add an untracked file and a binary change and confirm neither contributes
      text line totals.
- [ ] Commit all tracked changes and confirm the count disappears.
- [ ] Make rapid consecutive edits and confirm refreshes coalesce but the final
      count arrives.
- [ ] Switch, close, reopen, archive, and restore branches and confirm counts do
      not leak between branch identities or arrive after a branch is removed.

## 8. Archive lifecycle

- [ ] Archive an inactive branch and confirm its record disappears from active
      rows while its Git branch and worktree remain intact.
- [ ] Archive the active branch and confirm the right/left tab fallback is
      correct and its terminal resources are released.
- [ ] Archive a repository with interleaved open tabs from two repositories;
      only that repository's tabs, terminals, context headers, and live-diff
      state should disappear.
- [ ] Restore the repository and branches and confirm they can be opened again
      with their original stable identities.

## 9. Permanent deletion safety

Run every case in a disposable repository and inspect Git plus `void.db` after
each outcome.

- [ ] Cancel the initial exact-name dialog; nothing in Git or SQLite changes.
- [ ] Enter a near match, different case, or surrounding whitespace; deletion
      remains disabled. Enter the exact allocated branch name to proceed.
- [ ] Delete a clean worktree whose branch is merged; confirm the terminal is
      released first, then the worktree, branch, and finally SQLite row are
      removed.
- [ ] Make tracked and untracked worktree changes. Confirm safe removal refuses,
      identifies the dirty/untracked state, and leaves all data intact when the
      force prompt is cancelled.
- [ ] Explicitly confirm dirty-worktree removal in the disposable case and
      verify Void revalidates identity before removing it.
- [ ] Create an unmerged branch commit. Confirm branch deletion refuses after
      worktree removal and requires a separate unmerged-branch confirmation;
      cancelling preserves the SQLite row for retry.
- [ ] Retry the partial deletion, confirm the already-removed worktree is
      accepted only with matching recorded provenance, then explicitly confirm
      branch deletion and verify the row is removed last.
- [ ] Replace or re-register a managed path with a different worktree, branch,
      repository, or Git administration directory and confirm Void refuses to
      delete it.
- [ ] For a pre-provenance legacy row with its original live worktree, confirm
      the exact-name flow performs one-time adoption and then revalidates it.
      A missing legacy worktree must remain undeletable through this flow.

## 10. Updater

Use a disposable signed and notarized Apple-silicon release environment.

- [ ] Confirm development, unbundled, non-macOS, non-arm64, missing-marker, and
      missing-Team-ID builds do not attempt self-update.
- [ ] Publish a newer stable test manifest and confirm download progress appears
      and the exact `Void-aarch64.dmg` is selected.
- [ ] Confirm malformed JSON, oversized feed, prerelease/equal/older version,
      non-lowercase or incorrect SHA-256, wrong bundle id, wrong Team ID, wrong
      app version, non-arm64 code, invalid DMG signature, nested-code signature,
      and Gatekeeper failures are refused without replacing the running app.
- [ ] Confirm a valid update mounts read-only, replaces the app, unmounts the
      DMG, and presents restart; restart must launch the installed version.
- [ ] Cancel by quitting while the DMG is mounted and confirm the image is
      detached and the installer directory is eventually removed.
- [ ] Confirm installer directories older than 24 hours are removed on a later
      startup while newer or unrelated temporary directories are untouched.
- [ ] Confirm an automatic feed failure stays quiet and retries later, while a
      download/install failure shows its reason and a working **Retry** action.

## 11. Final regression pass

- [ ] Leave two repositories, several managed branches, multiple terminal tabs,
      pinned and archived records, and tracked modifications; relaunch and
      confirm only persisted state returns while session-only open-tab and
      terminal state follows the documented behavior.
- [ ] Confirm no operation freezes rendering while Git, filesystem watching,
      database work, terminal startup, download, or installation is active.
- [ ] Review logs for panics, orphan-task errors, repeated watcher failures,
      leaked mounts, and orphan terminal processes.
- [ ] Confirm no visible coding-agent controls or claims imply that an agent was
      started.
