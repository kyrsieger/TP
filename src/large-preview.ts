import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { LargePreviewApp } from "./preview/LargePreviewApp";
import "./large-preview.css";
import "./media-indicators.css";

createRoot(document.getElementById("app")!).render(createElement(LargePreviewApp));
