/**
 * Banc d'essai QA : l'application complète, connectée, avec le faux moteur
 * Rust, dans un vrai navigateur (Chrome). Utilisé par scripts/qa-fenetres.mjs.
 * Ouvrir à la main : npx vite, puis http://localhost:1420/src/qa/banc/
 * Paramètre ?nom=… : nom de l'hôtel (pour tester un nom très long).
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { Coquille } from "../../pages/Coquille";
import { FournisseurSession, useSession } from "../../lib/session";
import { session } from "../../test/outils";
import { fauxRust } from "../fuzz/fauxRust";
import "../../styles.css";

const faux = fauxRust();
mockIPC((commande, args) => faux.gestionnaires[commande]!((args ?? {}) as Record<string, unknown>));
Object.assign(window, { qaFaux: faux });

function Connecte() {
  const { ouvrir } = useSession();
  const [pret, setPret] = useState(false);
  useEffect(() => {
    void ouvrir(session()).then(() => setPret(true));
  }, [ouvrir]);
  const nom = new URLSearchParams(location.search).get("nom") ?? "Hôtel du Lac";
  return pret ? <Coquille nomHotel={nom} /> : null;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <FournisseurSession>
      <Connecte />
    </FournisseurSession>
  </StrictMode>,
);
