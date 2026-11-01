pub const PLAN: &str = include_str!("../templates/plan.md");
pub const LARP: &str = include_str!("../templates/larp.md");
pub const LOCKED_IN: &str = include_str!("../templates/locked_in.md");
pub const BASED: &str = include_str!("../templates/based.md");
pub const CRINGE: &str = include_str!("../templates/cringe.md");
pub const ULTRACHILL: &str = include_str!("../templates/ultrachill.md");
/// Clanker's default collaboration mode is LARP; upstream consumers that
/// resolve the bundled "default" mode message receive the LARP template.
pub const DEFAULT: &str = LARP;
