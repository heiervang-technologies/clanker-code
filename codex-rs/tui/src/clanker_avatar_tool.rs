//! Deferred client-owned tool for installing and selecting local terminal avatars.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_app_server_protocol::DynamicToolCallOutputContentItem;
use codex_app_server_protocol::DynamicToolCallParams;
use codex_app_server_protocol::DynamicToolCallResponse;
use codex_app_server_protocol::DynamicToolFunctionSpec;
use codex_app_server_protocol::DynamicToolNamespaceSpec;
use codex_app_server_protocol::DynamicToolNamespaceTool;
use codex_app_server_protocol::DynamicToolSpec;
use codex_character::AvatarSelector;
use image::ImageFormat;
use serde::Deserialize;
use serde_json::json;

const CLANKER_NAMESPACE: &str = "clanker";
const AVATAR_TOOL: &str = "avatar";
const MAX_AVATAR_ID_LEN: usize = 64;
const MAX_DISPLAY_NAME_LEN: usize = 80;

pub(crate) struct AvatarToolResult {
    pub(crate) response: DynamicToolCallResponse,
    pub(crate) selector: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AvatarAction {
    Select,
    Create,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AvatarToolArgs {
    action: AvatarAction,
    id: String,
    image_path: Option<PathBuf>,
    display_name: Option<String>,
}

pub(crate) fn dynamic_tool_spec() -> DynamicToolSpec {
    DynamicToolSpec::Namespace(DynamicToolNamespaceSpec {
        name: CLANKER_NAMESPACE.to_string(),
        description: "Small controls for the live Clanker TUI runtime.".to_string(),
        tools: vec![DynamicToolNamespaceTool::Function(DynamicToolFunctionSpec {
            name: AVATAR_TOOL.to_string(),
            description: "Select a built-in or local avatar, or create and select a local avatar from one 24x24 image. Use only when the user explicitly asks to change their live Clanker avatar. Image bytes and avatar catalogs are never returned to model context.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["select", "create"],
                        "description": "Select an existing avatar, or create one from image_path and select it."
                    },
                    "id": {
                        "type": "string",
                        "description": "Built-in id, local id, or custom:<id> selector. New ids use lowercase letters, digits, and hyphens."
                    },
                    "image_path": {
                        "type": "string",
                        "description": "Local 24x24 image path. Required only for create."
                    },
                    "display_name": {
                        "type": "string",
                        "description": "Optional display name for a newly created avatar."
                    }
                },
                "required": ["action", "id"],
                "additionalProperties": false
            }),
            defer_loading: true,
        })],
    })
}

pub(crate) fn handles(params: &DynamicToolCallParams) -> bool {
    params.namespace.as_deref() == Some(CLANKER_NAMESPACE) && params.tool == AVATAR_TOOL
}

pub(crate) fn execute(params: &DynamicToolCallParams, codex_home: &Path) -> AvatarToolResult {
    match try_execute(params, codex_home) {
        Ok(selector) => AvatarToolResult {
            response: response(
                /*success*/ true,
                json!({"avatar": selector, "status": "selection_requested"}),
            ),
            selector: Some(selector),
        },
        Err(error) => AvatarToolResult {
            response: response(/*success*/ false, json!({"error": error.to_string()})),
            selector: None,
        },
    }
}

fn try_execute(params: &DynamicToolCallParams, codex_home: &Path) -> Result<String> {
    let args: AvatarToolArgs = serde_json::from_value(params.arguments.clone())
        .context("invalid clanker::avatar arguments")?;
    match args.action {
        AvatarAction::Select => {
            if args.image_path.is_some() || args.display_name.is_some() {
                bail!("image_path and display_name are valid only when action is create");
            }
            let id = validate_selector_id(&args.id)?;
            crate::pets::selectable_avatar_selector(&id, codex_home)
                .with_context(|| format!("unknown avatar {id}"))
        }
        AvatarAction::Create => {
            let id = args.id.strip_prefix("custom:").unwrap_or(&args.id);
            validate_new_avatar_id(id)?;
            let image_path = args
                .image_path
                .as_deref()
                .context("image_path is required when action is create")?;
            let display_name = validate_display_name(args.display_name.as_deref(), id)?;
            install_avatar(codex_home, id, &display_name, image_path)?;
            Ok(format!("custom:{id}"))
        }
    }
}

fn validate_selector_id(value: &str) -> Result<String> {
    let value = value.trim();
    let id = value.strip_prefix("custom:").unwrap_or(value);
    validate_new_avatar_id(id)?;
    Ok(value.to_string())
}

fn validate_new_avatar_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > MAX_AVATAR_ID_LEN {
        bail!("avatar id must be between 1 and {MAX_AVATAR_ID_LEN} bytes");
    }
    if id == crate::pets::DISABLED_PET_ID
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || !id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
    {
        bail!(
            "avatar id must start with a lowercase letter or digit and contain only lowercase letters, digits, and hyphens"
        );
    }
    Ok(())
}

fn validate_display_name(value: Option<&str>, id: &str) -> Result<String> {
    let display_name = value.unwrap_or(id).trim();
    if display_name.is_empty()
        || display_name.len() > MAX_DISPLAY_NAME_LEN
        || display_name.chars().any(char::is_control)
    {
        bail!("display_name must contain 1 to {MAX_DISPLAY_NAME_LEN} printable bytes");
    }
    Ok(display_name.to_string())
}

fn install_avatar(
    codex_home: &Path,
    id: &str,
    display_name: &str,
    image_path: &Path,
) -> Result<()> {
    let image = image::ImageReader::open(image_path)
        .with_context(|| format!("open avatar image {}", image_path.display()))?
        .with_guessed_format()
        .with_context(|| format!("detect avatar image format for {}", image_path.display()))?
        .decode()
        .with_context(|| format!("decode avatar image {}", image_path.display()))?;
    if image.width() != 24 || image.height() != 24 {
        bail!(
            "avatar image must be exactly 24x24 pixels, got {}x{}",
            image.width(),
            image.height()
        );
    }

    let avatars_root = codex_home.join("avatars");
    fs::create_dir_all(&avatars_root)
        .with_context(|| format!("create avatar root {}", avatars_root.display()))?;
    let destination = avatars_root.join(id);
    if destination.exists() {
        bail!("avatar {id} already exists; select it or choose a new id");
    }

    let staging = tempfile::Builder::new()
        .prefix(".clanker-avatar-")
        .tempdir_in(&avatars_root)
        .with_context(|| format!("stage avatar under {}", avatars_root.display()))?;
    image
        .into_rgba8()
        .save_with_format(staging.path().join("avatar.png"), ImageFormat::Png)
        .context("write staged avatar image")?;
    let manifest = json!({
        "id": id,
        "displayName": display_name,
        "description": "Local avatar installed by clanker::avatar",
        "renderMode": "ansi-half-block",
        "spritesheetPath": "avatar.png",
        "frame": {"width": 24, "height": 24, "columns": 1, "rows": 1},
        "animations": {"idle": {"frames": [0]}}
    });
    fs::write(
        staging.path().join("avatar.json"),
        serde_json::to_vec_pretty(&manifest).context("serialize avatar manifest")?,
    )
    .context("write staged avatar manifest")?;

    let staging_name = staging
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .context("staged avatar directory name is not UTF-8")?;
    codex_character::validate_avatar_selector(
        &avatars_root,
        &AvatarSelector(format!("{staging_name}/avatar.json")),
    )
    .map(drop)
    .map_err(anyhow::Error::msg)
    .context("validate staged avatar")?;

    let staging_path = staging.keep();
    fs::rename(&staging_path, &destination).with_context(|| {
        format!(
            "install staged avatar {} at {}",
            staging_path.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn response(success: bool, value: serde_json::Value) -> DynamicToolCallResponse {
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText {
            text: value.to_string(),
        }],
        success,
    }
}

#[cfg(test)]
#[path = "clanker_avatar_tool_tests.rs"]
mod tests;
