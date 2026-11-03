import { render } from "preact";
import { App } from "./app";
import "./styles.css";

// Applied before first paint, so a saved theme does not flash the other one.
try {
  const saved = localStorage.getItem("wraith-theme");
  if (saved === "light" || saved === "dark") document.documentElement.setAttribute("data-theme", saved);
} catch {
  // Storage can be unavailable. The default theme is fine then.
}

render(<App />, document.getElementById("app")!);
