import { createApp } from "vue";
// 必须在 App 之前：本地化 monaco，消除对 cdn.jsdelivr.net 的运行时依赖。
import "./monaco-setup";
import App from "./views/App.vue";
import "./styles/app.css";

createApp(App).mount("#app");
