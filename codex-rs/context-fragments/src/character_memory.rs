// Modified by Heiervang Technologies.
use crate::ContextualUserFragment;

const END_MARKER: &str = "</character_memory_context>";
const START_MARKER: &str = "<character_memory_context>";

/// Bounded, cited memory context selected for one canonical character scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterMemoryContext {
    body: String,
}

impl CharacterMemoryContext {
    pub fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }
}

impl ContextualUserFragment for CharacterMemoryContext {
    fn role(&self) -> &'static str {
        "user"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn body(&self) -> String {
        format!("\n{}\n", self.body)
    }

    fn type_markers() -> (&'static str, &'static str) {
        (START_MARKER, END_MARKER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_matches_registered_markers() {
        let context = CharacterMemoryContext::new("schema_version=1");
        let rendered = context.render();

        assert_eq!(
            rendered,
            "<character_memory_context>\nschema_version=1\n</character_memory_context>"
        );
        assert!(CharacterMemoryContext::matches_text(&rendered));
        assert!(CharacterMemoryContext::matches_text(
            "<CHARACTER_MEMORY_CONTEXT>\nbody\n</CHARACTER_MEMORY_CONTEXT>"
        ));
        assert!(!CharacterMemoryContext::matches_text(
            "<character_memory_context>\nbody"
        ));
    }
}
