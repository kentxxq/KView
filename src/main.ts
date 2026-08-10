import "./assets/css/main.css";
import { createApp } from "vue";
import { createPinia } from "pinia";
import ui from "@nuxt/ui/vue-plugin";
import App from "./App.vue";
import { installGlobalDiagnostics } from "./services/diagnostics";

installGlobalDiagnostics();
createApp(App).use(createPinia()).use(ui).mount("#app");
