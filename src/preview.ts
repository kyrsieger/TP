import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { PreviewApp } from "./preview/PreviewApp";
import "./preview.css";
import "./media-indicators.css";

createRoot(document.getElementById("app")!).render(createElement(PreviewApp));
