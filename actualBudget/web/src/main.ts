import { mount } from "svelte";
import App from "./App.svelte";
import "./styles.css";
import sourceColors from "./lib/themes/source-colors.css?raw";
// Formula strings observe custom-property text. Keep source spelling intact;
// CSS minification otherwise rewrites values such as rgba() and hex colors.
const formulaStyle = document.createElement("style");
formulaStyle.textContent = sourceColors;
document.head.append(formulaStyle);
mount(App, { target: document.getElementById("app")! });
