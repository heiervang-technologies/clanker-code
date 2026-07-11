// clanker-delta: version-source

/// Product name used by Clanker Code command-line and TUI surfaces.
pub const PRODUCT_NAME: &str = "Clanker Code";

/// Clanker Code release plus the exact upstream Codex release provenance.
///
/// The hardened rebase pipeline updates this value from the upstream commit's
/// nearest `rust-v*` tag, tag distance, and twelve-character Git revision.
pub const VERSION: &str = "0.1.0+codex.0.143.0-alpha.10.355.g5c19155cbd93";
