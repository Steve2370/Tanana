/**
 * Réglages ajoutés à `tauri.conf.json` au moment de publier une version
 * (workflow `.github/workflows/publication.yml`). Rien de secret ici : les
 * clés privées restent dans les secrets GitHub et passent par l'environnement.
 *
 * - Mise à jour automatique : si la clé publique ET la clé privée de signature
 *   des mises à jour sont fournies, l'application publiée cherchera ses
 *   nouvelles versions à l'adresse `TANANA_MISE_A_JOUR_URL`.
 * - Signature Windows : si l'empreinte d'un certificat importé est fournie.
 *
 * Usage : node scripts/config-publication.mjs src-tauri/tauri.publication.conf.json
 */
import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const rempli = (v) => typeof v === "string" && v.trim() !== "";

/** Construit la configuration et un résumé lisible de ce qui est activé. */
export function configurationPublication(env) {
  const conf = {};
  const resume = [];

  if (rempli(env.TAURI_UPDATER_PUBKEY) && rempli(env.TAURI_SIGNING_PRIVATE_KEY) && rempli(env.TANANA_MISE_A_JOUR_URL)) {
    conf.bundle = { createUpdaterArtifacts: true };
    conf.plugins = {
      updater: {
        pubkey: env.TAURI_UPDATER_PUBKEY.trim(),
        endpoints: [env.TANANA_MISE_A_JOUR_URL.trim()],
        // Windows : installation avec une simple barre de progression, sans questions
        windows: { installMode: "passive" },
      },
    };
    resume.push("mise à jour automatique : activée");
  } else {
    resume.push("mise à jour automatique : désactivée (TAURI_UPDATER_PUBKEY ou TAURI_SIGNING_PRIVATE_KEY absent)");
  }

  if (rempli(env.WINDOWS_CERTIFICATE_THUMBPRINT)) {
    conf.bundle = {
      ...conf.bundle,
      windows: {
        certificateThumbprint: env.WINDOWS_CERTIFICATE_THUMBPRINT.trim(),
        digestAlgorithm: "sha256",
        timestampUrl: rempli(env.WINDOWS_TIMESTAMP_URL) ? env.WINDOWS_TIMESTAMP_URL.trim() : "http://timestamp.digicert.com",
      },
    };
    resume.push("signature Windows : activée");
  }
  return { conf, resume };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const fichier = process.argv[2] ?? "src-tauri/tauri.publication.conf.json";
  const { conf, resume } = configurationPublication(process.env);
  writeFileSync(fichier, `${JSON.stringify(conf, null, 2)}\n`);
  for (const ligne of resume) console.log(`• ${ligne}`);
}
