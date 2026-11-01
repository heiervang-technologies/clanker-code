use crate::ContextualUserFragment;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;

const END_MARKER: &str = "</character_memory_context>";
const START_MARKER: &str = "<character_memory_context>";

/// Bounded, cited memory context selected for one canonical character scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterMemoryContext {
    body: String,
}

impl CharacterMemoryContext {
    /// Content kind attached to rendered character memory context.
    pub const KIND: &'static str = "memory.character_context";

    pub fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }

    /// Renders the marked context as plain text.
    pub fn render_text(&self) -> String {
        format!("{START_MARKER}\n{}\n{END_MARKER}", self.body)
    }
}

impl ContextualUserFragment for CharacterMemoryContext {
    fn role(&self) -> &'static str {
        "user"
    }

    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind(Self::KIND.to_string())
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn body(&self) -> ContentItem {
        input_text(format!("\n{}\n", self.body))
    }

    fn type_markers() -> (&'static str, &'static str) {
        (START_MARKER, END_MARKER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn renders_and_matches_registered_markers() {
        let context = CharacterMemoryContext::new("schema_version=1");
        let rendered = context.render_text();

        assert_eq!(
            rendered,
            "<character_memory_context>\nschema_version=1\n</character_memory_context>"
        );
        assert_eq!(context.render(), input_text(rendered.clone()));
        assert!(CharacterMemoryContext::matches_text(&rendered));
        assert!(CharacterMemoryContext::matches_text(
            "<CHARACTER_MEMORY_CONTEXT>\nbody\n</CHARACTER_MEMORY_CONTEXT>"
        ));
        assert!(!CharacterMemoryContext::matches_text(
            "<character_memory_context>\nbody"
        ));
    }
}
