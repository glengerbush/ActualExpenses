import { mount } from "svelte";
import { isIPhonePwa, enableOfflineShell } from "./lib/iphone-pwa";
import "./styles.css";
const companion = isIPhonePwa();
const { default: App } = companion
  ? await import("./lib/ExpenseCompanion.svelte")
  : await import("./App.svelte");
mount(App, { target: document.getElementById("app")! });
if (companion) void enableOfflineShell();
