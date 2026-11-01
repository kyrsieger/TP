import { startLanguageSync } from "./services/language";
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./theme.css";
import "./styles.css";

const render = () => createRoot(document.getElementById("app")!).render(createElement(App));
void startLanguageSync().then(render, render);
