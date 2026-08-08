import "./assets/css/main.css";
import { createApp } from "vue";
import ui from "@nuxt/ui/vue-plugin";
import App from "./App.vue";
import { installGlobalDiagnostics } from "./services/diagnostics";

installGlobalDiagnostics();
createApp(App).use(ui).mount("#app");
