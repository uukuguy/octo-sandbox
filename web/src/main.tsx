import { createRoot } from "react-dom/client";
import { Provider } from "jotai";
import App from "./App";
import { initConfig } from "./config";
import "./globals.css";

// Initialize configuration before rendering
async function main() {
  try {
    await initConfig();
    console.log('[App] Config initialized, rendering...');
  } catch (err) {
    console.error('[App] Failed to initialize config:', err);
  }

  createRoot(document.getElementById("root")!).render(
    <Provider>
      <App />
    </Provider>,
  );
}

main();
