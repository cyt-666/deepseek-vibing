import path from "node:path";

export function deepseekVibingPathOverride() {
  return (
    process.env.DEEPSEEK_VIBING_EXECUTABLE ??
    path.join(process.cwd(), "..", "..", "deepseek-vibing-rs", "target", "debug", "deepseek-vibing")
  );
}
