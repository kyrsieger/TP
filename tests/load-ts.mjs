import { readFileSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { createRequire } from "node:module";
import { runInNewContext } from "node:vm";
import ts from "typescript";

// Execute the real modules with explicit desktop ports, rather than slicing main.ts.
export function loadTs(file, globals = {}, mocks = {}) {
  const cache = new Map(), nativeRequire = createRequire(import.meta.url);
  function load(path) {
    path = resolve(path);
    if (cache.has(path)) return cache.get(path);
    const exports = {};
    cache.set(path, exports);
    const { outputText } = ts.transpileModule(readFileSync(path, "utf8"), {
      compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX },
    });
    runInNewContext(outputText, { ...globals, exports, require(name) {
      if (Object.hasOwn(mocks, name)) return mocks[name];
      if (!name.startsWith(".")) return nativeRequire(name);
      const target = resolve(dirname(path), name);
      return load(existsSync(target) ? target : existsSync(target + ".ts") ? target + ".ts" : target + ".tsx");
    } }, { filename: path });
    return exports;
  }
  return load(file);
}
