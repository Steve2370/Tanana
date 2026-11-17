// @ts-expect-error -- script Node en JavaScript, sans types
import { configurationPublication } from "./config-publication.mjs";

const MAJ = {
  TAURI_UPDATER_PUBKEY: " cle-publique ",
  TAURI_SIGNING_PRIVATE_KEY: "cle-privee",
  TANANA_MISE_A_JOUR_URL: "https://github.com/beltran/tanana/releases/latest/download/latest.json",
};

describe("réglages de publication", () => {
  it("sans secrets : rien n'est activé, la publication fonctionne quand même", () => {
    const { conf, resume } = configurationPublication({});
    expect(conf).toEqual({});
    expect(resume.join()).toContain("désactivée");
  });

  it("mise à jour : clé publique, clé privée et adresse sont toutes nécessaires", () => {
    expect(configurationPublication({ ...MAJ, TAURI_SIGNING_PRIVATE_KEY: "" }).conf).toEqual({});
    const { conf } = configurationPublication(MAJ);
    expect(conf).toEqual({
      bundle: { createUpdaterArtifacts: true },
      plugins: {
        updater: {
          pubkey: "cle-publique",
          endpoints: [MAJ.TANANA_MISE_A_JOUR_URL],
          windows: { installMode: "passive" },
        },
      },
    });
  });

  it("signature Windows en plus de la mise à jour", () => {
    const { conf, resume } = configurationPublication({ ...MAJ, WINDOWS_CERTIFICATE_THUMBPRINT: "ABC123" });
    expect(conf.bundle).toEqual({
      createUpdaterArtifacts: true,
      windows: { certificateThumbprint: "ABC123", digestAlgorithm: "sha256", timestampUrl: "http://timestamp.digicert.com" },
    });
    expect(resume).toContain("signature Windows : activée");
  });
});
