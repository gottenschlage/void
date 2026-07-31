#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

//! Void's reusable, domain-agnostic UI primitives.
//!
//! Every type here renders with only `gpui` and `theme` — no knowledge of
//! Void's product types (`Branch`, `Repository`, ...). Screen-specific
//! components that compose these primitives with domain types stay in their
//! owning product crates, currently `workspace`.

mod auto_scroll;
mod dialog;
mod icon;
mod popover;
pub mod prelude;
mod reorder;
mod row;
mod scale;
mod text_input;

pub use auto_scroll::*;
pub use dialog::*;
pub use icon::*;
pub use popover::*;
pub use reorder::*;
pub use row::*;
pub use scale::*;
pub use text_input::*;
