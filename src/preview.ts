import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { PreviewApp } from "./preview/PreviewApp";
import { startThemeSync } from "./services/theme";
import "./theme.css";
import "./preview.css";
import "./media-indicators.css";

const render = () => createRoot(document.getElementById("app")!).render(createElement(PreviewApp));
void startThemeSync().then(render, render);
