import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { installerCurseurAuto } from "./lib/curseur";
import { FournisseurSession } from "./lib/session";
import "./styles.css";

// Le curseur se place tout seul dans le premier champ des écrans et formulaires qui s'ouvrent
installerCurseurAuto();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <FournisseurSession>
      <App />
    </FournisseurSession>
  </StrictMode>,
);
