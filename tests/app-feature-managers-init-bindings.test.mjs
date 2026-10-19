import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

// initializeFeatureManagers() 通过简写属性把 app.js 等脚本里的全局绑定传给各
// manager。曾因引用已被删除的元素变量（apiClipboardExportButton）在页面加载时抛
// ReferenceError，中断 compileStatusManager 等后续初始化，设置→构建面板因此空白。
// 本测试在隔离 vm 中真实执行该函数：每遇到一个 "X is not defined" 就注入桩并重跑，
// 汇总其引用的全部自由标识符，并要求每个标识符都在 index.html 脚本束中有声明，
// 或属于显式白名单中的浏览器全局。
const initSource = readFileSync(
  new URL("../static/app-feature-managers-init.js", import.meta.url),
  "utf8",
);
const appBundle = readEntryScriptBundle("index.html");

function createPermissiveStub() {
  const stub = new Proxy(function stub() {}, {
    apply() {
      return stub;
    },
    construct() {
      return {};
    },
    get(target, property) {
      if (property === Symbol.toPrimitive) {
        return (hint) => (hint === "number" ? 0 : "");
      }
      if (property === Symbol.iterator) {
        return function* iterator() {};
      }
      return stub;
    },
    set() {
      return true;
    },
    has() {
      return true;
    },
  });
  return stub;
}

const BROWSER_GLOBALS = new Set(["globalThis", "document", "window"]);
const sandbox = {};
for (const name of BROWSER_GLOBALS) {
  sandbox[name] = createPermissiveStub();
}
const context = vm.createContext(sandbox);
const initScript = new vm.Script(initSource, {
  filename: "app-feature-managers-init.js",
});
const callScript = new vm.Script("initializeFeatureManagers()", {
  filename: "app-feature-managers-init.js#call",
});

const freeIdentifiers = new Set();
for (let attempt = 0; attempt < 1000; attempt += 1) {
  try {
    initScript.runInContext(context);
    callScript.runInContext(context);
    break;
  } catch (error) {
    const match = /^(\S+) is not defined$/.exec(error?.message || "");
    if (!match) {
      assert.fail(
        `app-feature-managers-init.js 执行抛出非自由变量错误：${error?.stack || error}`,
      );
    }
    const identifier = match[1];
    if (!BROWSER_GLOBALS.has(identifier)) {
      freeIdentifiers.add(identifier);
    }
    sandbox[identifier] = createPermissiveStub();
  }
}

function declaredInBundle(identifier) {
  const pattern = new RegExp(
    `\\b(?:const|let|var|function\\*?|class)\\s+${identifier}\\b`,
  );
  return pattern.test(appBundle);
}

assert.ok(
  freeIdentifiers.size > 50,
  `isolated execution should enumerate a meaningful number of free identifiers, got ${freeIdentifiers.size}`,
);

const undeclared = [...freeIdentifiers]
  .filter((name) => !declaredInBundle(name))
  .sort();
assert.deepEqual(
  undeclared,
  [],
  `app-feature-managers-init.js 引用了 index.html 脚本束中未声明的标识符：${undeclared.join(", ")}`,
);
