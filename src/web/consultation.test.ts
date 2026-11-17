/**
 * La page web lit exactement ce qu'envoie le vrai Tanana.
 * Fixture : `cargo run -p tanana-relais --example demo -- --date 2026-11-17 --fixture src/web/__fixtures__/hotel-demo.json`
 * (hôtel rempli par tanana-core, chiffré, passé par le vrai relais HTTP ; « attendu » = calculs de Tanana).
 */
import { readFileSync } from "node:fs";

import demo from "./__fixtures__/hotel-demo.json";
import { CleIncorrecte, lireCle, normaliserIdentifiant, preparerCles } from "./crypto";
import { Copie, ErreurConsultation, TABLES_ENVOYEES, mettreAJour, verifierCle, type PageLots } from "./relais";
import { SENSIBLES, caisse, hotel, journal, journee, planning, reservations, taxesAjoutees } from "./vues";

type Fixture = typeof demo;

/** Le relais, servi depuis la fixture, avec ses vraies règles (jeton, 404, pages). */
function fauxRelais(f: Fixture, jetonAttendu: string, modifier?: (p: PageLots) => PageLots) {
  const appels: string[] = [];
  const lecteur = (async (adresse: string, init?: RequestInit) => {
    appels.push(adresse);
    const jeton = (init?.headers as Record<string, string>).Authorization?.replace("Bearer ", "");
    if (!adresse.startsWith(`/v1/hotels/${f.hotel_id}/`)) return new Response("{}", { status: 404 });
    if (jeton !== jetonAttendu) return new Response("{}", { status: 403 });
    if (adresse.endsWith("/temoin")) return Response.json(f.temoin);
    const apres = Number(new URL(adresse, "http://x").searchParams.get("apres"));
    const tous = f.pages.flatMap((p) => p.lots).filter((l) => l.numero > apres);
    const page = { lots: tous.slice(0, 1), suite: tous.length > 1, dernier_contact: f.pages[0]!.dernier_contact };
    return Response.json(modifier ? modifier(page) : page);
  }) as typeof fetch;
  return { lecteur, appels };
}

async function charger(f: Fixture = demo) {
  const cles = await preparerCles(lireCle(f.cle_secrete));
  const { lecteur } = fauxRelais(f, cles.jetonLecture);
  const copie = new Copie();
  await verifierCle(lecteur, f.hotel_id, cles);
  await mettreAJour(lecteur, f.hotel_id, cles, copie);
  return copie;
}

describe("Clé et identifiant recopiés à la main", () => {
  it("la clé se lit avec ou sans tirets, en minuscules, avec 0/1/8 à la place de O/I/B", () => {
    const juste = lireCle(demo.cle_secrete);
    expect(juste).toHaveLength(16);
    const abimee = demo.cle_secrete.toLowerCase().replace(/-/g, " ").replace(/o/g, "0").replace(/i/g, "1").replace(/b/g, "8");
    expect(lireCle(abimee)).toEqual(juste);
  });

  it("une clé trop courte, trop longue ou avec un caractère impossible est refusée clairement", () => {
    for (const mauvaise of ["", "ABCD-EFGH", `${demo.cle_secrete}A`, demo.cle_secrete.replace(/.$/, "9"), "é".repeat(26)]) {
      expect(() => lireCle(mauvaise), mauvaise).toThrow(/mal recopiée/);
    }
  });

  it("l'identifiant de l'hôtel est remis en forme", () => {
    expect(normaliserIdentifiant(" hk7q f29x ")).toBe("HK7Q-F29X");
    expect(normaliserIdentifiant("HK7Q-F29X")).toBe("HK7Q-F29X");
    expect(normaliserIdentifiant("hk7q-f2ox")).toBe("HK7Q-F2OX");
  });
});

describe("Lecture sur le relais", () => {
  it("vérifie la clé, puis applique tous les lots, page par page", async () => {
    const cles = await preparerCles(lireCle(demo.cle_secrete));
    const { lecteur, appels } = fauxRelais(demo, cles.jetonLecture);
    const copie = new Copie();
    await verifierCle(lecteur, demo.hotel_id, cles);
    const n = await mettreAJour(lecteur, demo.hotel_id, cles, copie);
    expect(n).toBe(demo.pages.flatMap((p) => p.lots).length);
    expect(appels[0]).toMatch(/\/temoin$/);
    expect(copie.donneesDu).toMatch(/^\d{4}-\d\d-\d\dT/);
    // Une nouvelle lecture ne redemande que la suite
    expect(await mettreAJour(lecteur, demo.hotel_id, cles, copie)).toBe(0);
    expect(appels.at(-1)).toContain(`apres=${copie.dernierNumero}`);
  });

  it("une mauvaise clé est refusée avant tout téléchargement", async () => {
    const vraie = await preparerCles(lireCle(demo.cle_secrete));
    const autre = await preparerCles(lireCle("AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AA"));
    const { lecteur, appels } = fauxRelais(demo, vraie.jetonLecture);
    await expect(verifierCle(lecteur, demo.hotel_id, autre)).rejects.toBeInstanceOf(CleIncorrecte);
    expect(appels.some((a) => a.includes("/lots"))).toBe(false);
  });

  it("un identifiant inconnu donne un message clair", async () => {
    const cles = await preparerCles(lireCle(demo.cle_secrete));
    const { lecteur } = fauxRelais(demo, cles.jetonLecture);
    await expect(verifierCle(lecteur, "ZZZZ-ZZZZ", cles)).rejects.toThrow(ErreurConsultation);
    await expect(verifierCle(lecteur, "ZZZZ-ZZZZ", cles)).rejects.toThrow(/identifiant d'hôtel est inconnu/);
  });

  it("un paquet modifié par le serveur est refusé, et la copie reste intacte", async () => {
    const cles = await preparerCles(lireCle(demo.cle_secrete));
    const abimer = (p: PageLots): PageLots => ({
      ...p,
      lots: p.lots.map((l) => ({ ...l, lot: { ...l.lot, seq_fin: l.lot.seq_fin + 1 } })),
    });
    const { lecteur } = fauxRelais(demo, cles.jetonLecture, abimer);
    const copie = new Copie();
    await expect(mettreAJour(lecteur, demo.hotel_id, cles, copie)).rejects.toBeInstanceOf(CleIncorrecte);
    expect(copie.tables.size).toBe(0);
    expect(copie.dernierNumero).toBe(0);
  });

  it("serveur injoignable : message clair", async () => {
    const cles = await preparerCles(lireCle(demo.cle_secrete));
    const panne = (async () => {
      throw new TypeError("Failed to fetch");
    }) as typeof fetch;
    await expect(verifierCle(panne, demo.hotel_id, cles)).rejects.toThrow(/ne répond pas/);
  });
});

describe("Mêmes chiffres que Tanana", () => {
  it("nom de l'hôtel, devise et chambres", async () => {
    const copie = await charger();
    expect(hotel(copie).nom).toBe(demo.attendu.nom_hotel);
    expect(hotel(copie).devise).toEqual({ code: "XAF", decimales: 0 });
    expect(copie.lignes("chambres")).toHaveLength(demo.attendu.chambres);
  });

  it("chaque réservation : nuits, taxes, total à payer, encaissé et reste dû", async () => {
    const copie = await charger();
    const j = (n: number) => new Date(Date.parse(`${demo.aujourdhui}T00:00:00Z`) + n * 86_400_000).toISOString().slice(0, 10);
    const vues = planning(reservations(copie), j(-60), j(60));
    const champs = ["id", "chambre_numero", "client_nom", "date_arrivee", "date_depart", "nuits", "prix_total", "taxes_ajoutees", "total_a_payer", "total_encaisse", "solde", "statut"] as const;
    const garder = (r: object) => Object.fromEntries(champs.map((c) => [c, (r as Record<string, unknown>)[c]]));
    const tri = (a: Record<string, unknown>, b: Record<string, unknown>) => String(a.id).localeCompare(String(b.id));
    expect(vues.map(garder).sort(tri)).toEqual(demo.attendu.reservations.map(garder).sort(tri));
  });

  it("caisse du jour et des 7 derniers jours", async () => {
    const copie = await charger();
    const toutes = reservations(copie);
    const lendemain = new Date(Date.parse(`${demo.aujourdhui}T00:00:00Z`) + 86_400_000).toISOString().slice(0, 10);
    const ilYA6 = new Date(Date.parse(`${demo.aujourdhui}T00:00:00Z`) - 6 * 86_400_000).toISOString().slice(0, 10);
    for (const [debut, attendu] of [
      [demo.aujourdhui, demo.attendu.caisse_jour],
      [ilYA6, demo.attendu.caisse_semaine],
    ] as const) {
      const c = caisse(copie, toutes, debut, lendemain, 0);
      expect({
        encaissements: c.encaissements,
        encaissements_par_mode: c.encaissements_par_mode,
        remboursements: c.remboursements,
        depenses: c.depenses,
        net: c.net,
      }).toEqual({
        encaissements: attendu.encaissements,
        encaissements_par_mode: attendu.encaissements_par_mode,
        remboursements: attendu.remboursements,
        depenses: attendu.depenses,
        net: attendu.net,
      });
    }
  });

  it("la journée : arrivées, départs, chambres occupées, en maintenance, libres", async () => {
    const copie = await charger();
    const j = journee(copie, reservations(copie), demo.aujourdhui);
    expect(j.arrivees.map((r) => r.chambre_numero)).toEqual(["102"]);
    expect(j.departs.map((r) => r.chambre_numero)).toEqual(["201"]);
    const etats = Object.fromEntries(j.chambres.map((c) => [c.numero, c.etat]));
    expect(etats).toEqual({ "101": "occupee", "102": "attendue", "103": "libre", "104": "maintenance", "201": "depart", "202": "libre" });
    // Seul Mbarga (101) doit encore de l'argent parmi les présents
    expect(j.resteAEncaisser).toBe(demo.attendu.reservations.find((r) => r.chambre_numero === "101")!.solde);
  });

  it("le journal : du plus récent au plus ancien, avec le nom de l'employé et les actions à vérifier", async () => {
    const copie = await charger();
    const j = journal(copie);
    expect(j.length).toBeGreaterThan(10);
    expect(j[0]!.seq).toBeGreaterThan(j.at(-1)!.seq);
    expect(j.some((e) => e.utilisateur === "Beltran")).toBe(true);
    expect(j.find((e) => e.action === "reservation.annulation")?.sensible).toBe(true);
    expect(j.find((e) => e.action.startsWith("paiement.") && e.action !== "paiement.remboursement")?.sensible).toBe(false);
  });
});

describe("Taxes : mêmes arrondis que Tanana (domaine.rs)", () => {
  it("cas connus", () => {
    const sejour = { calcul: "par_nuit", valeur: 1_000, comprise: false };
    expect(taxesAjoutees(75_000, 3, [sejour])).toBe(3_000);
    // TVA 19,25 % ajoutée sur 100 000 : 19 250
    expect(taxesAjoutees(100_000, 1, [{ calcul: "pourcentage", valeur: 1925, comprise: false }])).toBe(19_250);
    // TVA comprise : rien ne s'ajoute ; une taxe ajoutée en % porte sur le hors-taxes
    const tvaComprise = { calcul: "pourcentage", valeur: 1925, comprise: true };
    expect(taxesAjoutees(119_250, 1, [tvaComprise])).toBe(0);
    expect(taxesAjoutees(119_250, 1, [tvaComprise, { calcul: "pourcentage", valeur: 1000, comprise: false }])).toBe(10_000);
    expect(taxesAjoutees(0, 2, [sejour, { calcul: "par_sejour", valeur: 500, comprise: false }])).toBe(2_500);
  });
});

describe("Cohérence avec le code Rust", () => {
  const rust = (chemin: string) => readFileSync(chemin, "utf8");
  const chaines = (bloc: string) => [...bloc.matchAll(/"([^"]+)"/g)].map((m) => m[1]);

  it("mêmes tables reçues que celles envoyées par l'hôtel", () => {
    const bloc = rust("crates/tanana-core/src/distance/capture.rs").match(/TABLES_ENVOYEES: \[&str; \d+\] = \[([\s\S]*?)\];/)![1]!;
    expect([...TABLES_ENVOYEES]).toEqual(chaines(bloc));
  });

  it("mêmes actions « à vérifier » que le journal de Tanana", () => {
    const bloc = rust("crates/tanana-core/src/audit.rs").match(/SENSIBLES: \[&str; \d+\] = \[([\s\S]*?)\];/)![1]!;
    expect(SENSIBLES).toEqual(chaines(bloc));
  });
});

describe("Cas absents de l'hôtel de démonstration", () => {
  const J = "2026-11-17";
  /** Petit hôtel construit à la main, ligne par ligne, comme l'hôtel les envoie. */
  function copieMinimale() {
    const copie = new Copie();
    const ecrire = (t: string, d: Record<string, unknown>, cle: unknown = d.id) => ({ t, o: "ecrire" as const, c: [cle], d });
    copie.appliquer([
      ecrire("parametres", { cle: "devise", valeur: "XAF" }, "devise"),
      ecrire("utilisateurs", { id: "u1", nom: "Awa" }),
      ecrire("types_chambre", { id: "t1", nom: "Standard" }),
      ecrire("chambres", { id: "c1", numero: "1", type_id: "t1", statut: "disponible" }),
      ecrire("chambres", { id: "c2", numero: "2", type_id: "t1", statut: "disponible" }),
      ecrire("clients", { id: "k1", nom: "Ndiaye", prenom: null }),
      // Parti ce matin (statut « partie ») : ce n'est plus un départ à faire
      ecrire("reservations", { id: "r1", chambre_id: "c1", client_id: "k1", date_arrivee: "2026-11-15", date_depart: J, prix_total: 50_000, statut: "partie" }),
      // Annulée qui devait arriver aujourd'hui : ni arrivée, ni chambre attendue
      ecrire("reservations", { id: "r2", chambre_id: "c2", client_id: "k1", date_arrivee: J, date_depart: "2026-11-18", prix_total: 25_000, statut: "annulee" }),
      ecrire("paiements", { id: "p1", reservation_id: "r1", montant: 50_000, type: "solde", mode: "especes", date: `${J}T09:00:00.000Z`, utilisateur_id: "u1" }),
      ecrire("paiements", { id: "p2", reservation_id: "r1", montant: 5_000, type: "remboursement", mode: "especes", date: `${J}T10:00:00.000Z`, utilisateur_id: "u1" }),
      ecrire("paiements", { id: "p3", reservation_id: "r1", montant: 7_000, type: "acompte", mode: "mobile", date: "2026-11-16T23:59:59.999Z", utilisateur_id: "u1" }),
      ecrire("depenses", { id: "d1", categorie: "Eau", montant: 2_000, date: J, annulee_le: null }),
      ecrire("depenses", { id: "d2", categorie: "Erreur", montant: 9_000, date: J, annulee_le: `${J}T11:00:00.000Z` }),
    ]);
    return copie;
  }

  it("caisse : remboursements à part, dépenses annulées et jours voisins exclus", () => {
    const copie = copieMinimale();
    const c = caisse(copie, reservations(copie), J, "2026-11-18", 0);
    expect(c.encaissements).toBe(50_000);
    expect(c.encaissements_par_mode).toEqual([{ mode: "especes", montant: 50_000 }]);
    expect(c.remboursements).toBe(5_000);
    expect(c.depenses).toBe(2_000);
    expect(c.net).toBe(43_000);
    expect(c.paiements.map((p) => p.par)).toEqual(["Awa", "Awa"]);
    // À Paris en hiver (UTC+1), l'acompte de 23 h 59 UTC la veille tombe aujourd'hui
    expect(caisse(copie, reservations(copie), J, "2026-11-18", 60).encaissements).toBe(57_000);
  });

  it("journée : un client déjà parti et une réservation annulée ne comptent pas", () => {
    const copie = copieMinimale();
    const j = journee(copie, reservations(copie), J);
    expect(j.departs).toEqual([]);
    expect(j.arrivees).toEqual([]);
    expect(j.chambres.map((c) => c.etat)).toEqual(["libre", "libre"]);
    // Remboursement : le reste dû tient compte de l'argent rendu
    expect(reservations(copie).find((r) => r.id === "r1")).toMatchObject({ total_encaisse: 52_000, solde: -2_000 });
  });

  it("taxes : arrondi au plus proche, comme Tanana", () => {
    // 33 333 × 19,25 % = 6 416,6 → 6 417
    expect(taxesAjoutees(33_333, 1, [{ calcul: "pourcentage", valeur: 1925, comprise: false }])).toBe(6_417);
    // TVA 19,25 % comprise dans 100 000 → hors taxes 83 857 ; 10 % ajoutés → 8 385,7 → 8 386
    expect(
      taxesAjoutees(100_000, 1, [
        { calcul: "pourcentage", valeur: 1925, comprise: true },
        { calcul: "pourcentage", valeur: 1000, comprise: false },
      ]),
    ).toBe(8_386);
  });
});
