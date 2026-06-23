# OpenAI DeepSeekVibing Python SDK (Beta)

Build Python applications that start DeepSeekVibing threads, run turns, stream progress,
and control workspace access.

## Install

Install the SDK:

```bash
pip install deepseek-vibing-sdk
```

## Quickstart

The SDK reuses your existing DeepSeekVibing authentication when one is already
available:

```python
from deepseek_vibing import DeepSeekVibing

with DeepSeekVibing() as agent:
    thread = agent.thread_start()
    result = thread.run("Explain this repository in three bullets.")
    print(result.final_response)
```

`thread.run(...)` returns a `TurnResult` containing the final response,
collected items, and token usage.

## Authentication

Existing DeepSeekVibing authentication is reused automatically. To start ChatGPT
browser login explicitly:

```python
from deepseek_vibing import DeepSeekVibing

with DeepSeekVibing() as agent:
    login = agent.login_chatgpt()
    print(login.auth_url)
    print(login.wait().success)
```

For device-code login:

```python
with DeepSeekVibing() as agent:
    login = agent.login_chatgpt_device_code()
    print(login.verification_url, login.user_code)
    login.wait()
```

For API-key login:

```python
with DeepSeekVibing() as agent:
    agent.login_api_key("sk-...")
```

## Built-In Help

Use Python's standard `help(deepseek_vibing)`, `help(DeepSeekVibing)`, or
`python -m pydoc deepseek_vibing` documentation tools.

## Documentation

- [Getting started](https://github.com/deepseek-vibing/deepseek-vibing/blob/main/sdk/python/docs/getting-started.md)
- [API reference](https://github.com/deepseek-vibing/deepseek-vibing/blob/main/sdk/python/docs/api-reference.md)
- [FAQ](https://github.com/deepseek-vibing/deepseek-vibing/blob/main/sdk/python/docs/faq.md)
- [Examples](https://github.com/deepseek-vibing/deepseek-vibing/blob/main/sdk/python/examples/README.md)

The package is licensed under the
[repository Apache License 2.0](https://github.com/deepseek-vibing/deepseek-vibing/blob/main/LICENSE).
