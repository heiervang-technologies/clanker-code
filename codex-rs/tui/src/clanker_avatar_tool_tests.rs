use super::*;
use pretty_assertions::assert_eq;

fn call(arguments: serde_json::Value) -> DynamicToolCallParams {
    DynamicToolCallParams {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
        namespace: Some(CLANKER_NAMESPACE.to_string()),
        tool: AVATAR_TOOL.to_string(),
        arguments,
    }
}

#[test]
fn avatar_tool_is_deferred_under_clanker_namespace() {
    let DynamicToolSpec::Namespace(namespace) = dynamic_tool_spec() else {
        panic!("avatar tool should use a namespace");
    };
    let [DynamicToolNamespaceTool::Function(tool)] = namespace.tools.as_slice() else {
        panic!("clanker namespace should contain only the avatar tool");
    };

    assert_eq!(namespace.name, "clanker");
    assert_eq!(tool.name, "avatar");
    assert!(tool.defer_loading);
}

#[test]
fn create_installs_valid_avatar_pack_and_returns_custom_selector() {
    let codex_home = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("warren.png");
    image::RgbaImage::from_pixel(24, 24, image::Rgba([1, 2, 3, 255]))
        .save(&source)
        .unwrap();

    let result = execute(
        &call(json!({
            "action": "create",
            "id": "warren",
            "display_name": "Warren",
            "image_path": source
        })),
        codex_home.path(),
    );

    assert!(result.response.success);
    assert_eq!(result.selector.as_deref(), Some("custom:warren"));
    let pack = codex_character::validate_avatar_selector(
        &codex_home.path().join("avatars"),
        &AvatarSelector("warren/avatar.json".to_string()),
    )
    .unwrap();
    assert_eq!((pack.frame.width, pack.frame.height), (24, 24));
    assert_eq!(pack.display_name.as_deref(), Some("Warren"));
}

#[test]
fn create_rejects_wrong_dimensions_without_leaving_destination() {
    let codex_home = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("large.png");
    image::RgbaImage::new(48, 48).save(&source).unwrap();

    let result = execute(
        &call(json!({
            "action": "create",
            "id": "warren",
            "image_path": source
        })),
        codex_home.path(),
    );

    assert!(!result.response.success);
    assert_eq!(result.selector, None);
    assert!(!codex_home.path().join("avatars/warren").exists());
}

#[test]
fn select_normalizes_existing_local_avatar() {
    let codex_home = tempfile::tempdir().unwrap();
    let avatar = codex_home.path().join("avatars/warren");
    fs::create_dir_all(&avatar).unwrap();
    image::RgbaImage::new(24, 24)
        .save(avatar.join("avatar.png"))
        .unwrap();
    fs::write(
        avatar.join("avatar.json"),
        r#"{
            "renderMode": "ansi-half-block",
            "spritesheetPath": "avatar.png"
        }"#,
    )
    .unwrap();

    let result = execute(
        &call(json!({"action": "select", "id": "warren"})),
        codex_home.path(),
    );

    assert!(result.response.success);
    assert_eq!(result.selector.as_deref(), Some("custom:warren"));
}
