# Configuration

For basic configuration instructions, see [this documentation](https://developers.openai.com/codex/config-basic).

For advanced configuration instructions, see [this documentation](https://developers.openai.com/codex/config-advanced).

For a full configuration reference, see [this documentation](https://developers.openai.com/codex/config-reference).

## Chat Completions providers

Clanker Code speaks the Responses API by default. Providers that only implement
the legacy Chat Completions API (`POST /v1/chat/completions`, e.g. llama.cpp,
vLLM, Ollama, LiteLLM) can opt in per provider with `wire_api = "chat"`:

```toml
model = "gemma-4-12b"
model_provider = "gems"

[model_providers.gems]
name = "gems"
base_url = "https://api.markus.sh/v1"
env_key = "GEMS_API_KEY"   # read from the environment; never store the key here
wire_api = "chat"          # "responses" (default) or "chat"
```

On the chat wire:

- responses are streamed over SSE, with `stream_options.include_usage` so token
  usage is reported;
- function tools (shell, `exec_command`, MCP tools, ...) are sent as Chat
  Completions functions. Namespaced tools are flattened to
  `<namespace>__<name>`, and freeform tools such as grammar-based
  `apply_patch` become a function taking a single `input` string;
- hosted Responses tools (web search, tool search, image generation) are not
  advertised;
- images are sent as `image_url` parts; `data:audio/<format>;base64,...` parts
  are sent as `input_audio` for audio-capable models;
- streamed `reasoning_content` (or `reasoning`) is surfaced as raw reasoning and
  replayed on later assistant messages;
- WebSockets, request compression and remote compaction are Responses-only and
  are disabled.

### Reasoning control

`model_reasoning_effort` is forwarded as `reasoning_effort`. Many open-weight
servers (llama.cpp, vLLM, SGLang) ignore that field and switch thinking on or off
through the chat template instead, so `model_reasoning_effort = "none"` or
`"minimal"` also sends `chat_template_kwargs = { enable_thinking = false }`
(skipped for OpenAI/Azure base URLs, which reject unknown fields). For Gemma 4
on llama.cpp this cuts a short answer from hundreds of reasoning tokens to a
handful.

Anything else the server understands can be added per provider with
`extra_body`, which is deep-merged into every chat request after Clanker's own
fields:

```toml
[model_providers.gems]
# ...
wire_api = "chat"
extra_body = { chat_template_kwargs = { enable_thinking = false }, top_k = 40 }
```

Large prompts on a slow local server can take longer to prefill than the
default 300 s stream idle timeout; raise `stream_idle_timeout_ms` for such
providers.

## Lifecycle hooks

Admins can set top-level `allow_managed_hooks_only = true` in
`requirements.toml` to ignore user, project, and session hook configs while
still allowing managed hooks from requirements and managed config layers. This
setting is only supported in `requirements.toml`; putting it in `config.toml`
does not enable managed-hooks-only mode.
