import type { Erreur } from "../bindings/Erreur";
import { ErreurTanana, estSessionPerdue, messageErreur, versErreur } from "./erreurs";

/** Un exemple de chaque variante de l'enum Rust. */
const EXEMPLES: Erreur[] = [
  { code: "NON_INITIALISE" },
  { code: "DEJA_INITIALISE" },
  { code: "NON_AUTHENTIFIE" },
  { code: "SESSION_EXPIREE" },
  { code: "IDENTIFIANTS_INVALIDES" },
  { code: "COMPTE_INACTIF" },
  { code: "COMPTE_BLOQUE", details: { jusqu_a: "2026-03-10T14:15:00.000Z" } },
  { code: "CHANGEMENT_MDP_REQUIS" },
  { code: "PERMISSION_REFUSEE", details: { permission: "journal.voir" } },
  { code: "ACTION_SUR_SOI_MEME" },
  { code: "DERNIER_ADMINISTRATEUR" },
  { code: "VALIDATION", details: "le nom est obligatoire" },
  { code: "INTROUVABLE", details: "chambre" },
  { code: "DOUBLON", details: "chambre « 101 »" },
  { code: "CHAMBRE_DEJA_RESERVEE", details: { reservation_en_conflit: "r1" } },
  { code: "CHAMBRE_INDISPONIBLE" },
  { code: "STATUT_INVALIDE", details: { de: "annulee", vers: "arrivee" } },
  { code: "SOLDE_IMPAYE", details: { montant: 50_000 } },
  { code: "MONTANT_INVALIDE", details: "négatif" },
  { code: "RELAIS_INJOIGNABLE", details: "pas de connexion" },
  { code: "RELAIS_REFUSE", details: "accès refusé" },
  { code: "HOTEL_INCONNU_DU_RELAIS" },
  { code: "CLE_DISTANCE_INCORRECTE" },
  { code: "LECTURE_SEULE" },
  { code: "ENREGISTREMENT_IMPOSSIBLE", details: "/Documents : accès refusé" },
  { code: "BASE", details: "disque plein" },
];

describe("messageErreur", () => {
  it.each(EXEMPLES.map((e) => [e.code, e] as const))("%s a un message en français", (_, e) => {
    const m = messageErreur(e);
    expect(m.length).toBeGreaterThan(10);
    expect(m).not.toMatch(/undefined|\[object/);
  });

  it("couvre les 26 variantes de l'enum Rust", () => {
    expect(new Set(EXEMPLES.map((e) => e.code)).size).toBe(26);
  });

  it("intègre les détails utiles", () => {
    expect(messageErreur({ code: "VALIDATION", details: "le nom est obligatoire" })).toBe("Le nom est obligatoire.");
    expect(messageErreur({ code: "SOLDE_IMPAYE", details: { montant: 50_000 } }).replace(/\s/g, " ")).toContain("50 000");
    // Le statut est dit avec les mots de l'écran, pas le code interne
    expect(messageErreur({ code: "STATUT_INVALIDE", details: { de: "partie", vers: "arrivee" } })).toContain("« Séjour terminé »");
    expect(messageErreur({ code: "STATUT_INVALIDE", details: { de: "annulee", vers: "facture" } })).toBe(
      "Une réservation annulée n'a pas de facture. Vous pouvez imprimer le reçu de chaque paiement.",
    );
  });
});

describe("versErreur", () => {
  it("reconnaît une erreur structurée venue de Rust", () => {
    const e = versErreur({ code: "COMPTE_INACTIF" });
    expect(e).toBeInstanceOf(ErreurTanana);
    expect(e.code).toBe("COMPTE_INACTIF");
  });

  it("enveloppe toute autre valeur en erreur technique", () => {
    expect(versErreur("boom").erreur).toEqual({ code: "BASE", details: "boom" });
    expect(versErreur(new Error("oups")).erreur).toEqual({ code: "BASE", details: "oups" });
    expect(versErreur(null).code).toBe("BASE");
  });

  it("détecte une session perdue", () => {
    expect(estSessionPerdue(versErreur({ code: "SESSION_EXPIREE" }))).toBe(true);
    expect(estSessionPerdue(versErreur({ code: "NON_AUTHENTIFIE" }))).toBe(true);
    expect(estSessionPerdue(versErreur({ code: "PERMISSION_REFUSEE", details: { permission: "x" } }))).toBe(false);
  });
});
