# Void

Void is an AI-first coding workspace for running coding agents concurrently in isolated Git branches and worktrees. It is built in Rust with [GPUI](https://gpui.rs/), Zed's GPU-accelerated UI framework.

The repository contains the native GPUI application shell and SQLite persistence for Void's workspace, repositories, and managed branch/worktree records. Terminal, agent, repository-discovery, and Git-operation features have not been implemented yet.

## Prerequisites

Void currently follows the Rust toolchain used by the pinned Zed revision in [`rust-toolchain.toml`](rust-toolchain.toml).

1. Install Rust with [rustup](https://rustup.rs/). Rustup installs the pinned toolchain automatically when a Cargo command runs in this repository.
2. Install the platform prerequisites listed in the official [GPUI documentation](https://gpui.rs/).
   - On macOS, install Xcode, launch it once to install its components, and install the Xcode command-line tools with `xcode-select --install`.
   - For Linux, FreeBSD, and Windows, consult the current GPUI and Zed platform documentation before installing system packages.

## Run

From the repository root:

```sh
cargo run -p void
```

A successful run opens a centered `1300 × 800` window containing the temporary Void scaffold view.

## Validate

```sh
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Repository layout

```text
.
├── crates/
│   ├── void/          # Thin native application crate and composition root
│   └── workspace/     # Application paths and workspace-domain persistence
├── docs/
│   ├── architecture.md
│   └── decisions/     # Durable architecture decision records
├── Cargo.toml         # Workspace members, shared dependencies, and lints
└── rust-toolchain.toml
```

See [`docs/architecture.md`](docs/architecture.md) for the current boundaries and lifecycle.

## GPUI and Zed references

The scaffold is aligned with:

- official GPUI documentation: <https://gpui.rs/>;
- local Zed source at `/Users/usama/Documents/archive/zed`;
- Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`;
- `crates/gpui/README.md`;
- `crates/gpui/examples/hello_world.rs`;
- `crates/gpui_platform/src/gpui_platform.rs`;
- Zed's root `Cargo.toml`, `rust-toolchain.toml`, and `rustfmt.toml`.

GPUI is pre-1.0 and changes frequently. The dependency revision is therefore pinned; update it only through an explicit, documented compatibility change.

## Local data

Void stores local state beneath the platform application-data directory:

```text
Void/
├── void.db
└── worktrees/
    └── <repository name>/
        └── <allocated branch name>/
```

Git branch separators are flattened only in the directory name, so `feature/auth` uses `feature-auth`. Existing and archived names or paths receive `-2`, `-3`, and later suffixes instead of being reused. The database schema and invariants are documented in [`docs/decisions/0002-persist-workspace-repository-branches.md`](docs/decisions/0002-persist-workspace-repository-branches.md).

## License

Void is licensed under GPL-3.0-only. GPUI is a separate dependency licensed under Apache-2.0.
