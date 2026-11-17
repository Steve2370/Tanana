/**
 * Vérifie que le frontend et Rust restent alignés, en lisant les sources :
 * un oubli d'un côté ou de l'autre casse ce test, pas l'app chez le client.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { LONGUEUR_CLE } from "./distance";
import { libelleAction, LIBELLES_PERMISSIONS, TOUTES_PERMISSIONS, GROUPES_PERMISSIONS } from "./permissions";

const racine = join(__dirname, "..", "..");
const lire = (chemin: string) => readFileSync(join(racine, chemin), "utf8");

function commandesRust(): string[] {
  const lib = lire("src-tauri/src/lib.rs");
  const bloc = lib.slice(lib.indexOf("generate_handler!["), lib.indexOf("])"));
  return [...bloc.matchAll(/commandes::(\w+)/g)].map((m) => m[1]!).sort();
}

function commandesFrontend(): string[] {
  const api = lire("src/lib/api.ts");
  return [...new Set([...api.matchAll(/appel(?:Connecte)?<[^>]*>\("(\w+)"/g)].map((m) => m[1]!))].sort();
}

describe("cohérence Rust ↔ TypeScript", () => {
  it("chaque commande Rust a sa fonction dans api.ts, et inversement", () => {
    expect(commandesFrontend()).toEqual(commandesRust());
  });

  it("chaque commande déclarée dans commandes.rs est enregistrée dans lib.rs", () => {
    const declarees = [...lire("src-tauri/src/commandes.rs").matchAll(/(?:pub fn |commande!\()(\w+)/g)]
      .map((m) => m[1]!)
      .sort();
    expect(declarees).toEqual(commandesRust());
  });

  it("la longueur de la clé secrète vérifiée à l'écran est celle que Rust affiche", () => {
    const rust = lire("crates/tanana-core/src/distance/chiffrement.rs");
    expect(rust).toContain(`assert_eq!(affiche.len(), ${LONGUEUR_CLE} + 6`);
  });

  it("chaque permission de la migration SQL a un libellé et un groupe", () => {
    const sql = lire("crates/tanana-core/migrations/0001_initial.sql");
    const bloc = sql.slice(sql.indexOf("INSERT INTO permissions"), sql.indexOf("INSERT INTO roles"));
    const codes = [...bloc.matchAll(/\('([a-z]+(?:\.[a-z]+)+)',\s+'/g)].map((m) => m[1]!).sort();
    expect(codes).toEqual([...TOUTES_PERMISSIONS].sort());
    const groupees = GROUPES_PERMISSIONS.flatMap((g) => g.permissions).sort();
    expect(groupees).toEqual([...TOUTES_PERMISSIONS].sort());
    for (const p of TOUTES_PERMISSIONS) expect(LIBELLES_PERMISSIONS[p]).toBeTruthy();
  });

  it("chaque action écrite dans le journal par Rust a un libellé lisible", () => {
    const dossiers = ["crates/tanana-core/src", "crates/tanana-core/src/services", "crates/tanana-core/src/distance"];
    const sources = dossiers
      .flatMap((d) => readdirSync(join(racine, d)).filter((f) => f.endsWith(".rs")).map((f) => lire(join(d, f))))
      .join("\n");
    // Toutes les chaînes « domaine.action » des sources, moins les codes de permission
    const permissions = new Set<string>(TOUTES_PERMISSIONS);
    const actions = new Set(
      [...sources.matchAll(/"((?:auth|acces|utilisateur|role|type_chambre|chambre|client|reservation|sejour|paiement|depense|document|taxes|parametres|sauvegarde|systeme|distance)\.[a-z_]+)"/g)]
        .map((m) => m[1]!)
        .filter((a) => !permissions.has(a)),
    );
    expect(actions.size).toBeGreaterThan(30);
    const sansLibelle = [...actions].filter((a) => libelleAction(a) === a);
    expect(sansLibelle).toEqual([]);
  });
});
