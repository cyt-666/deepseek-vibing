# Authentication

DeepSeek Vibing does not use ChatGPT browser login, device login, subscription
checks, `auth.json`, `CODEX_API_KEY`, `OPENAI_API_KEY`, or `CODEX_ACCESS_TOKEN`.

The normal authentication path is the single config file:

```toml
[auth]
deepseek_api_key = "sk-..."
vision_api_key = ""
```

DeepSeek key lookup order:

1. `[auth].deepseek_api_key`
2. `DEEPSEEK_API_KEY` for CI fallback

Vision key lookup order:

1. `[auth].vision_api_key`
2. `VISION_API_KEY` for CI fallback

Keep the file private:

```shell
chmod 600 ~/.deepseek-vibing/config.toml
```

MCP servers can still have their own OAuth or login semantics. Those credentials
belong to the MCP server, not to DeepSeek Vibing's main model authentication.
