import { createApp } from "vue";
// 必须在 App 之前：本地化 monaco，消除对 cdn.jsdelivr.net 的运行时依赖。
import "./monaco-setup";
import App from "./views/App.vue";
import "./styles/app.css";
import { installAppErrorHandler, installGlobalErrorHandlers } from "./core/useGlobalError";

const app = createApp(App);
// 全局兜底：未被 ErrorBoundary 捕获的组件错误、window 错误与未处理 Promise 拒绝
// 统一进错误条并提示，避免静默白屏。
installAppErrorHandler(app);
installGlobalErrorHandlers();
app.mount("#app");
