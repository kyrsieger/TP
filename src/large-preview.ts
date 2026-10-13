import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { LargePreviewApp } from "./preview/LargePreviewApp";
import { startThemeSync } from "./services/theme";
import "./theme.css";
import "./large-preview.css";
import "./media-indicators.css";

const render = () => createRoot(document.getElementById("app")!).render(createElement(LargePreviewApp));
void startThemeSync().then(render, render);
