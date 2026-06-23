export type DeepSeekVibingConfigValue = string | number | boolean | DeepSeekVibingConfigValue[] | DeepSeekVibingConfigObject;

export type DeepSeekVibingConfigObject = { [key: string]: DeepSeekVibingConfigValue };

export type DeepSeekVibingOptions = {
  deepseekVibingPathOverride?: string;
  baseUrl?: string;
  apiKey?: string;
  /**
   * Additional `--config key=value` overrides to pass to the DeepSeekVibing CLI.
   *
   * Provide a JSON object and the SDK will flatten it into dotted paths and
   * serialize values as TOML literals so they are compatible with the CLI's
   * `--config` parsing.
   */
  config?: DeepSeekVibingConfigObject;
  /**
   * Environment variables passed to the DeepSeekVibing CLI process. When provided, the SDK
   * will not inherit variables from `process.env`.
   */
  env?: Record<string, string>;
};
