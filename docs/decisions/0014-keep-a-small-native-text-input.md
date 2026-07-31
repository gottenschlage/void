# ADR 0014: Keep a small native text input primitive

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

Void needs a single-line text field for workspace names, managed branch names,
and destructive confirmation. The existing `ui::TextInput` already owned its
GPUI focus handle, registered an `EntityInputHandler`, converted between
platform UTF-16 ranges and Rust UTF-8 byte offsets, painted its shaped line,
and exposed clipboard actions.

The audit found four correctness gaps in this narrow implementation:

- arrow movement used Unicode scalar boundaries rather than extended grapheme
  boundaries, so combining sequences and joined emoji could be split;
- selections did not retain their active direction, causing Shift+Arrow to
  move the wrong edge after reversing direction and reporting every native
  selection as forward;
- pointer input placed a caret but did not support Shift+click or drag
  selection;
- active IME composition had no marked-text presentation.

The text input also used one global element id for every instance and exposed a
text-input role and label without its current value, placeholder, or accessible
set-value action.

Zed's `ui_input::InputField` is not an appropriate dependency: it wraps the
full editor and therefore brings editor, buffer, language, settings, component
preview, and erased-editor infrastructure into a three-purpose primitive. The
pinned GPUI input example already demonstrates the smaller ownership boundary
Void needs.

## Decision

Keep `TextInput` as a GPUI entity in the domain-independent `ui` crate. Its
public API remains `new`, `text`, `set_text`, and `Focusable`; action types
remain unchanged.

Split only the custom element into `text_input/element.rs`. The entity module
owns content, directional selection, focus, clipboard and pointer interaction,
platform UTF-16 conversion, and IME state. The element module owns layout, text
shaping, composition underline, themed selection/caret painting, native input
registration, and the last painted geometry used by platform queries.

Represent selection as a normalized byte range plus a `reversed` bit. The bit
identifies which range edge is the active caret and is reported through
`UTF16Selection`. Move by extended grapheme boundaries from
`unicode-segmentation`, while continuing to translate all platform ranges as
UTF-16 code units.

Give each rendered field an id derived from its GPUI entity id. Expose the
current value and placeholder with `Role::TextInput`, and handle AccessKit's
`SetValue` action through the owning entity. This is intentionally the simple
value-based accessibility contract documented by GPUI; Void does not reproduce
Zed's editor-specific synthetic text-run and value-snapshot infrastructure.

Preserve the established single-line dimensions, key bindings, paste newline
normalization, focus border, cursor width, and public call sites. Use the active
theme's selection token instead of a hard-coded selection color.

## Consequences

- Cursor movement no longer enters the middle of an extended grapheme.
- Keyboard, pointer, platform, and painted selection state share one direction
  model.
- Mouse drag, Shift+click, and visible IME composition work without adding an
  editor dependency.
- Multiple simultaneous inputs have stable, distinct accessibility ids.
- The primitive remains intentionally limited: it has no multiline mode,
  scrolling, word navigation, undo history, validation model, password mode,
  or editor services.
- Full screen-reader caret/review support through synthetic AccessKit text runs
  remains outside this small value-based field. It should be introduced only
  if manual assistive-technology testing demonstrates that the simple GPUI
  contract is insufficient.

## References

Verified against local Zed commit
`5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/gpui/examples/input.rs::{TextInput,TextElement}`
- `crates/gpui/src/_accessibility.rs`
- `crates/gpui/src/elements/div.rs::StatefulInteractiveElement`
- `crates/settings_ui/src/components/input_field.rs::{SettingsInputField::render,text_field_a11y_state}`
- `crates/ui_input/src/{ui_input.rs,input_field.rs}`

Current primary documentation consulted:

- GPUI: <https://gpui.rs/>
- `unicode-segmentation` 1.13.3: <https://docs.rs/unicode-segmentation/1.13.3/unicode_segmentation/trait.UnicodeSegmentation.html>

The directional-selection, grapheme-boundary, pointer-selection, IME-marking,
and element-ownership patterns are adapted from the Apache-2.0-licensed pinned
GPUI example. Void keeps its own smaller styling, actions, accessibility value,
and single-line product boundary.
