// Modified by Heiervang Technologies.
//! Mandatory character avatar selection and rendering state.
//!
//! Character identity owns the required default avatar, optional per-mode
//! variants, and placement. The sprite decoder/rendering engine is shared with
//! pets, but selection and lifecycle state are deliberately independent: pet
//! changes cannot disable, replace, or move the active character avatar.

mod assets;
mod binding;
mod runtime;

#[allow(unused_imports)]
pub(crate) use assets::ensure_bundled_avatars;
pub use assets::ensure_bundled_character_for_name;
pub(crate) use binding::resolve_named_avatar_binding;
pub(crate) use runtime::AvatarBinding;
pub(crate) use runtime::AvatarPlacement;
pub(crate) use runtime::AvatarRuntime;
pub(crate) use runtime::resolve_pet_placement;
