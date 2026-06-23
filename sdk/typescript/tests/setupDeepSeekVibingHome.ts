import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach } from "@jest/globals";

const originalDeepSeekVibingHome = process.env.DEEPSEEK_VIBING_HOME;
let currentDeepSeekVibingHome: string | undefined;

beforeEach(async () => {
  currentDeepSeekVibingHome = await fs.mkdtemp(path.join(os.tmpdir(), "deepseek-vibing-sdk-test-"));
  process.env.DEEPSEEK_VIBING_HOME = currentDeepSeekVibingHome;
});

afterEach(async () => {
  const deepseekVibingHomeToDelete = currentDeepSeekVibingHome;
  currentDeepSeekVibingHome = undefined;

  if (originalDeepSeekVibingHome === undefined) {
    delete process.env.DEEPSEEK_VIBING_HOME;
  } else {
    process.env.DEEPSEEK_VIBING_HOME = originalDeepSeekVibingHome;
  }

  if (deepseekVibingHomeToDelete) {
    await fs.rm(deepseekVibingHomeToDelete, { recursive: true, force: true });
  }
});
