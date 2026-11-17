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
 *   S3-3 起编辑器按扩展名启用六类语法高亮：py/yaml/md 用内建 Monarch
 *   贡献（纯内存态高亮，不需要语言 worker）；json/log/conf 用下方自定义
 *   Monarch tokenizer（Monaco 无内建 log，json 引官方 language service
 *   需要 json.worker，体积代价不值）。仍只需 editor.worker 一个 worker。
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
// S3-3：按需接入内建语言贡献（Monarch 高亮无需语言 worker）。
import '../node_modules/monaco-editor/esm/vs/languages/definitions/python/register.js';
import '../node_modules/monaco-editor/esm/vs/languages/definitions/yaml/register.js';
import '../node_modules/monaco-editor/esm/vs/languages/definitions/markdown/register.js';
import EditorWorker from '../node_modules/monaco-editor/esm/vs/editor/editor.worker.js?worker';
import loader from '@monaco-editor/loader';
import { douTokens } from './shared/tokens';

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

// ---------- S3-3：dou-terminal 主题 + 六类语法高亮 ----------
// 主题色值与 src/styles/app.css 的 --dou-* token 同源（douTokens）。

editor.defineTheme('dou-terminal', {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'comment', foreground: douTokens.color.dim.slice(1), fontStyle: 'italic' },
    { token: 'keyword', foreground: douTokens.color.accent.slice(1) },
    { token: 'keyword.flow', foreground: douTokens.color.accent.slice(1) },
    { token: 'string', foreground: douTokens.color.warn.slice(1) },
    { token: 'string.escape', foreground: douTokens.color.accentStrong.slice(1) },
    { token: 'number', foreground: douTokens.color.info.slice(1) },
    { token: 'type', foreground: douTokens.color.ok.slice(1) },
    { token: 'type.identifier', foreground: douTokens.color.ok.slice(1) },
    { token: 'function', foreground: douTokens.color.accentStrong.slice(1) },
    { token: 'identifier', foreground: douTokens.color.text.slice(1) },
    { token: 'delimiter', foreground: douTokens.color.dim.slice(1) },
    { token: 'operator', foreground: douTokens.color.accent.slice(1) },
    // dou-json（自定义 tokenizer）
    { token: 'dou.json.key', foreground: douTokens.color.accent.slice(1) },
    { token: 'dou.json.string', foreground: douTokens.color.warn.slice(1) },
    { token: 'dou.json.keyword', foreground: douTokens.color.dangerText.slice(1) },
    { token: 'dou.json.number', foreground: douTokens.color.info.slice(1) },
    // dou-log（自定义 tokenizer）
    { token: 'dou.log.time', foreground: douTokens.color.dim.slice(1) },
    { token: 'dou.log.error', foreground: douTokens.color.dangerText.slice(1), fontStyle: 'bold' },
    { token: 'dou.log.warn', foreground: douTokens.color.warnText.slice(1) },
    { token: 'dou.log.info', foreground: douTokens.color.text.slice(1) },
    { token: 'dou.log.debug', foreground: douTokens.color.dim.slice(1) },
    { token: 'dou.log.num', foreground: douTokens.color.info.slice(1) },
    { token: 'dou.log.path', foreground: douTokens.color.accentStrong.slice(1) },
    // dou-conf（自定义 tokenizer）
    { token: 'dou.conf.comment', foreground: douTokens.color.dim.slice(1), fontStyle: 'italic' },
    { token: 'dou.conf.key', foreground: douTokens.color.accent.slice(1) },
    { token: 'dou.conf.string', foreground: douTokens.color.warn.slice(1) },
    { token: 'dou.conf.num', foreground: douTokens.color.info.slice(1) },
    { token: 'dou.conf.keyword', foreground: douTokens.color.accent.slice(1) },
  ],
  colors: {
    'editor.background': douTokens.color.bg,
    'editor.foreground': douTokens.color.text,
    'editorLineNumber.foreground': douTokens.color.borderStrong,
    'editorLineNumber.activeForeground': douTokens.color.dim,
    'editor.selectionBackground': '#' + '223029',
    'editor.lineHighlightBackground': douTokens.color.panel,
    'editorCursor.foreground': douTokens.color.accent,
    'editorIndentGuide.background1': '#' + '1a2420',
    'editorIndentGuide.activeBackground1': '#' + '223029',
  },
});

// json / log / conf 自定义 Monarch tokenizer（无语言依赖，零 worker）。
// 显式类型注解：Monarch 规则是二元/三元元组，裸字面量会被推断为数组而非元组。
const douJsonLang: languages.IMonarchLanguage = {
  ignoreCase: false,
  tokenizer: {
    root: [
      [/"(?:[^"\\]|\\.)*"(?=\s*:)/, 'dou.json.key'],
      [/"(?:[^"\\]|\\.)*"/, 'dou.json.string'],
      [/\b(?:true|false|null)\b/, 'dou.json.keyword'],
      [/-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/, 'dou.json.number'],
      [/[{}[\]]/, 'delimiter'],
      [/:,]/, 'delimiter'],
    ],
  },
};

const douLogLang: languages.IMonarchLanguage = {
  ignoreCase: true,
  tokenizer: {
    root: [
      [/\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?/, 'dou.log.time'],
      [/\[?(?:ERROR|FATAL|CRITICAL)\]?/, 'dou.log.error'],
      [/\[?(?:WARN|WARNING)\]?/, 'dou.log.warn'],
      [/\[?(?:INFO|NOTICE)\]?/, 'dou.log.info'],
      [/\[?(?:DEBUG|TRACE)\]?/, 'dou.log.debug'],
      [/(?:\/[\w.@\-]+)+/, 'dou.log.path'],
      [/\b\d+(?:\.\d+)?\b/, 'dou.log.num'],
    ],
  },
};

const douConfLang: languages.IMonarchLanguage = {
  ignoreCase: false,
  tokenizer: {
    root: [
      [/#.*$/, 'dou.conf.comment'],
      [/;.*$/, 'dou.conf.comment'],
      [/"(?:[^"\\]|\\.)*"/, 'dou.conf.string'],
      [/'(?:[^'\\]|\\.)*'/, 'dou.conf.string'],
      [/\b(?:yes|no|true|false|on|off|true|false)\b/, 'dou.conf.keyword'],
      [/^\s*[\w.@\-]+(?=[\s=])/, 'dou.conf.key'],
      [/\b\d+(?:\.\d+)?\b/, 'dou.conf.num'],
    ],
  },
};

languages.register({ id: 'dou-json' });
languages.setMonarchTokensProvider('dou-json', douJsonLang);
languages.register({ id: 'dou-log' });
languages.setMonarchTokensProvider('dou-log', douLogLang);
languages.register({ id: 'dou-conf' });
languages.setMonarchTokensProvider('dou-conf', douConfLang);

export { monaco };
