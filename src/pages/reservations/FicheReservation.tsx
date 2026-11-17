import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, Banknote, CalendarX, DoorClosed, DoorOpen, FileText, Pencil, Percent, Printer, Undo2 } from "lucide-react";

import type { Chambre } from "../../bindings/Chambre";
import type { DetailTaxes } from "../../bindings/DetailTaxes";
import type { DocumentEmis } from "../../bindings/DocumentEmis";
import type { ModePaiement } from "../../bindings/ModePaiement";
import type { Paiement } from "../../bindings/Paiement";
import type { Reservation } from "../../bindings/Reservation";
import type { Taxe } from "../../bindings/Taxe";
import type { StatutReservation } from "../../bindings/StatutReservation";
import { AvisDocument, useImpression } from "../../components/impression";
import { ChampDate, Compteur } from "../../components/saisie";
import { EditeurTaxes, ResumeTaxes } from "../../components/taxes";
import { Alerte, Bouton, Carte, Champ, Pastille, Selection, entreePourValider, useConfirmation } from "../../components/ui";
import { api } from "../../lib/api";
import { ajouterJours, formatJourLong, formatSejour, joursEntre, pluriel } from "../../lib/dates";
import { versErreur } from "../../lib/erreurs";
import { formatHorodatage, formatMontant, lireMontant, saisieMontant, type Devise } from "../../lib/format";
import { LIBELLE_MODE, LIBELLE_STATUT_RESERVATION, LIBELLE_TYPE_PAIEMENT, MODES_PAIEMENT } from "../../lib/libelles";
import { useDevise, useSession } from "../../lib/session";

type Panneau = null | "encaisser" | "rembourser" | "modifier" | "annuler" | "taxes";

const TON_STATUT: Record<StatutReservation, "neutre" | "succes" | "alerte" | "danger"> = {
  confirmee: "succes",
  arrivee: "alerte",
  partie: "neutre",
  annulee: "danger",
};

export function FicheReservation({
  id,
  messageInitial,
  libelleRetour = "Retour au tableau",
  onRetour,
}: {
  id: string;
  messageInitial?: string;
  libelleRetour?: string;
  onRetour: () => void;
}) {
  const { peut } = useSession();
  const devise = useDevise();
  const { confirmer, fenetre } = useConfirmation();
  const [r, setR] = useState<Reservation | null>(null);
  const [paiements, setPaiements] = useState<Paiement[]>([]);
  const [panneau, setPanneau] = useState<Panneau>(null);
  const [message, setMessage] = useState<string | null>(messageInitial ?? null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [enCours, setEnCours] = useState(false);
  const [documents, setDocuments] = useState<DocumentEmis[]>([]);
  /** Paiement qui vient d'être enregistré : on propose tout de suite son reçu. */
  const [recuPropose, setRecuPropose] = useState<string | null>(null);
  const impression = useImpression();

  const chargerDocuments = useCallback(() => {
    api.documentsReservation(id).then(setDocuments, () => setDocuments([]));
  }, [id]);
  useEffect(chargerDocuments, [chargerDocuments]);

  const [taxes, setTaxes] = useState<DetailTaxes | null>(null);
  const chargerTaxes = useCallback(() => {
    api.taxesReservation(id).then(setTaxes, () => setTaxes(null));
  }, [id]);
  useEffect(chargerTaxes, [chargerTaxes]);

  const charger = useCallback(async () => {
    const [reservation, p] = await Promise.all([api.obtenirReservation(id), api.paiementsReservation(id)]);
    setR(reservation);
    setPaiements(p);
    return reservation;
  }, [id]);

  useEffect(() => {
    charger().catch((e) => setErreur(versErreur(e).message));
  }, [charger]);

  // Le résultat d'une action s'affiche en haut de la fiche : on le ramène à l'écran.
  const zoneMessages = useRef<HTMLDivElement>(null);
  const avisImpression = impression.document ?? impression.erreur;
  useEffect(() => {
    if (message || erreur || avisImpression) zoneMessages.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
  }, [message, erreur, avisImpression]);

  /** Fait l'action, recharge la fiche, puis affiche le message de réussite. */
  const executer = async <T,>(action: () => Promise<T>, succes: (apres: Reservation) => string): Promise<T | null> => {
    setEnCours(true);
    setErreur(null);
    setMessage(null);
    setRecuPropose(null);
    impression.effacer();
    try {
      const resultat = await action();
      const apres = await charger();
      chargerTaxes();
      setPanneau(null);
      setMessage(succes(apres));
      return resultat;
    } catch (e) {
      setErreur(versErreur(e).message);
      return null;
    } finally {
      setEnCours(false);
    }
  };

  const imprimer = async (lancer: () => Promise<unknown>) => {
    setMessage(null);
    setErreur(null);
    setRecuPropose(null);
    await lancer();
    chargerDocuments();
  };

  const retour = (
    <Bouton variante="secondaire" icone={ArrowLeft} onClick={onRetour} className="mb-5">
      {libelleRetour}
    </Bouton>
  );

  if (!r) {
    return (
      <>
        {retour}
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      </>
    );
  }

  const m = (montant: number) => formatMontant(montant, devise);
  const ouvrir = (p: Panneau) => {
    setPanneau(p);
    setErreur(null);
    setMessage(null);
  };

  const arrivee = async () => {
    const ok = await confirmer({
      titre: `Enregistrer l'arrivée de ${r.client_nom} ?`,
      message: `Le client s'installe dans la chambre ${r.chambre_numero}.`,
      libelleConfirmer: "Oui, le client est arrivé",
    });
    if (ok) await executer(() => api.enregistrerArrivee(r.id), () => `L'arrivée de ${r.client_nom} est enregistrée.`);
  };

  const depart = async () => {
    const ok = await confirmer({
      titre: `Enregistrer le départ de ${r.client_nom} ?`,
      message: `Le séjour sera terminé et la chambre ${r.chambre_numero} sera libérée.`,
      libelleConfirmer: "Oui, le client est parti",
    });
    if (ok)
      await executer(
        () => api.enregistrerDepart(r.id),
        () => `Le départ de ${r.client_nom} est enregistré. La chambre ${r.chambre_numero} est libre.`,
      );
  };

  const encaisser = async (montant: number, mode: ModePaiement, note: string) => {
    const ok = await confirmer({
      titre: `Encaisser ${m(montant)} ?`,
      message: `Paiement en « ${LIBELLE_MODE[mode]} » pour ${r.client_nom}. Un paiement enregistré ne peut plus être effacé.`,
      libelleConfirmer: "Oui, encaisser",
    });
    if (!ok) return;
    const paiement = await executer(
      () =>
        api.encaisser({
          reservation_id: r.id,
          montant,
          type_paiement: montant === r.solde ? "solde" : "acompte",
          mode,
          note: note.trim() || null,
        }),
      (apres) =>
        apres.solde > 0
          ? `Paiement de ${m(montant)} enregistré. Il reste ${m(apres.solde)} à payer.`
          : `Paiement de ${m(montant)} enregistré. Tout est payé.`,
    );
    if (paiement && peut("paiement.encaisser")) setRecuPropose(paiement.id);
  };

  const rembourser = async (montant: number, mode: ModePaiement, motif: string) => {
    const ok = await confirmer({
      titre: `Rembourser ${m(montant)} à ${r.client_nom} ?`,
      message: `Vous rendez cet argent au client (${LIBELLE_MODE[mode]}). Ce remboursement sera noté dans l'historique.`,
      libelleConfirmer: "Oui, rembourser",
      danger: true,
    });
    if (!ok) return;
    const paiement = await executer(() => api.rembourser(r.id, montant, mode, motif), () => `Remboursement de ${m(montant)} enregistré.`);
    if (paiement && peut("paiement.encaisser")) setRecuPropose(paiement.id);
  };

  const annuler = async (motif: string) => {
    const ok = await confirmer({
      titre: `Annuler la réservation de ${r.client_nom} ?`,
      message:
        r.total_encaisse > 0
          ? `La chambre ${r.chambre_numero} sera libérée. Attention : les ${m(r.total_encaisse)} déjà payés ne sont pas rendus automatiquement.`
          : `La chambre ${r.chambre_numero} sera libérée. Cette action ne peut pas être défaite.`,
      libelleConfirmer: "Oui, annuler la réservation",
      danger: true,
    });
    if (ok)
      await executer(
        () => api.annulerReservation(r.id, motif),
        (apres) =>
          apres.total_encaisse > 0
            ? `Réservation annulée. Le client avait payé ${m(apres.total_encaisse)} : utilisez « Rembourser » si vous devez lui rendre.`
            : "Réservation annulée. La chambre est libre.",
      );
  };

  const changerTaxes = async (nouvelles: Taxe[]) => {
    // Si le total à payer change, on le dit clairement avant d'enregistrer.
    let apercu: DetailTaxes;
    try {
      apercu = await api.calculerTaxes(r.prix_total, r.nuits, nouvelles);
    } catch (e) {
      setErreur(versErreur(e).message);
      return;
    }
    if (apercu.total_a_payer !== r.total_a_payer) {
      const ok = await confirmer({
        titre: "Enregistrer les nouvelles taxes ?",
        message: `${r.client_nom} devra payer ${m(apercu.total_a_payer)} en tout, au lieu de ${m(r.total_a_payer)}.`,
        libelleConfirmer: "Oui, enregistrer",
      });
      if (!ok) return;
    }
    await executer(
      () => api.modifierTaxesReservation(r.id, nouvelles),
      (apres) =>
        `Les taxes sont enregistrées. Le client doit ${m(apres.total_a_payer)} en tout` +
        (apres.solde > 0 ? `, il reste ${m(apres.solde)} à payer.` : "."),
    );
  };

  const active = r.statut === "confirmee" || r.statut === "arrivee";
  const peutEncaisser = active && r.solde > 0 && peut("paiement.encaisser");
  const peutRembourser = r.total_encaisse > 0 && peut("paiement.rembourser");
  // Remettre un reçu ou une facture va avec le droit d'encaisser.
  const peutImprimer = peut("paiement.encaisser");
  const peutChangerTaxes = r.statut !== "annulee" && peut("reservation.modifier");
  const avecTaxes = (taxes?.lignes.length ?? 0) > 0;
  const cartesSecondaires = panneau !== "encaisser" && panneau !== "rembourser" && panneau !== "taxes";
  const remplacees = new Set(documents.map((d) => d.remplace_numero).filter(Boolean));

  return (
    <>
      {fenetre}
      {retour}
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight text-marque-fonce">{r.client_nom}</h1>
          <p className="mt-1.5 text-lg text-gris">
            Chambre <strong className="text-encre">{r.chambre_numero}</strong> · {formatSejour(r.date_arrivee, r.date_depart)} ·{" "}
            {pluriel(r.nuits, "nuit")}
          </p>
        </div>
        <span className="text-lg">
          <Pastille ton={TON_STATUT[r.statut]}>{LIBELLE_STATUT_RESERVATION[r.statut]}</Pastille>
        </span>
      </div>

      <div className="grid max-w-5xl gap-6">
        <div ref={zoneMessages} className="grid scroll-mt-6 gap-4 empty:hidden">
          {message && <Alerte type="succes">{message}</Alerte>}
          {recuPropose && (
            <div>
              <Bouton
                icone={Printer}
                grand
                enCours={impression.enCours === recuPropose}
                onClick={() => void imprimer(() => impression.imprimerRecu(recuPropose))}
              >
                Imprimer le reçu pour le client
              </Bouton>
            </div>
          )}
          {impression.document && <AvisDocument document={impression.document} />}
          {impression.erreur && <Alerte type="erreur">{impression.erreur}</Alerte>}
          {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        </div>
        {r.notes && <Alerte type="info" titre="Remarque">{r.notes}</Alerte>}

        <Carte titre="Paiement">
          <div className="grid grid-cols-3 gap-4">
            {r.taxes_ajoutees > 0 ? (
              <Chiffre libelle="Total à payer" detail={`Chambre ${m(r.prix_total)} + taxes ${m(r.taxes_ajoutees)}`}>
                {m(r.total_a_payer)}
              </Chiffre>
            ) : (
              <Chiffre libelle="Prix du séjour">{m(r.prix_total)}</Chiffre>
            )}
            <Chiffre libelle="Déjà payé">{m(r.total_encaisse)}</Chiffre>
            {r.solde > 0 ? (
              <Chiffre libelle="Reste à payer" ton="orange">
                {m(r.solde)}
              </Chiffre>
            ) : r.solde < 0 ? (
              <Chiffre libelle="Trop payé, à rendre" ton="danger">
                {m(-r.solde)}
              </Chiffre>
            ) : (
              <Chiffre libelle="Reste à payer" ton="succes">
                Tout est payé
              </Chiffre>
            )}
          </div>

          {panneau !== "taxes" && (avecTaxes || peutChangerTaxes) && (
            <div className="mt-5 rounded-xl border-2 border-trait px-4 py-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <p className="text-lg font-bold">Taxes</p>
                {peutChangerTaxes && panneau === null && (
                  <Bouton variante="secondaire" icone={Percent} onClick={() => ouvrir("taxes")}>
                    Modifier les taxes
                  </Bouton>
                )}
              </div>
              <div className="mt-2">
                {avecTaxes && taxes ? (
                  <ResumeTaxes detail={taxes} devise={devise} />
                ) : (
                  <p className="text-gris">Aucune taxe sur ce séjour.</p>
                )}
              </div>
            </div>
          )}
          {panneau === "taxes" && taxes && (
            <div className="mt-5">
              <p className="mb-3 text-xl font-bold">Taxes de ce séjour</p>
              <EditeurTaxes
                titre="Taxes de ce séjour"
                initiales={taxes.lignes.map((l) => l.taxe)}
                devise={devise}
                apercu={{ prix: r.prix_total, nuits: r.nuits }}
                libelleValider="Enregistrer les taxes"
                enCours={enCours}
                onValider={(t) => void changerTaxes(t)}
                onFermer={() => ouvrir(null)}
              />
            </div>
          )}

          {paiements.length > 0 && (
            <ul className="mt-5 divide-y divide-trait border-y border-trait" aria-label="Paiements enregistrés">
              {paiements.map((p) => (
                <li key={p.id} className="flex min-h-14 flex-wrap items-center justify-between gap-x-4 py-2">
                  <span>
                    <span className="font-semibold">{LIBELLE_TYPE_PAIEMENT[p.type_paiement]}</span>
                    <span className="text-gris">
                      {" "}
                      · {LIBELLE_MODE[p.mode]} · {formatHorodatage(p.date)} · par {p.utilisateur_nom}
                    </span>
                    {p.note && <span className="block text-base text-gris">« {p.note} »</span>}
                  </span>
                  <span className="ml-auto flex items-center gap-4">
                    <span className={`text-lg font-bold ${p.type_paiement === "remboursement" ? "text-danger" : ""}`}>
                      {p.type_paiement === "remboursement" ? "− " : ""}
                      {m(p.montant)}
                    </span>
                    {peutImprimer && (
                      <Bouton
                        variante="secondaire"
                        icone={Printer}
                        enCours={impression.enCours === p.id}
                        aria-label={`Imprimer le reçu : ${LIBELLE_TYPE_PAIEMENT[p.type_paiement].toLowerCase()} de ${m(p.montant)}`}
                        onClick={() => void imprimer(() => impression.imprimerRecu(p.id))}
                      >
                        Reçu
                      </Bouton>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          )}

          {panneau === null && (peutEncaisser || peutRembourser) && (
            <div className="mt-5 flex flex-wrap gap-3">
              {peutEncaisser && (
                <Bouton icone={Banknote} onClick={() => ouvrir("encaisser")} grand>
                  Encaisser un paiement
                </Bouton>
              )}
              {peutRembourser && (
                <Bouton variante="secondaire" icone={Undo2} onClick={() => ouvrir("rembourser")}>
                  Rembourser
                </Bouton>
              )}
            </div>
          )}
          {panneau === "encaisser" && (
            <FormulaireArgent
              titre="Encaisser un paiement"
              montantPropose={r.solde}
              plafond={r.solde}
              messagePlafond={`Le client ne doit plus que ${m(r.solde)}. Vous ne pouvez pas encaisser plus.`}
              libelleTexte="Remarque"
              texteObligatoire={false}
              libelleValider="Encaisser"
              devise={devise}
              enCours={enCours}
              onValider={encaisser}
              onFermer={() => ouvrir(null)}
            />
          )}
          {panneau === "rembourser" && (
            <FormulaireArgent
              titre="Rembourser le client"
              montantPropose={r.solde < 0 ? -r.solde : r.total_encaisse}
              plafond={r.total_encaisse}
              messagePlafond={`Le client a payé ${m(r.total_encaisse)} en tout. Vous ne pouvez pas rendre plus.`}
              libelleTexte="Raison du remboursement"
              texteObligatoire
              libelleValider="Rembourser"
              devise={devise}
              enCours={enCours}
              onValider={rembourser}
              onFermer={() => ouvrir(null)}
            />
          )}
        </Carte>

        {cartesSecondaires && (peutImprimer || documents.length > 0) && (
          <Carte titre="Reçus et factures">
            {r.statut !== "annulee" && peutImprimer && avecTaxes && (
              <p className="mb-3 text-gris">La facture reprend les taxes indiquées dans « Paiement », plus haut.</p>
            )}
            {r.statut !== "annulee" && peutImprimer && (
              <Bouton
                icone={FileText}
                enCours={impression.enCours === `facture-${r.id}`}
                onClick={() => void imprimer(() => impression.imprimerFacture(r.id))}
                className="mb-4"
              >
                Imprimer la facture
              </Bouton>
            )}
            {documents.length === 0 ? (
              <p className="text-gris">
                Aucun reçu ni facture imprimé pour cette réservation.
                {paiements.length > 0 && peutImprimer && " Le bouton « Reçu » à côté de chaque paiement imprime son reçu."}
              </p>
            ) : (
              <ul aria-label="Documents imprimés" className="divide-y divide-trait border-y border-trait">
                {documents.map((d) => (
                  <li key={d.id} className="flex min-h-12 flex-wrap items-center gap-x-2 py-2">
                    <span className="font-semibold">
                      {d.type_document === "recu" ? "Reçu" : "Facture"} {d.numero}
                    </span>{" "}
                    <span className="text-gris">
                      · {formatHorodatage(d.emis_le)} · par {d.emis_par_nom}
                    </span>
                    {remplacees.has(d.numero) && <Pastille ton="neutre">Remplacée</Pastille>}
                  </li>
                ))}
              </ul>
            )}
          </Carte>
        )}

        {cartesSecondaires && (
          <Carte titre="Séjour">
            {r.statut === "partie" && <p className="text-lg text-gris">Le séjour est terminé.</p>}
            {r.statut === "annulee" && <p className="text-lg text-gris">Cette réservation a été annulée.</p>}

            {active && panneau !== "modifier" && panneau !== "annuler" && (
              <div className="grid gap-4">
                {r.statut === "arrivee" && r.solde > 0 && peut("sejour.gerer") && (
                  <Alerte type="attention">
                    Avant d'enregistrer le départ, encaissez le reste à payer ({m(r.solde)}).
                  </Alerte>
                )}
                <div className="flex flex-wrap gap-3">
                  {r.statut === "confirmee" && peut("sejour.gerer") && (
                    <Bouton icone={DoorOpen} onClick={() => void arrivee()} enCours={enCours} grand>
                      Le client est arrivé
                    </Bouton>
                  )}
                  {r.statut === "arrivee" && peut("sejour.gerer") && (
                    <Bouton icone={DoorClosed} onClick={() => void depart()} disabled={r.solde > 0} enCours={enCours} grand>
                      Le client est parti
                    </Bouton>
                  )}
                  {peut("reservation.modifier") && (
                    <Bouton variante="secondaire" icone={Pencil} onClick={() => ouvrir("modifier")}>
                      Modifier
                    </Bouton>
                  )}
                  {r.statut === "confirmee" && peut("reservation.annuler") && (
                    <Bouton variante="secondaire" icone={CalendarX} onClick={() => ouvrir("annuler")}>
                      Annuler la réservation
                    </Bouton>
                  )}
                </div>
              </div>
            )}

            {panneau === "annuler" && <FormulaireAnnulation enCours={enCours} onValider={annuler} onFermer={() => ouvrir(null)} />}
            {panneau === "modifier" && (
              <FormulaireModification
                reservation={r}
                devise={devise}
                enCours={enCours}
                onFermer={() => ouvrir(null)}
                onValider={async (donnees) => {
                  const ok = await confirmer({
                    titre: "Enregistrer les changements ?",
                    message: `${r.client_nom} : ${formatSejour(donnees.date_arrivee, donnees.date_depart)}, prix ${m(donnees.prix_total)}.`,
                    libelleConfirmer: "Oui, enregistrer",
                  });
                  if (ok) await executer(() => api.modifierReservation(r.id, donnees), () => "Les changements sont enregistrés.");
                }}
              />
            )}
          </Carte>
        )}
      </div>
    </>
  );
}

const TON_CHIFFRE = {
  neutre: "border-trait bg-white",
  orange: "border-orange bg-orange-clair text-orange-texte",
  succes: "border-succes/40 bg-succes-clair text-succes",
  danger: "border-danger/40 bg-danger-clair text-danger",
};

function Chiffre({
  libelle,
  ton = "neutre",
  detail,
  children,
}: {
  libelle: string;
  ton?: keyof typeof TON_CHIFFRE;
  detail?: string;
  children: ReactNode;
}) {
  return (
    <div className={`rounded-xl border-2 px-4 py-3 ${TON_CHIFFRE[ton]}`}>
      <p className="text-base font-semibold">{libelle}</p>
      <p className="text-2xl font-bold">{children}</p>
      {detail && <p className="mt-0.5 text-sm text-gris">{detail}</p>}
    </div>
  );
}

function ChoixMode({ valeur, onChange }: { valeur: ModePaiement; onChange: (m: ModePaiement) => void }) {
  return (
    <div>
      <p className="mb-1.5 font-semibold">Moyen de paiement</p>
      <div role="radiogroup" aria-label="Moyen de paiement" className="flex flex-wrap gap-2">
        {MODES_PAIEMENT.map((mode) => (
          <label
            key={mode}
            className="flex min-h-12 cursor-pointer items-center gap-2 rounded-xl border-2 border-trait px-4 font-semibold has-[:checked]:border-marque has-[:checked]:bg-marque-clair hover:border-marque/40"
          >
            <input type="radio" name="mode-paiement" className="size-5 accent-marque" checked={valeur === mode} onChange={() => onChange(mode)} />
            {LIBELLE_MODE[mode]}
          </label>
        ))}
      </div>
    </div>
  );
}

/** Formulaire commun à l'encaissement et au remboursement. */
function FormulaireArgent({
  titre,
  montantPropose,
  plafond,
  messagePlafond,
  libelleTexte,
  texteObligatoire,
  libelleValider,
  devise,
  enCours,
  onValider,
  onFermer,
}: {
  titre: string;
  montantPropose: number;
  plafond: number;
  messagePlafond: string;
  libelleTexte: string;
  texteObligatoire: boolean;
  libelleValider: string;
  devise: Devise;
  enCours: boolean;
  onValider: (montant: number, mode: ModePaiement, texte: string) => void;
  onFermer: () => void;
}) {
  const [saisie, setSaisie] = useState(saisieMontant(montantPropose, devise.decimales));
  const [mode, setMode] = useState<ModePaiement>("especes");
  const [texte, setTexte] = useState("");
  const [erreur, setErreur] = useState<string | null>(null);

  const valider = () => {
    const montant = lireMontant(saisie, devise.decimales);
    if (montant === null || montant <= 0) return setErreur("Le montant n'est pas lisible. Écrivez seulement des chiffres, par exemple 10000.");
    if (montant > plafond) return setErreur(messagePlafond);
    if (texteObligatoire && !texte.trim()) return setErreur(`Écrivez la ${libelleTexte.toLowerCase()}.`);
    setErreur(null);
    onValider(montant, mode, texte.trim());
  };

  return (
    <div role="group" aria-label={titre} onKeyDown={entreePourValider(valider)} className="apparition mt-5 grid gap-5 rounded-xl border-2 border-marque/30 bg-papier p-5">
      <p className="text-xl font-bold">{titre}</p>
      <div className="flex flex-wrap items-end gap-3">
        <Champ
          className="w-64"
          libelle={`Montant (${devise.code})`}
          value={saisie}
          onChange={(e) => setSaisie(e.target.value)}
          inputMode="decimal"
          autoFocus
        />
        {saisieMontant(montantPropose, devise.decimales) !== saisie && (
          <Bouton variante="discret" onClick={() => setSaisie(saisieMontant(montantPropose, devise.decimales))}>
            Remettre {formatMontant(montantPropose, devise)}
          </Bouton>
        )}
      </div>
      <ChoixMode valeur={mode} onChange={setMode} />
      <Champ
        libelle={texteObligatoire ? libelleTexte : `${libelleTexte} (facultatif)`}
        value={texte}
        onChange={(e) => setTexte(e.target.value)}
      />
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      <div className="flex justify-end gap-3">
        <Bouton variante="secondaire" onClick={onFermer}>
          Fermer
        </Bouton>
        <Bouton onClick={valider} enCours={enCours} grand>
          {libelleValider}
        </Bouton>
      </div>
    </div>
  );
}

function FormulaireAnnulation({
  enCours,
  onValider,
  onFermer,
}: {
  enCours: boolean;
  onValider: (motif: string) => void;
  onFermer: () => void;
}) {
  const [motif, setMotif] = useState("");
  const [erreur, setErreur] = useState<string | null>(null);
  const valider = () => (motif.trim() ? onValider(motif.trim()) : setErreur("Écrivez la raison de l'annulation."));
  return (
    <div role="group" aria-label="Annuler la réservation" onKeyDown={entreePourValider(valider)} className="apparition grid gap-4 rounded-xl border-2 border-danger/30 bg-danger-clair/40 p-5">
      <p className="text-xl font-bold">Annuler la réservation</p>
      <Champ
        libelle="Raison de l'annulation"
        value={motif}
        onChange={(e) => setMotif(e.target.value)}
        placeholder="Exemple : le client a annulé par téléphone"
        autoFocus
      />
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      <div className="flex justify-end gap-3">
        <Bouton variante="secondaire" onClick={onFermer}>
          Ne pas annuler
        </Bouton>
        <Bouton variante="danger" enCours={enCours} onClick={valider}>
          Annuler la réservation
        </Bouton>
      </div>
    </div>
  );
}

function FormulaireModification({
  reservation: r,
  devise,
  enCours,
  onValider,
  onFermer,
}: {
  reservation: Reservation;
  devise: Devise;
  enCours: boolean;
  onValider: (d: { chambre_id: string; date_arrivee: string; date_depart: string; prix_total: number; notes: string | null }) => void;
  onFermer: () => void;
}) {
  const [arrivee, setArrivee] = useState(r.date_arrivee);
  const [nuits, setNuits] = useState(Math.max(1, joursEntre(r.date_arrivee, r.date_depart)));
  const [chambreId, setChambreId] = useState(r.chambre_id);
  const [prix, setPrix] = useState(saisieMontant(r.prix_total, devise.decimales));
  const [prixRecalcule, setPrixRecalcule] = useState(false);
  const [notes, setNotes] = useState(r.notes ?? "");
  const [toutes, setToutes] = useState<Chambre[]>([]);
  const [libres, setLibres] = useState<Chambre[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);
  const depart = ajouterJours(arrivee, nuits);
  const clientPresent = r.statut === "arrivee";

  useEffect(() => {
    api.listerChambres().then(setToutes, () => {});
  }, []);

  useEffect(() => {
    let annule = false;
    api.chambresDisponibles(arrivee, depart).then(
      (c) => !annule && setLibres(c),
      () => {},
    );
    return () => {
      annule = true;
    };
  }, [arrivee, depart]);

  // La chambre actuelle reste proposée : c'est cette réservation qui l'occupe.
  const options = [
    { valeur: r.chambre_id, libelle: `Chambre ${r.chambre_numero} (actuelle)` },
    ...libres.filter((c) => c.id !== r.chambre_id).map((c) => ({ valeur: c.id, libelle: `Chambre ${c.numero} — ${c.type_nom}` })),
  ];

  /** Quand la durée ou la chambre change, on propose le prix normal ; il reste modifiable. */
  const recalculer = (n: number, cId: string) => {
    const c = [...toutes, ...libres].find((x) => x.id === cId);
    if (!c) return;
    setPrix(saisieMontant(c.prix_nuit_defaut * n, devise.decimales));
    setPrixRecalcule(true);
  };

  const valider = () => {
    const montant = lireMontant(prix, devise.decimales);
    if (montant === null) return setErreur("Le prix n'est pas lisible. Écrivez seulement des chiffres.");
    setErreur(null);
    onValider({ chambre_id: chambreId, date_arrivee: arrivee, date_depart: depart, prix_total: montant, notes: notes.trim() || null });
  };

  return (
    <div role="group" aria-label="Modifier la réservation" onKeyDown={entreePourValider(valider)} className="apparition grid gap-5 rounded-xl border-2 border-marque/30 bg-papier p-5">
      <p className="text-xl font-bold">Modifier la réservation</p>
      <div className="flex flex-wrap items-start gap-x-12 gap-y-5">
        <ChampDate
          libelle="Jour d'arrivée"
          valeur={arrivee}
          onChange={setArrivee}
          desactive={clientPresent}
          aide={clientPresent ? "Le client est déjà arrivé : ce jour ne change plus." : undefined}
        />
        <Compteur
          libelle="Nombre de nuits"
          valeur={nuits}
          onChange={(n) => {
            setNuits(n);
            recalculer(n, chambreId);
          }}
          unite="nuit"
        />
      </div>
      <p className="rounded-xl bg-sable px-4 py-3 text-lg">
        Départ le <strong>{formatJourLong(depart)}</strong>
      </p>
      <Selection
        className="max-w-md"
        libelle="Chambre"
        valeur={chambreId}
        onChange={(cId) => {
          setChambreId(cId);
          recalculer(nuits, cId);
        }}
        options={options}
        aide="Seules les chambres libres à ces dates sont proposées."
      />
      <Champ
        className="max-w-sm"
        libelle={`Prix du séjour (${devise.code})`}
        value={prix}
        onChange={(e) => {
          setPrix(e.target.value);
          setPrixRecalcule(false);
        }}
        inputMode="decimal"
        aide={prixRecalcule ? "Prix recalculé avec le tarif normal. Corrigez-le si un autre prix a été convenu." : undefined}
      />
      <Champ libelle="Remarque (facultatif)" value={notes} onChange={(e) => setNotes(e.target.value)} />
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      <div className="flex justify-end gap-3">
        <Bouton variante="secondaire" onClick={onFermer}>
          Fermer sans changer
        </Bouton>
        <Bouton onClick={valider} enCours={enCours} grand>
          Enregistrer les changements
        </Bouton>
      </div>
    </div>
  );
}
