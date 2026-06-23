# DeepSeekVibing SDK

Embed the DeepSeekVibing agent in your workflows and apps.

The TypeScript SDK wraps the `deepseek-vibing` CLI from `deepseek-vibing`. It spawns the CLI and exchanges JSONL events over stdin/stdout.

## Installation

```bash
npm install deepseek-vibing-sdk
```

Requires Node.js 18+.

## Quickstart

```typescript
import { DeepSeekVibing } from "deepseek-vibing-sdk";

const deepseekVibing = new DeepSeekVibing();
const thread = deepseekVibing.startThread();
const turn = await thread.run("Diagnose the test failure and propose a fix");

console.log(turn.finalResponse);
console.log(turn.items);
```

Call `run()` repeatedly on the same `Thread` instance to continue that conversation.

```typescript
const nextTurn = await thread.run("Implement the fix");
```

### Streaming responses

`run()` buffers events until the turn finishes. To react to intermediate progress—tool calls, streaming responses, and file change notifications—use `runStreamed()` instead, which returns an async generator of structured events.

```typescript
const { events } = await thread.runStreamed("Diagnose the test failure and propose a fix");

for await (const event of events) {
  switch (event.type) {
    case "item.completed":
      console.log("item", event.item);
      break;
    case "turn.completed":
      console.log("usage", event.usage);
      break;
  }
}
```

### Structured output

The DeepSeekVibing agent can produce a JSON response that conforms to a specified schema. The schema can be provided for each turn as a plain JSON object.

```typescript
const schema = {
  type: "object",
  properties: {
    summary: { type: "string" },
    status: { type: "string", enum: ["ok", "action_required"] },
  },
  required: ["summary", "status"],
  additionalProperties: false,
} as const;

const turn = await thread.run("Summarize repository status", { outputSchema: schema });
console.log(turn.finalResponse);
```

You can also create a JSON schema from a [Zod schema](https://github.com/colinhacks/zod) using the [`zod-to-json-schema`](https://www.npmjs.com/package/zod-to-json-schema) package and setting the `target` to `"openAi"`.

```typescript
const schema = z.object({
  summary: z.string(),
  status: z.enum(["ok", "action_required"]),
});

const turn = await thread.run("Summarize repository status", {
  outputSchema: zodToJsonSchema(schema, { target: "openAi" }),
});
console.log(turn.finalResponse);
```

### Attaching images

Provide structured input entries when you need to include images alongside text. Text entries are concatenated into the final prompt while image entries are passed to the DeepSeekVibing CLI via `--image`.

```typescript
const turn = await thread.run([
  { type: "text", text: "Describe these screenshots" },
  { type: "local_image", path: "./ui.png" },
  { type: "local_image", path: "./diagram.jpg" },
]);
```

### Resuming an existing thread

Threads are persisted in `~/.deepseek-vibing/sessions`. If you lose the in-memory `Thread` object, reconstruct it with `resumeThread()` and keep going.

```typescript
const savedThreadId = process.env.DEEPSEEK_VIBING_THREAD_ID!;
const thread = deepseekVibing.resumeThread(savedThreadId);
await thread.run("Implement the fix");
```

### Working directory controls

DeepSeekVibing runs in the current working directory by default. To avoid unrecoverable errors, DeepSeekVibing requires the working directory to be a Git repository. You can skip the Git repository check by passing the `skipGitRepoCheck` option when creating a thread.

```typescript
const thread = deepseekVibing.startThread({
  workingDirectory: "/path/to/project",
  skipGitRepoCheck: true,
});
```

### Controlling the DeepSeekVibing CLI environment

By default, the DeepSeekVibing CLI inherits the Node.js process environment. Provide the optional `env` parameter when instantiating the
`DeepSeekVibing` client to fully control which variables the CLI receives—useful for sandboxed hosts like Electron apps.

```typescript
const deepseekVibing = new DeepSeekVibing({
  env: {
    PATH: "/usr/local/bin",
  },
});
```

The SDK still injects its required variables (such as `DEEPSEEK_API_KEY`) on top of the environment you provide. If you set
`baseUrl`, the SDK passes it as a `--config openai_base_url=...` override.

### Passing `--config` overrides

Use the `config` option to provide additional DeepSeekVibing CLI configuration overrides. The SDK accepts a JSON object, flattens it
into dotted paths, and serializes values as TOML literals before passing them as repeated `--config key=value` flags.

```typescript
const deepseekVibing = new DeepSeekVibing({
  config: {
    show_raw_agent_reasoning: true,
    sandbox_workspace_write: { network_access: true },
  },
});
```

Thread options still take precedence for overlapping settings because they are emitted after these global overrides.
