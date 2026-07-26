// Modified by Heiervang Technologies.
use std::sync::Arc;

use codex_core::config::Config;
use codex_extension_api::ConfigContributor;
use codex_extension_api::ContextContributor;
use codex_extension_api::ExtensionData;
use codex_extension_api::ExtensionFuture;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::PromptFragment;
use codex_extension_api::PromptSlot;
use codex_extension_api::ThreadLifecycleContributor;
use codex_extension_api::ThreadStartInput;
use codex_extension_api::ToolContributor;
use codex_features::Feature;
use codex_otel::MetricsClient;
use codex_protocol::ThreadId;
use codex_state::MemorySelectionScope;
use codex_state::StateRuntime;
use codex_utils_absolute_path::AbsolutePathBuf;

use crate::character_context::CHARACTER_MEMORY_SELECTION_LIMIT;
use crate::character_context::build_character_memory_context;
use crate::local::ScopedLocalMemoriesBackend;
use crate::prompts::build_memory_tool_developer_instructions;
use crate::tools;

/// Contributes Codex memory read-path prompt context and memory read tools.
#[derive(Clone, Default)]
pub(crate) struct MemoriesExtension {
    metrics_client: Option<MetricsClient>,
    state_db: Option<Arc<StateRuntime>>,
}

impl MemoriesExtension {
    pub(crate) fn new(
        metrics_client: Option<MetricsClient>,
        state_db: Option<Arc<StateRuntime>>,
    ) -> Self {
        Self {
            metrics_client,
            state_db,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MemoriesExtensionConfig {
    pub(crate) enabled: bool,
    pub(crate) dedicated_tools: bool,
    pub(crate) codex_home: AbsolutePathBuf,
}

impl MemoriesExtensionConfig {
    fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.features.enabled(Feature::MemoryTool) && config.memories.use_memories,
            dedicated_tools: config.memories.dedicated_tools,
            codex_home: config.codex_home.clone(),
        }
    }
}

impl ContextContributor for MemoriesExtension {
    fn contribute_thread_context<'a>(
        &'a self,
        _session_store: &'a ExtensionData,
        thread_store: &'a ExtensionData,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<PromptFragment>> + Send + 'a>> {
        Box::pin(async move {
            let Some(config) = thread_store.get::<MemoriesExtensionConfig>() else {
                return Vec::new();
            };
            if !config.enabled {
                return Vec::new();
            }

            let Some(state_db) = self.state_db.as_ref() else {
                return legacy_memory_context(&config.codex_home).await;
            };
            let thread_id = match ThreadId::try_from(thread_store.level_id()) {
                Ok(thread_id) => thread_id,
                Err(_) => return Vec::new(),
            };
            let scope = match state_db.memories().memory_scope(thread_id).await {
                Ok(Some(scope)) => scope.selection_scope(),
                Ok(None) => return legacy_memory_context(&config.codex_home).await,
                Err(error) => {
                    tracing::warn!(%thread_id, "failed to resolve memory context scope: {error}");
                    return Vec::new();
                }
            };
            let MemorySelectionScope::Named { .. } = &scope else {
                return legacy_memory_context(&config.codex_home).await;
            };
            let records = match state_db
                .memories()
                .select_scoped_memories(&scope, CHARACTER_MEMORY_SELECTION_LIMIT)
                .await
            {
                Ok(records) => records,
                Err(error) => {
                    tracing::warn!(%thread_id, "failed to select character memory context: {error}");
                    return Vec::new();
                }
            };
            build_character_memory_context(&config.codex_home, &scope, &records)
                .await
                .map(|context| PromptFragment::new(PromptSlot::ContextualUser, context))
                .into_iter()
                .collect()
        })
    }
}

impl ThreadLifecycleContributor<Config> for MemoriesExtension {
    fn on_thread_start<'a>(
        &'a self,
        input: ThreadStartInput<'a, Config>,
    ) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            input
                .thread_store
                .insert(MemoriesExtensionConfig::from_config(input.config));
        })
    }
}

impl ConfigContributor<Config> for MemoriesExtension {
    fn on_config_changed(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
        _previous_config: &Config,
        new_config: &Config,
    ) {
        thread_store.insert(MemoriesExtensionConfig::from_config(new_config));
    }
}

impl ToolContributor for MemoriesExtension {
    fn tools(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
    ) -> Vec<Arc<dyn codex_extension_api::ToolExecutor<codex_extension_api::ToolCall>>> {
        let Some(config) = thread_store.get::<MemoriesExtensionConfig>() else {
            return Vec::new();
        };
        if !config.enabled || !config.dedicated_tools {
            return Vec::new();
        }

        let Some(state_db) = self.state_db.as_ref() else {
            return tools::memory_tools(
                crate::local::LocalMemoriesBackend::from_codex_home(&config.codex_home),
                self.metrics_client.clone(),
            );
        };

        tools::memory_tools(
            ScopedLocalMemoriesBackend::new(
                config.codex_home.clone(),
                Some(Arc::clone(state_db)),
                match ThreadId::try_from(thread_store.level_id()) {
                    Ok(thread_id) => thread_id,
                    Err(_) => return Vec::new(),
                },
            ),
            self.metrics_client.clone(),
        )
    }
}

/// Installs the memories extension contributors into the extension registry.
pub fn install(
    registry: &mut ExtensionRegistryBuilder<Config>,
    metrics_client: Option<MetricsClient>,
    state_db: Option<Arc<StateRuntime>>,
) {
    let extension = Arc::new(MemoriesExtension::new(metrics_client, state_db));
    registry.thread_lifecycle_contributor(extension.clone());
    registry.config_contributor(extension.clone());
    registry.prompt_contributor(extension.clone());
    registry.tool_contributor(extension);
}

async fn legacy_memory_context(codex_home: &AbsolutePathBuf) -> Vec<PromptFragment> {
    build_memory_tool_developer_instructions(codex_home)
        .await
        .map(PromptFragment::developer_policy)
        .into_iter()
        .collect()
}
