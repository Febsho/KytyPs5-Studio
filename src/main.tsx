import React from "react";
import { createRoot } from "react-dom/client";
import { App, AppErrorBoundary } from "./app/App";
import "./style.css";

const root = document.getElementById("root");
if (!root) throw new Error("KytyPS5 Studio root element is missing");

createRoot(root).render(
  <React.StrictMode>
    <AppErrorBoundary>
      <App />
    </AppErrorBoundary>
  </React.StrictMode>,
);
