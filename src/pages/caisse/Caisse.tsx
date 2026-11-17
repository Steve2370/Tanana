import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Plus, Printer } from "lucide-react";

import type { Depense } from "../../bindings/Depense";
import type { PaiementDetaille } from "../../bindings/PaiementDetaille";
import type { RapportCaisse } from "../../bindings/RapportCaisse";
import { AvisDocument, useImpression } from "../../components/impression";
import { ChampDate } from "../../components/saisie";
import { Alerte, Bouton, Carte, Champ, EnTete, Pastille, Selection, entreePourValider, useConfirmation } from "../../components/ui";
import { api } from "../../lib/api";
import { ajouterJours, debutMois, debutMoisSuivant, debutSemaine, formatJourLong, formatJourMoyen } from "../../lib/dates";
import { versErreur } from "../../lib/erreurs";
import { aujourdhui, decalageLocal, formatHorodatage, formatMontant, lireMontant, type Devise } from "../../lib/format";
import { LIBELLE_MODE, LIBELLE_TYPE_PAIEMENT } from "../../lib/libelles";
import { useDevise, useSession } from "../../lib/session";

export type ChoixPeriode = "aujourdhui" | "hier" | "semaine" | "mois" | "jour";

const PERIODES: { id: ChoixPeriode; libelle: string }[] = [
  { id: "aujourdhui", libelle: "Aujourd'hui" },
  { id: "hier", libelle: "Hier" },
  { id: "semaine", libelle: "Cette semaine" },
  { id: "mois", libelle: "Ce mois" },
  { id: "jour", libelle: "Un autre jour" },
];

/** Période `[debut, fin)` (fin exclue) et son titre en toutes lettres. */
export function calculerPeriode(choix: ChoixPeriode, jourChoisi: string, ceJour: string): { debut: string; fin: string; titre: string } {
  const unJour = (j: string) => ({ debut: j, fin: ajouterJours(j, 1), titre: formatJourLong(j) });
  const plusieurs = (debut: string, fin: string) => ({
    debut,
    fin,
    titre: `Du ${formatJourMoyen(debut)} au ${formatJourLong(ajouterJours(fin, -1))}`,
  });
  switch (choix) {
    case "aujourdhui":
      return unJour(ceJour);
    case "hier":
      return unJour(ajouterJours(ceJour, -1));
    case "semaine":
      return plusieurs(debutSemaine(ceJour), ajouterJours(debutSemaine(ceJour), 7));
    case "mois":
      return plusieurs(debutMois(ceJour), debutMoisSuivant(ceJour));
    case "jour":
      return unJour(jourChoisi);
  }
}

export const CATEGORIES_DEPENSE = [
  "Électricité",
  "Eau",
  "Salaires",
  "Entretien et réparations",
  "Fournitures",
  "Nourriture et boissons",
  "Internet et téléphone",
  "Taxes et impôts",
];
const AUTRE = "Autre";

export function Caisse() {
  const { peut } = useSession();
  const devise = useDevise();
  const { confirmer, fenetre } = useConfirmation();
  const ceJour = aujourdhui();
  const [choix, setChoix] = useState<ChoixPeriode>("aujourdhui");
  const [jourChoisi, setJourChoisi] = useState(ceJour);
  const { debut, fin, titre } = calculerPeriode(choix, jourChoisi, ceJour);

  const voitArgent = peut("rapport.financier.voir");
  const peutImprimer = peut("paiement.encaisser");
  const impression = useImpression();
  // Le résultat d'une impression s'affiche en haut : on le ramène à l'écran.
  const zoneMessages = useRef<HTMLDivElement>(null);
  const avisImpression = impression.document ?? impression.erreur;
  useEffect(() => {
    if (avisImpression) zoneMessages.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
  }, [avisImpression]);
  const voitDepenses = peut("depense.voir");
  const peutSaisir = peut("depense.saisir");

  const [rapport, setRapport] = useState<RapportCaisse | null>(null);
  const [paiements, setPaiements] = useState<PaiementDetaille[]>([]);
  const [depenses, setDepenses] = useState<Depense[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [ajout, setAjout] = useState(false);
  const [aAnnuler, setAAnnuler] = useState<string | null>(null);

  const charger = useCallback(async () => {
    const decalage = decalageLocal();
    const [r, p, d] = await Promise.all([
      voitArgent ? api.rapportCaisse(debut, fin, decalage) : null,
      voitArgent ? api.listerPaiements(debut, fin, decalage) : [],
      voitDepenses ? api.listerDepenses(debut, fin) : [],
    ]);
    setRapport(r);
    setPaiements(p);
    setDepenses(d);
  }, [debut, fin, voitArgent, voitDepenses]);

  useEffect(() => {
    let annule = false;
    setErreur(null);
    charger().catch((e) => !annule && setErreur(versErreur(e).message));
    return () => {
      annule = true;
    };
  }, [charger]);

  const m = (montant: number) => formatMontant(montant, devise);

  const apresChangement = async (succes: string) => {
    setAjout(false);
    setAAnnuler(null);
    setMessage(succes);
    try {
      await charger();
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  };

  const enregistrerDepense = async (d: { categorie: string; montant: number; description: string | null; date: string }) => {
    const ok = await confirmer({
      titre: `Enregistrer une dépense de ${m(d.montant)} ?`,
      message: `${d.categorie}, le ${formatJourLong(d.date)}.`,
      libelleConfirmer: "Oui, enregistrer",
    });
    if (!ok) return;
    setErreur(null);
    try {
      await api.saisirDepense(d);
      await apresChangement(`Dépense de ${m(d.montant)} enregistrée (${d.categorie}).`);
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  };

  const annulerDepense = async (d: Depense, motif: string) => {
    const ok = await confirmer({
      titre: `Annuler la dépense de ${m(d.montant)} ?`,
      message: `${d.categorie}, le ${formatJourLong(d.date)}. Elle restera visible, barrée, mais ne comptera plus dans les totaux.`,
      libelleConfirmer: "Oui, annuler cette dépense",
      danger: true,
    });
    if (!ok) return;
    setErreur(null);
    try {
      await api.annulerDepense(d.id, motif);
      await apresChangement(`La dépense de ${m(d.montant)} est annulée.`);
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  };

  // Sans accès au rapport, on calcule au moins le total des dépenses affichées.
  const totalDepenses = rapport?.depenses ?? depenses.filter((d) => !d.annulee).reduce((s, d) => s + d.montant, 0);
  const parEmploye = new Map<string, number>();
  for (const p of paiements) {
    if (p.type_paiement !== "remboursement") parEmploye.set(p.utilisateur_nom, (parEmploye.get(p.utilisateur_nom) ?? 0) + p.montant);
  }

  return (
    <>
      {fenetre}
      <EnTete titre="Caisse" sousTitre="L'argent qui est entré et celui qui est sorti." />

      <div className="mb-6 flex flex-wrap items-start gap-x-8 gap-y-4">
        <div role="group" aria-label="Période" className="flex flex-wrap gap-1 rounded-xl bg-sable p-1.5">
          {PERIODES.map((p) => (
            <button
              key={p.id}
              type="button"
              aria-pressed={choix === p.id}
              onClick={() => {
                setChoix(p.id);
                setMessage(null);
              }}
              className={`min-h-12 rounded-lg px-4 text-base font-semibold ${choix === p.id ? "bg-white text-marque-fonce shadow-sm" : "text-gris hover:text-encre"}`}
            >
              {p.libelle}
            </button>
          ))}
        </div>
        {choix === "jour" && <ChampDate libelle="Jour à afficher" valeur={jourChoisi} onChange={setJourChoisi} />}
      </div>

      <h2 className="mb-4 text-2xl font-bold first-letter:uppercase">{titre}</h2>

      <div className="grid gap-6">
        <div ref={zoneMessages} className="grid scroll-mt-6 gap-4 empty:hidden">
          {message && <Alerte type="succes">{message}</Alerte>}
          {erreur && <Alerte type="erreur">{erreur}</Alerte>}
          {impression.document && <AvisDocument document={impression.document} />}
          {impression.erreur && <Alerte type="erreur">{impression.erreur}</Alerte>}
        </div>

        {voitArgent && rapport && (
          <>
            <div role="group" aria-label="Totaux de la période" className="grid grid-cols-2 gap-4">
              <Total libelle="Argent entré" ton="succes">
                {m(rapport.encaissements)}
              </Total>
              <Total libelle="Rendu aux clients">{m(rapport.remboursements)}</Total>
              <Total libelle="Dépenses">{m(rapport.depenses)}</Total>
              <Total libelle="Entrées moins sorties" ton={rapport.net < 0 ? "danger" : "marque"}>
                {m(rapport.net)}
              </Total>
            </div>
            {rapport.encaissements_par_mode.length > 0 && (
              <ul aria-label="Argent entré par moyen de paiement" className="flex flex-wrap gap-3">
                {rapport.encaissements_par_mode.map((l) => (
                  <li key={l.mode} className="rounded-xl border-2 border-trait bg-white px-4 py-2">
                    <span className="text-gris">{LIBELLE_MODE[l.mode]} : </span>
                    <strong>{m(l.montant)}</strong>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}

        {voitArgent && (
          <Carte titre="Paiements des clients">
            {paiements.length === 0 ? (
              <p className="text-lg text-gris">Aucun paiement sur cette période.</p>
            ) : (
              <>
                <ul className="divide-y divide-trait border-y border-trait" aria-label="Paiements des clients">
                  {paiements.map((p) => {
                    const rendu = p.type_paiement === "remboursement";
                    return (
                      <li key={p.id} className="flex min-h-16 flex-wrap items-center justify-between gap-x-4 py-2">
                        <span>
                          <span className="text-lg font-semibold">{p.client_nom}</span>
                          <span className="text-gris"> · chambre {p.chambre_numero}</span>
                          <span className="block text-base text-gris">
                            {LIBELLE_TYPE_PAIEMENT[p.type_paiement]} · {LIBELLE_MODE[p.mode]} · {formatHorodatage(p.date)} · par{" "}
                            {p.utilisateur_nom}
                            {p.note ? ` · « ${p.note} »` : ""}
                          </span>
                        </span>
                        <span className="ml-auto flex items-center gap-4">
                          <span className={`text-xl font-bold ${rendu ? "text-danger" : "text-succes"}`}>
                            {rendu ? "− " : "+ "}
                            {m(p.montant)}
                          </span>
                          {peutImprimer && (
                            <Bouton
                              variante="secondaire"
                              icone={Printer}
                              enCours={impression.enCours === p.id}
                              aria-label={`Imprimer le reçu de ${p.client_nom} : ${m(p.montant)}`}
                              onClick={() => {
                                setMessage(null);
                                setErreur(null);
                                void impression.imprimerRecu(p.id);
                              }}
                            >
                              Reçu
                            </Bouton>
                          )}
                        </span>
                      </li>
                    );
                  })}
                </ul>
                {parEmploye.size > 0 && (
                  <p className="mt-4 text-base text-gris" aria-label="Argent encaissé par employé">
                    Encaissé par :{" "}
                    {[...parEmploye].map(([nom, total], i) => (
                      <span key={nom}>
                        {i > 0 && " · "}
                        {nom} <strong className="text-encre">{m(total)}</strong>
                      </span>
                    ))}
                  </p>
                )}
              </>
            )}
          </Carte>
        )}

        {voitDepenses && (
          <Carte
            titre={voitArgent ? "Dépenses" : `Dépenses : ${m(totalDepenses)}`}
            actions={
              peutSaisir &&
              !ajout && (
                <Bouton
                  icone={Plus}
                  onClick={() => {
                    setAjout(true);
                    setAAnnuler(null);
                    setMessage(null);
                  }}
                >
                  Ajouter une dépense
                </Bouton>
              )
            }
          >
            {ajout && (
              <FormulaireDepense
                devise={devise}
                jourPropose={choix === "jour" || choix === "hier" ? debut : ceJour}
                onValider={enregistrerDepense}
                onFermer={() => setAjout(false)}
              />
            )}
            {depenses.length === 0 ? (
              !ajout && <p className="text-lg text-gris">Aucune dépense sur cette période.</p>
            ) : (
              <ul className="divide-y divide-trait border-y border-trait" aria-label="Dépenses">
                {depenses.map((d) => (
                  <li key={d.id} className="py-2">
                    <div className="flex min-h-14 flex-wrap items-center justify-between gap-x-4">
                      <span>
                        <span className={`text-lg font-semibold ${d.annulee ? "text-gris line-through" : ""}`}>{d.categorie}</span>{" "}
                        {d.annulee && <Pastille ton="danger">Annulée</Pastille>}
                        <span className="block text-base text-gris">
                          {d.description ? `${d.description} · ` : ""}
                          {formatJourMoyen(d.date)} · par {d.utilisateur_nom}
                          {d.annulee && d.motif_annulation ? ` · raison : ${d.motif_annulation}` : ""}
                        </span>
                      </span>
                      <span className="flex items-center gap-4">
                        <span className={`text-xl font-bold ${d.annulee ? "text-gris line-through" : ""}`}>{m(d.montant)}</span>
                        {peutSaisir && !d.annulee && aAnnuler !== d.id && (
                          <Bouton
                            variante="secondaire"
                            aria-label={`Annuler la dépense ${d.categorie} de ${m(d.montant)}`}
                            onClick={() => {
                              setAAnnuler(d.id);
                              setAjout(false);
                              setMessage(null);
                            }}
                          >
                            Annuler
                          </Bouton>
                        )}
                      </span>
                    </div>
                    {aAnnuler === d.id && (
                      <FormulaireAnnulationDepense onValider={(motif) => void annulerDepense(d, motif)} onFermer={() => setAAnnuler(null)} />
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Carte>
        )}
      </div>
    </>
  );
}

const TON_TOTAL = {
  neutre: "border-trait bg-white",
  succes: "border-succes/40 bg-succes-clair text-succes",
  marque: "border-marque/40 bg-marque-clair text-marque-fonce",
  danger: "border-danger/40 bg-danger-clair text-danger",
};

function Total({ libelle, ton = "neutre", children }: { libelle: string; ton?: keyof typeof TON_TOTAL; children: ReactNode }) {
  return (
    <div className={`rounded-xl border-2 px-4 py-3 ${TON_TOTAL[ton]}`}>
      <p className="text-base font-semibold">{libelle}</p>
      <p className="text-3xl font-bold whitespace-nowrap">{children}</p>
    </div>
  );
}

function FormulaireDepense({
  devise,
  jourPropose,
  onValider,
  onFermer,
}: {
  devise: Devise;
  jourPropose: string;
  onValider: (d: { categorie: string; montant: number; description: string | null; date: string }) => void;
  onFermer: () => void;
}) {
  const [categorie, setCategorie] = useState(CATEGORIES_DEPENSE[0]!);
  const [autre, setAutre] = useState("");
  const [saisie, setSaisie] = useState("");
  const [description, setDescription] = useState("");
  const [date, setDate] = useState(jourPropose);
  const [erreur, setErreur] = useState<string | null>(null);

  const valider = () => {
    const nom = categorie === AUTRE ? autre.trim() : categorie;
    if (!nom) return setErreur("Écrivez de quelle dépense il s'agit.");
    const montant = lireMontant(saisie, devise.decimales);
    if (montant === null || montant <= 0) return setErreur("Le montant n'est pas lisible. Écrivez seulement des chiffres, par exemple 8000.");
    setErreur(null);
    onValider({ categorie: nom, montant, description: description.trim() || null, date });
  };

  return (
    <div
      role="group"
      aria-label="Nouvelle dépense"
      onKeyDown={entreePourValider(valider)}
      className="apparition mb-5 grid gap-5 rounded-xl border-2 border-marque/30 bg-papier p-5"
    >
      <p className="text-xl font-bold">Nouvelle dépense</p>
      <div className="grid grid-cols-2 gap-5">
        <Selection
          libelle="Type de dépense"
          valeur={categorie}
          onChange={setCategorie}
          options={[...CATEGORIES_DEPENSE, AUTRE].map((c) => ({ valeur: c, libelle: c }))}
        />
        <Champ libelle={`Montant (${devise.code})`} value={saisie} onChange={(e) => setSaisie(e.target.value)} inputMode="decimal" autoFocus />
      </div>
      {categorie === AUTRE && <Champ libelle="Précisez la dépense" value={autre} onChange={(e) => setAutre(e.target.value)} />}
      <Champ
        libelle="Détail (facultatif)"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="Exemple : facture du mois d'octobre"
      />
      <ChampDate libelle="Jour de la dépense" valeur={date} onChange={setDate} />
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      <div className="flex justify-end gap-3">
        <Bouton variante="secondaire" onClick={onFermer}>
          Fermer
        </Bouton>
        <Bouton onClick={valider} grand>
          Enregistrer la dépense
        </Bouton>
      </div>
    </div>
  );
}

function FormulaireAnnulationDepense({ onValider, onFermer }: { onValider: (motif: string) => void; onFermer: () => void }) {
  const [motif, setMotif] = useState("");
  const [erreur, setErreur] = useState<string | null>(null);
  const valider = () => (motif.trim() ? onValider(motif.trim()) : setErreur("Écrivez la raison de l'annulation."));
  return (
    <div
      role="group"
      aria-label="Annuler la dépense"
      onKeyDown={entreePourValider(valider)}
      className="apparition my-2 grid gap-4 rounded-xl border-2 border-danger/30 bg-danger-clair/40 p-5"
    >
      <Champ
        libelle="Raison de l'annulation"
        value={motif}
        onChange={(e) => setMotif(e.target.value)}
        placeholder="Exemple : erreur de montant"
        autoFocus
      />
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      <div className="flex justify-end gap-3">
        <Bouton variante="secondaire" onClick={onFermer}>
          Ne pas annuler
        </Bouton>
        <Bouton variante="danger" onClick={valider}>
          Annuler cette dépense
        </Bouton>
      </div>
    </div>
  );
}
