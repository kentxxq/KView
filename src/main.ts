import "./assets/css/main.css";
import { createApp } from "vue";
import ui from "@nuxt/ui/vue-plugin";
import App from "./App.vue";
import { installIcons } from "./icons";
import { installGlobalDiagnostics } from "./services/diagnostics";

installGlobalDiagnostics();
installIcons();
createApp(App).use(ui).mount("#app");
