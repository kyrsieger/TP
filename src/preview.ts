import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { PreviewApp } from "./preview/PreviewApp";
import { startThemeSync } from "./services/theme";
import { startFontSync } from "./services/fonts";
import "./theme.css";
import "./preview.css";
import "./media-indicators.css";

const render = () => createRoot(document.getElementById("app")!).render(createElement(PreviewApp));
void Promise.all([startThemeSync(), startFontSync()]).then(render, render);
