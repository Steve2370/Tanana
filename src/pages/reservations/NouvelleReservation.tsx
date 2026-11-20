import { useEffect, useState, type ReactNode } from "react";
import { ArrowLeft, Check } from "lucide-react";

import type { Chambre } from "../../bindings/Chambre";
import type { Client } from "../../bindings/Client";
import type { DetailTaxes } from "../../bindings/DetailTaxes";
import type { Groupe } from "../../bindings/Groupe";
import type { NouveauGroupe } from "../../bindings/NouveauGroupe";
import type { NouvelleReservation as DonneesReservation } from "../../bindings/NouvelleReservation";
import type { Reservation } from "../../bindings/Reservation";
import type { Taxe } from "../../bindings/Taxe";
import { ChampDate, Compteur } from "../../components/saisie";
import { cumulerTaxes, ResumeTaxes } from "../../components/taxes";
import { Alerte, Bouton, Carte, Champ, EnTete, useAction } from "../../components/ui";
import { api } from "../../lib/api";
import { ajouterJours, formatJourLong, formatSejour, pluriel } from "../../lib/dates";
import { versErreur } from "../../lib/erreurs";
import { aujourdhui, formatMontant, lireMontant } from "../../lib/format";
import { useDevise } from "../../lib/session";
import { ChoixClient, nomComplet } from "./ChoixClient";

export function NouvelleReservation({
  chambreInitiale,
  arriveeInitiale,
  clientInitial,
  libelleRetour = "Retour au tableau",
  onRetour,
  onCree,
}: {
  chambreInitiale?: string;
  arriveeInitiale?: string;
  /** Client déjà choisi (réservation lancée depuis sa fiche). */
  clientInitial?: Client;
  libelleRetour?: string;
  onRetour: () => void;
  /** `chambres` : nombre de chambres réservées (plus d'une : un groupe, `r` est sa première chambre). */
  onCree: (r: Reservation, chambres: number) => void;
}) {
  const devise = useDevise();
  const [arrivee, setArrivee] = useState(arriveeInitiale ?? aujourdhui());
  const [nuits, setNuits] = useState(1);
  const depart = ajouterJours(arrivee, nuits);

  const [libres, setLibres] = useState<Chambre[] | null>(null);
  const [erreurChambres, setErreurChambres] = useState<string | null>(null);
  /** Chambres cochées, dans l'ordre du choix. Plusieurs : une réservation de groupe. */
  const [chambreIds, setChambreIds] = useState<string[]>(chambreInitiale ? [chambreInitiale] : []);
  const [client, setClient] = useState<Client | null>(clientInitial ?? null);
  const [prixDifferent, setPrixDifferent] = useState(false);
  /** Prix convenu, chambre par chambre. */
  const [prixSaisis, setPrixSaisis] = useState<Record<string, string>>({});
  const [nomGroupe, setNomGroupe] = useState("");
  const [notes, setNotes] = useState("");
  const { lancer, enCours, erreur, setErreur } = useAction(
    (d: { seule: DonneesReservation } | { groupe: NouveauGroupe }): Promise<Reservation> =>
      "seule" in d ? api.creerReservation(d.seule) : api.creerGroupe(d.groupe).then(premiereChambre),
  );

  // Les chambres proposées sont seulement celles qui sont libres à ces dates.
  useEffect(() => {
    let annule = false;
    setLibres(null);
    api.chambresDisponibles(arrivee, depart).then(
      (c) => {
        if (annule) return;
        setLibres(c);
        setErreurChambres(null);
      },
      (e) => !annule && setErreurChambres(versErreur(e).message),
    );
    return () => {
      annule = true;
    };
  }, [arrivee, depart]);

  // Un message d'erreur disparaît dès que la personne corrige quelque chose.
  useEffect(() => setErreur(null), [setErreur, arrivee, nuits, chambreIds, client, prixDifferent, prixSaisis]);

  const chambres = libres === null ? [] : chambreIds.flatMap((id) => libres.filter((c) => c.id === id));
  const choixPlusLibre = libres !== null && chambres.length < chambreIds.length;
  const groupe = chambres.length > 1;
  const premiere = chambres[0];
  const cocher = (id: string, coche: boolean) =>
    setChambreIds((ids) => (coche ? [...ids.filter((x) => x !== id), id] : ids.filter((x) => x !== id)));
  const prixNormal = (c: Chambre) => c.prix_nuit_defaut * nuits;
  /** Prix retenu pour chaque chambre (null : prix convenu illisible). */
  const prixRetenus = chambres.map((c) => (prixDifferent ? lireMontant(prixSaisis[c.id] ?? "", devise.decimales) : prixNormal(c)));
  const prixLisibles = prixRetenus.every((p) => p !== null);
  const totalPrix = prixRetenus.reduce<number>((t, p) => t + (p ?? 0), 0);
  const cleTaxes = chambres.length > 0 && prixLisibles ? `${nuits}:${prixRetenus.join(",")}` : null;

  // Taxes habituelles de l'hôtel : elles s'appliqueront à cette réservation.
  const [taxesHotel, setTaxesHotel] = useState<Taxe[]>([]);
  useEffect(() => {
    api.taxesHotel().then(setTaxesHotel, () => setTaxesHotel([]));
  }, []);
  const [detailTaxes, setDetailTaxes] = useState<DetailTaxes | null>(null);
  useEffect(() => {
    if (taxesHotel.length === 0 || cleTaxes === null) return setDetailTaxes(null);
    let annule = false;
    const [n = "1", prix = ""] = cleTaxes.split(":");
    // Une chambre après l'autre, puis le tout additionné (comme sur la facture du groupe).
    Promise.all(prix.split(",").map((p) => api.calculerTaxes(Number(p), Number(n), taxesHotel))).then(
      (d) => !annule && setDetailTaxes(cumulerTaxes(d)),
      () => !annule && setDetailTaxes(null),
    );
    return () => {
      annule = true;
    };
  }, [taxesHotel, cleTaxes]);

  const enregistrer = async () => {
    if (!premiere) return setErreur("Choisissez une chambre (étape 2).");
    if (!client) return setErreur("Choisissez le client (étape 3).");
    const illisible = chambres.find((_, i) => prixRetenus[i] === null);
    if (illisible) {
      return setErreur(
        groupe
          ? `Le prix de la chambre ${illisible.numero} n'est pas lisible. Écrivez seulement des chiffres, par exemple 40000.`
          : "Le prix n'est pas lisible. Écrivez seulement des chiffres, par exemple 40000.",
      );
    }
    const prixEnvoye = (i: number) => (prixDifferent ? (prixRetenus[i] ?? null) : null);
    const r = await lancer(
      groupe
        ? {
            groupe: {
              client_id: client.id,
              chambres: chambres.map((c, i) => ({ chambre_id: c.id, prix_total: prixEnvoye(i) })),
              date_arrivee: arrivee,
              date_depart: depart,
              nom: nomGroupe.trim() || null,
              notes: notes.trim() || null,
            },
          }
        : {
            seule: {
              chambre_id: premiere.id,
              client_id: client.id,
              date_arrivee: arrivee,
              date_depart: depart,
              prix_total: prixEnvoye(0),
              notes: notes.trim() || null,
            },
          },
    );
    if (r) onCree(r, chambres.length);
  };

  return (
    <>
      <Bouton variante="secondaire" icone={ArrowLeft} onClick={onRetour} className="mb-5">
        {libelleRetour}
      </Bouton>
      <EnTete titre="Nouvelle réservation" sousTitre="Remplissez les 4 étapes, puis enregistrez." />

      <div className="grid max-w-4xl gap-6">
        <Etape numero={1} titre="Les dates">
          <div className="flex flex-wrap items-start gap-x-12 gap-y-5">
            <ChampDate libelle="Jour d'arrivée" valeur={arrivee} onChange={setArrivee} />
            <Compteur libelle="Nombre de nuits" valeur={nuits} onChange={setNuits} unite="nuit" />
          </div>
          <p className="mt-4 rounded-xl bg-sable px-4 py-3 text-lg">
            Départ le <strong>{formatJourLong(depart)}</strong>
          </p>
        </Etape>

        <Etape numero={2} titre="La chambre">
          {erreurChambres && <Alerte type="erreur">{erreurChambres}</Alerte>}
          {choixPlusLibre && (
            <div className="mb-4">
              <Alerte type="attention">
                {chambreIds.length > 1 ? "Une chambre choisie n'est plus libre" : "La chambre choisie n'est pas libre"} à ces dates.
                Choisissez-en une autre, ou changez les dates.
              </Alerte>
            </div>
          )}
          {libres?.length === 0 && <AucuneChambreLibre />}
          {libres && libres.length > 0 && (
            <>
              <p className="mb-1 text-gris">{pluriel(libres.length, "chambre")} libre{libres.length > 1 ? "s" : ""} à ces dates.</p>
              <p className="mb-3 text-gris">
                Cochez la chambre. Pour une équipe ou une entreprise, cochez-en plusieurs : une seule facture les regroupera.
              </p>
              <div role="group" aria-label="Chambres" className="grid grid-cols-2 gap-3">
                {libres.map((c) => (
                  <label
                    key={c.id}
                    className="flex min-h-16 cursor-pointer items-center gap-3 rounded-xl border-2 border-trait px-4 py-2 has-[:checked]:border-marque has-[:checked]:bg-marque-clair hover:border-marque/40"
                  >
                    <input
                      type="checkbox"
                      className="size-5 accent-marque"
                      checked={chambreIds.includes(c.id)}
                      onChange={(e) => cocher(c.id, e.target.checked)}
                    />
                    <span>
                      <span className="block text-lg font-bold">Chambre {c.numero}</span>
                      <span className="block text-gris">
                        {c.type_nom} · {formatMontant(c.prix_nuit_defaut, devise)} par nuit
                      </span>
                    </span>
                  </label>
                ))}
              </div>
              {groupe && (
                <p className="mt-3 rounded-xl bg-marque-clair px-4 py-3 text-lg font-semibold text-marque-fonce" role="status">
                  {pluriel(chambres.length, "chambre")} pour le même client : {listeNumeros(chambres)}.
                </p>
              )}
            </>
          )}
        </Etape>

        <Etape numero={3} titre="Le client">
          <ChoixClient client={client} onChange={setClient} />
        </Etape>

        <Etape numero={4} titre="Le prix">
          {!premiere ? (
            <p className="text-gris">Le prix s'affichera quand la chambre sera choisie.</p>
          ) : !groupe ? (
            <p className="text-lg">
              {pluriel(nuits, "nuit")} × {formatMontant(premiere.prix_nuit_defaut, devise)} ={" "}
              <strong className="text-2xl">{formatMontant(prixNormal(premiere), devise)}</strong>
            </p>
          ) : (
            <div>
              <ul aria-label="Prix par chambre" className="grid gap-1 text-lg">
                {chambres.map((c) => (
                  <li key={c.id} className="flex flex-wrap justify-between gap-x-4">
                    <span>
                      Chambre {c.numero} : {pluriel(nuits, "nuit")} × {formatMontant(c.prix_nuit_defaut, devise)}
                    </span>
                    <span className="font-semibold">{formatMontant(prixNormal(c), devise)}</span>
                  </li>
                ))}
              </ul>
              {!prixDifferent && (
                <p className="mt-2 flex justify-between gap-4 border-t border-trait pt-2 text-lg">
                  <span>Total des {pluriel(chambres.length, "chambre")}</span>
                  <strong className="text-2xl">{formatMontant(totalPrix, devise)}</strong>
                </p>
              )}
            </div>
          )}
          <label className="mt-4 flex min-h-12 cursor-pointer items-center gap-3 font-semibold">
            <input type="checkbox" className="size-5 accent-marque" checked={prixDifferent} onChange={(e) => setPrixDifferent(e.target.checked)} />
            Appliquer un autre prix (prix convenu avec le client)
          </label>
          {prixDifferent &&
            (groupe ? (
              <div className="mt-2 grid max-w-xl gap-3">
                {chambres.map((c) => (
                  <Champ
                    key={c.id}
                    libelle={`Prix convenu pour la chambre ${c.numero}`}
                    value={prixSaisis[c.id] ?? ""}
                    onChange={(e) => setPrixSaisis((p) => ({ ...p, [c.id]: e.target.value }))}
                    inputMode="decimal"
                    aide={`Pour tout le séjour de cette chambre, en ${devise.code}.`}
                  />
                ))}
              </div>
            ) : (
              premiere && (
                <Champ
                  className="mt-2 max-w-sm"
                  libelle="Prix total convenu"
                  value={prixSaisis[premiere.id] ?? ""}
                  onChange={(e) => setPrixSaisis({ [premiere.id]: e.target.value })}
                  inputMode="decimal"
                  aide={`Pour tout le séjour, en ${devise.code}.`}
                />
              )
            ))}
          {detailTaxes && (
            <div className="mt-4 rounded-xl border-2 border-trait px-4 py-3">
              <p className="mb-2 font-bold">Taxes de l'hôtel</p>
              <ResumeTaxes detail={detailTaxes} devise={devise} />
              <p className="mt-3 flex justify-between gap-4 text-lg font-bold">
                <span>Total à payer</span>
                <span>{formatMontant(detailTaxes.total_a_payer, devise)}</span>
              </p>
              <p className="mt-1 text-base text-gris">Les taxes pourront être changées ensuite, sur la fiche de la réservation.</p>
            </div>
          )}
          {groupe && (
            <Champ
              className="mt-5 max-w-xl"
              libelle="Nom du groupe (facultatif)"
              value={nomGroupe}
              onChange={(e) => setNomGroupe(e.target.value)}
              maxLength={80}
              aide="Par exemple : Équipe SOGEA, Mariage Ngono. Il apparaît sur la facture."
            />
          )}
          <div className="mt-5">
            <label htmlFor="notes-reservation" className="mb-1.5 block font-semibold">
              Remarque <span className="font-normal text-gris">(facultatif)</span>
            </label>
            <textarea
              id="notes-reservation"
              rows={2}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className="w-full rounded-xl border-2 border-bordure-champ/60 bg-white px-4 py-2.5 text-lg focus:border-marque focus:outline-none focus:ring-4 focus:ring-marque/15"
            />
          </div>
        </Etape>

        {premiere && client && (
          <Alerte type="info" titre="Vérifiez avant d'enregistrer">
            {nomComplet(client)} — {groupe ? `chambres ${listeNumeros(chambres)}` : `chambre ${premiere.numero}`} —{" "}
            {formatSejour(arrivee, depart)} ({pluriel(nuits, "nuit")}).
          </Alerte>
        )}
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <div className="flex justify-end gap-3 pb-6">
          <Bouton variante="secondaire" onClick={onRetour} grand>
            Annuler
          </Bouton>
          <Bouton icone={Check} enCours={enCours} onClick={() => void enregistrer()} grand>
            {groupe ? `Enregistrer les ${chambres.length} chambres` : "Enregistrer la réservation"}
          </Bouton>
        </div>
      </div>
    </>
  );
}

/** Message affiché sur la fiche qui s'ouvre après l'enregistrement. */
export function messageCreation(r: Reservation, chambres: number): string {
  const acompte = "Si le client verse un acompte, encaissez-le ci-dessous.";
  return chambres > 1
    ? `La réservation de groupe de ${r.client_nom} est enregistrée (${pluriel(chambres, "chambre")}, ${pluriel(r.nuits, "nuit")}). ${acompte}`
    : `La réservation de ${r.client_nom} est enregistrée (chambre ${r.chambre_numero}, ${pluriel(r.nuits, "nuit")}). ${acompte}`;
}

/** La fiche qui s'ouvre après la création d'un groupe : celle de sa première chambre. */
function premiereChambre(g: Groupe): Reservation {
  const r = g.reservations[0];
  if (!r) throw new Error("groupe sans chambre");
  return r;
}

/** « 101, 102 et 201 » */
export function listeNumeros(chambres: { numero: string }[]): string {
  const n = chambres.map((c) => c.numero);
  return n.length < 2 ? (n[0] ?? "") : `${n.slice(0, -1).join(", ")} et ${n[n.length - 1]}`;
}

function Etape({ numero, titre, children }: { numero: number; titre: string; children: ReactNode }) {
  return (
    <Carte
      titre={
        <span className="flex items-center gap-3">
          <span className="flex size-9 items-center justify-center rounded-full bg-marque text-lg font-bold text-white" aria-hidden>
            {numero}
          </span>
          <span>
            <span className="sr-only">Étape {numero} : </span>
            {titre}
          </span>
        </span>
      }
    >
      {children}
    </Carte>
  );
}

/**
 * Quand aucune chambre n'est proposée, on dit pourquoi et quoi faire :
 * un hôtel qui n'a encore aucune chambre n'a pas le même problème qu'un hôtel complet.
 */
function AucuneChambreLibre() {
  const [chambres, setChambres] = useState<Chambre[] | null>(null);
  useEffect(() => {
    let annule = false;
    api.listerChambres().then(
      (c) => !annule && setChambres(c),
      () => !annule && setChambres(null), // sans le droit de voir les chambres : message général
    );
    return () => {
      annule = true;
    };
  }, []);

  if (chambres?.length === 0) {
    return (
      <Alerte type="attention" titre="L'hôtel n'a encore aucune chambre.">
        Ajoutez d'abord les chambres dans « Chambres », dans le menu à gauche. Elles apparaîtront ensuite ici.
      </Alerte>
    );
  }
  if (chambres && chambres.every((c) => c.statut !== "disponible")) {
    return (
      <Alerte type="attention" titre="Aucune chambre n'est en service.">
        Toutes les chambres sont marquées « En maintenance » ou « Hors service ». Changez leur état dans « Chambres ».
      </Alerte>
    );
  }
  return (
    <Alerte type="attention" titre="Aucune chambre n'est libre à ces dates.">
      Toutes les chambres sont déjà réservées. Essayez d'autres dates.
    </Alerte>
  );
}
