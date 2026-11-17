import { readFileSync } from "node:fs";

import { changerVersion, comparer } from "./nouvelle-version.mjs";

const noms = ["package.json", "package-lock.json", "src-tauri/tauri.conf.json", "Cargo.toml", "Cargo.lock"];
const fichiers = Object.fromEntries(noms.map((n) => [n, readFileSync(n, "utf8")]));
const actuelle = JSON.parse(fichiers["package.json"]!).version as string;
const suivante = actuelle.replace(/\d+$/, (n) => String(Number(n) + 1));

describe("Changer de version en une fois", () => {
  it("met le même numéro partout, sans rien toucher d'autre", () => {
    const { sortie } = changerVersion(fichiers, suivante);
    expect(JSON.parse(sortie["package.json"]).version).toBe(suivante);
    expect(JSON.parse(sortie["src-tauri/tauri.conf.json"]).version).toBe(suivante);
    const lock = JSON.parse(sortie["package-lock.json"]);
    expect([lock.version, lock.packages[""].version]).toEqual([suivante, suivante]);
    expect(sortie["Cargo.toml"]).toContain(`[workspace.package]\nversion = "${suivante}"`);
    for (const crate of ["tanana", "tanana-core", "tanana-protocole", "tanana-relais"])
      expect(sortie["Cargo.lock"]).toContain(`name = "${crate}"\nversion = "${suivante}"`);
    // Une seule ligne change dans tauri.conf.json et Cargo.toml
    const differences = (a: string, b: string) => a.split("\n").filter((l, i) => l !== b.split("\n")[i]).length;
    expect(differences(fichiers["src-tauri/tauri.conf.json"]!, sortie["src-tauri/tauri.conf.json"])).toBe(1);
    expect(differences(fichiers["Cargo.toml"]!, sortie["Cargo.toml"])).toBe(1);
    // Les dépendances externes du Cargo.lock ne bougent pas
    expect(differences(fichiers["Cargo.lock"]!, sortie["Cargo.lock"])).toBe(4);
  });

  it("refuse un numéro qui ne monte pas, ou mal écrit", () => {
    expect(() => changerVersion(fichiers, actuelle)).toThrow(/plus grande/);
    expect(() => changerVersion(fichiers, "0.0.1")).toThrow(/plus grande/);
    expect(() => changerVersion(fichiers, "v1.2")).toThrow(/numéro de version/);
  });

  it("compare les versions comme des nombres", () => {
    expect(comparer("0.10.0", "0.9.9")).toBeGreaterThan(0);
    expect(comparer("1.0.0", "1.0.0")).toBe(0);
  });
});
