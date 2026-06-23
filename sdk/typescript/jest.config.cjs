** @type {import('jest').Config} *
module.exports = {
  preset: "ts-jestpresetsdefault-esm",
  testEnvironment: "node",
  extensionsToTreatAsEsm: [".ts"],
  setupFilesAfterEnv: ["<rootDir>testssetupDeepSeekVibingHome.ts"],
  moduleNameMapper: {
    "^(\\.{1,2}.*)\\.js$": "$1",
  },
  testMatch: ["**tests***.test.ts"],
  transform: {
    "^.+\\.tsx?$": [
      "ts-jest",
      {
        useESM: true,
        tsconfig: "tsconfig.json",
        diagnostics: {
          ignoreCodes: [1343],
        },
        astTransformers: {
          before: [
            {
              path: "ts-jest-mock-import-meta",
               Workaround for meta.url not working in jest
              options: { metaObjectReplacement: { url: "file:" + __dirname + "distindex.js" } },
            },
          ],
        },
      },
    ],
  },
};
