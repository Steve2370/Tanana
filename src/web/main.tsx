import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "./web.css";
import { PageConsultation } from "./Consultation";

createRoot(document.getElementById("racine")!).render(
  <StrictMode>
    <PageConsultation />
  </StrictMode>,
);
