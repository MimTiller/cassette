//! Standalone component mocks kept in the backend crate. Each module
//! exports a `#[component]`-annotated function; `#[macro_use]` lifts
//! each component's generated invocation macro into the parent scope
//! so a `ui!` tree elsewhere in this crate can drop it in by its
//! PascalCase tag. These render self-contained — no backend call, no
//! shared refs — and are kept for dogfooding/robot tests until the
//! matching screens land in the `discord-clone` UI crate.

pub mod emoji_reaction_bar;
pub mod status_picker;
pub mod pinned_messages_panel;
