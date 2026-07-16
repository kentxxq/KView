import "./assets/css/main.css";
import { createApp } from "vue";
import ui from "@nuxt/ui/vue-plugin";
import { createMemoryHistory, createRouter } from "vue-router";
import App from "./App.vue";

const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: "/", component: { render: () => null } }],
});

createApp(App).use(router).use(ui).mount("#app");
