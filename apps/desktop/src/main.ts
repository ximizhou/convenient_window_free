import App from "./App.svelte";
import { createDesktopHostBridge } from "./desktop-host-bridge";
import { configureHostBridge } from "./host-bridge";
import "./styles.css";
import { mount } from "svelte";

async function bootstrap(): Promise<void> {
  const target = document.getElementById("app");
  if (!target) throw new Error("App mount target was not found");

  const host = await createDesktopHostBridge();
  configureHostBridge(host);
  mount(App, { target });
  document.documentElement.dataset.appReady = "true";
}

void bootstrap().catch((error) => {
  console.error("Desktop bootstrap failed", error);
  document.documentElement.dataset.appError = "true";
});
