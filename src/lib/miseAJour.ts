/**
 * Mise à jour automatique : seul endroit qui parle au module de Tauri, pour
 * que les écrans restent simples à tester. Les versions de développement ne
 * cherchent rien (voir `infos_mise_a_jour` côté Rust).
 */
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

export interface NouvelleVersion {
  version: string;
  notes: string | null;
  /** Télécharge et installe ; `progression` reçoit un pourcentage (null : taille inconnue). */
  installer: (progression: (pourcent: number | null) => void) => Promise<void>;
}

/** Délai avant la première recherche, puis entre deux recherches. */
export const PREMIERE_RECHERCHE_MS = 15_000;
export const RECHERCHE_MS = 6 * 3600_000;

export async function chercherMiseAJour(): Promise<NouvelleVersion | null> {
  const maj = await check({ timeout: 30_000 });
  if (!maj) return null;
  return {
    version: maj.version,
    notes: maj.body?.trim() || null,
    installer: async (progression) => {
      let total = 0;
      let recu = 0;
      await maj.downloadAndInstall((e) => {
        if (e.event === "Started") {
          total = e.data.contentLength ?? 0;
          progression(total ? 0 : null);
        } else if (e.event === "Progress") {
          recu += e.data.chunkLength;
          progression(total ? Math.min(100, Math.round((recu / total) * 100)) : null);
        } else {
          progression(100);
        }
      });
    },
  };
}

export async function redemarrer(): Promise<void> {
  await relaunch();
}
