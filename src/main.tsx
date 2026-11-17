import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { FournisseurSession } from "./lib/session";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <FournisseurSession>
      <App />
    </FournisseurSession>
  </StrictMode>,
);
