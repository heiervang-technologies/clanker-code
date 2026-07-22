use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use codex_character::CharacterCatalog;
use codex_character::ValidationIssue;
use codex_character::validate_manifest_path;
use serde::Serialize;

use codex_character::CHARACTER_SCHEMA_VERSION;

#[derive(Debug, Parser)]
pub(crate) struct CharacterCli {
    #[command(subcommand)]
    command: CharacterSubcommand,
}

#[derive(Debug, clap::Subcommand)]
enum CharacterSubcommand {
    /// Validate one character manifest or the complete local registry.
    Validate(CharacterValidateArgs),
    /// Resolve a canonical id or declared alias from the local registry.
    Resolve(CharacterResolveArgs),
}

#[derive(Debug, Parser)]
struct CharacterValidateArgs {
    /// Manifest path. Defaults to the complete local registry when --all is used.
    #[arg(value_name = "MANIFEST", conflicts_with = "all")]
    manifest: Option<PathBuf>,
    /// Validate every manifest and global name collision in the local registry.
    #[arg(long, default_value_t = false, conflicts_with = "manifest")]
    all: bool,
    /// Emit the stable machine-readable character contract.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Parser)]
struct CharacterResolveArgs {
    /// Canonical character id or declared alias.
    #[arg(value_name = "NAME_OR_ALIAS")]
    input: String,
    /// Emit the stable machine-readable character contract.
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationOutput<'a> {
    schema_version: u32,
    ok: bool,
    manifest_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    errors: &'a [ValidationIssue],
    warnings: &'a [ValidationIssue],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolutionOutput<'a> {
    schema_version: u32,
    ok: bool,
    input: &'a str,
    id: &'a str,
    display_name: &'a str,
    manifest_path: String,
    match_kind: codex_character::MatchKind,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FailureOutput<'a> {
    schema_version: u32,
    ok: bool,
    errors: &'a [ValidationIssue],
}

impl CharacterCli {
    pub(crate) fn run(self, codex_home: &Path) -> anyhow::Result<bool> {
        match self.command {
            CharacterSubcommand::Validate(args) => run_validate(args, codex_home),
            CharacterSubcommand::Resolve(args) => run_resolve(args, codex_home),
        }
    }
}

fn run_validate(args: CharacterValidateArgs, codex_home: &Path) -> anyhow::Result<bool> {
    let _json = args.json;
    if args.all || args.manifest.is_none() {
        let catalog = CharacterCatalog::load(codex_home);
        let warnings = catalog
            .entries()
            .iter()
            .flat_map(|report| report.warnings.iter().cloned())
            .collect::<Vec<_>>();
        let output = ValidationOutput {
            schema_version: CHARACTER_SCHEMA_VERSION,
            ok: catalog.errors().is_empty(),
            manifest_path: catalog.registry_root().display().to_string(),
            id: None,
            errors: catalog.errors(),
            warnings: &warnings,
        };
        println!("{}", serde_json::to_string(&output)?);
        return Ok(output.ok);
    }

    let path = args.manifest.expect("manifest checked above");
    let report = validate_manifest_path(&path);
    let output = ValidationOutput {
        schema_version: CHARACTER_SCHEMA_VERSION,
        ok: report.is_valid(),
        manifest_path: report.manifest_path.display().to_string(),
        id: report.id(),
        errors: &report.errors,
        warnings: &report.warnings,
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(output.ok)
}

fn run_resolve(args: CharacterResolveArgs, codex_home: &Path) -> anyhow::Result<bool> {
    let _json = args.json;
    let catalog = CharacterCatalog::load(codex_home);
    match catalog.resolve(&args.input) {
        Ok(resolved) => {
            let output = ResolutionOutput {
                schema_version: CHARACTER_SCHEMA_VERSION,
                ok: true,
                input: &args.input,
                id: &resolved.manifest.id,
                display_name: &resolved.manifest.display_name,
                manifest_path: resolved.manifest_path.display().to_string(),
                match_kind: resolved.match_kind,
            };
            println!("{}", serde_json::to_string(&output)?);
            Ok(true)
        }
        Err(errors) => {
            let output = FailureOutput {
                schema_version: CHARACTER_SCHEMA_VERSION,
                ok: false,
                errors: &errors,
            };
            println!("{}", serde_json::to_string(&output)?);
            Ok(false)
        }
    }
}
