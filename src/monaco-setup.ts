/**
 * Monaco 本地化配置（消除运行时 CDN 依赖）
 *
 * 背景（2026-11-16 排查）：
 *   `@guolao/vue-monaco-editor` 依赖 `@monaco-editor/loader`，后者把 paths.vs
 *   硬编码为：
 *       https://cdn.jsdelivr.net/npm/monaco-editor@0.55.1/min/vs
 *   （见 node_modules/@monaco-editor/loader/lib/es/config/index.js）
 *   Tauri 应用在离线/内网环境访问不到该 CDN，loader 注入 <script> 失败 ->
 *   控制台报 "Monaco initialization error"，编辑器面板永远空白。
 *   附带问题：该 CDN 版本(0.55.1)与本地安装的 monaco-editor(0.57.0) 还错配。
 *
 * 方案：
 *   1) 把本地打包好的 monaco 实例注入 loader：
 *          loader.config({ monaco })
 *      loader.init() 检测到 state.monaco 已存在时直接 resolve，
 *      不会注入任何 <script>（见 loader/lib/es/loader/index.js:58-61）；
 *      组件库的 useMonaco() 也因为 __getMonacoInstance() 非空而完全跳过 init()。
 *      -> 运行时不访问任何 CDN。
 *
 *   2) 通过 MonacoEnvironment.getWorker 提供 editor worker。
 *      monaco 优先调用 getWorker(workerId, label) 拿 Worker 实例，
 *      拿不到才退回 getWorkerUrl（见 monaco-editor
 *      esm/vs/editor/standalone/browser/services/standaloneWebWorkerService.js:9-20）。
 *      所以这里必须用 getWorker 返回真实实例。
 *
 * 关于「必须显式导入具名导出」：
 *   monaco-editor 0.57 的 esm/vs/editor/editor.api.js **只有具名导出、没有 default**。
 *   若写成 `import * as monaco from '.../editor.api'`，rolldown 打包出来的命名空间
 *   对象会是空壳 `{}`（实测 bundle 中为 `Vo=e({})`），
 *   导致组件库调用 monaco.editor.getModel() 时报
 *   "Cannot read properties of undefined (reading 'getModel')"。
 *   因此这里改为显式具名导入，并手工组装成 monaco 实例对象。
 *
 * Worker 导入路径为什么用相对路径：
 *   monaco-editor 0.57 的 package.json exports 为
 *       "./*.js": "./esm/vs/*.js",  "./*": "./esm/vs/*.js"
 *   用包名导入 `monaco-editor/esm/vs/editor/editor.worker?worker` 时，
 *   exports 规则会把 "esm/vs/editor/editor.worker" 再拼一次 "esm/vs"，
 *   映射到不存在的 esm/vs/esm/vs/... 路径 ->
 *   rolldown 报 "failed to resolve import"，构建直接失败。
 *   改用相对路径绕过 package exports 解析即可正常打包，
 *   产物为 dist/assets/editor.worker-*.js（约 300 kB）。
 *   editor.api.js 同样用相对路径导入，原因一致（包名形式会被 exports
 *   规则映射到 .d.ts，导致 rolldown 报 MISSING_EXPORT）。
 *
 * 语言 worker 说明：
 *   本项目编辑器只用 language="plaintext"（纯文本），
 *   不需要 json/ts/css/html 等语言 worker。
 */
import {
  CancellationTokenSource,
  Emitter,
  KeyCode,
  KeyMod,
  MarkerSeverity,
  MarkerTag,
  Position,
  Range,
  Selection,
  SelectionDirection,
  Token,
  Uri,
  editor,
  languages,
} from '../node_modules/monaco-editor/esm/vs/editor/editor.api.js';
import EditorWorker from '../node_modules/monaco-editor/esm/vs/editor/editor.worker.js?worker';
import loader from '@monaco-editor/loader';

// 组装成标准的 monaco 实例对象（对齐 AMD 模式下 window.monaco 的形态）。
const monaco = {
  CancellationTokenSource,
  Emitter,
  KeyCode,
  KeyMod,
  MarkerSeverity,
  MarkerTag,
  Position,
  Range,
  Selection,
  SelectionDirection,
  Token,
  Uri,
  editor,
  languages,
};

// monaco 通过 MonacoEnvironment 向宿主索取 worker。
(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
  getWorker(_workerId: string, _label: string): Worker {
    return new EditorWorker();
  },
};

// 注入本地实例，loader 不会再尝试访问 CDN。
loader.config({ monaco });

export { monaco };
