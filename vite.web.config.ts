/**
 * Page web de consultation à distance (servie par le relais, voir
 * crates/tanana-relais). Construite à part de l'application de bureau :
 *   npm run build:web   →   dist-web/
 */
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  root: "src/web",
  base: "/",
  plugins: [react(), tailwindcss()],
  // Pas de variables d'environnement dans la page publique
  envPrefix: "TANANA_WEB_",
  build: {
    outDir: "../../dist-web",
    emptyOutDir: true,
    target: ["chrome110", "safari16.4", "firefox115"],
    // Tout en fichiers (pas de data: pour les scripts) ; la politique de sécurité n'autorise que 'self'
    assetsInlineLimit: 0,
  },
});
