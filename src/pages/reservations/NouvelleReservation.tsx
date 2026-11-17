import { useEffect, useState, type ReactNode } from "react";
import { ArrowLeft, Check } from "lucide-react";

import type { Chambre } from "../../bindings/Chambre";
import type { Client } from "../../bindings/Client";
import type { DetailTaxes } from "../../bindings/DetailTaxes";
import type { Reservation } from "../../bindings/Reservation";
import type { Taxe } from "../../bindings/Taxe";
import { ChampDate, Compteur } from "../../components/saisie";
import { ResumeTaxes } from "../../components/taxes";
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
  onCree: (r: Reservation) => void;
}) {
  const devise = useDevise();
  const [arrivee, setArrivee] = useState(arriveeInitiale ?? aujourdhui());
  const [nuits, setNuits] = useState(1);
  const depart = ajouterJours(arrivee, nuits);

  const [libres, setLibres] = useState<Chambre[] | null>(null);
  const [erreurChambres, setErreurChambres] = useState<string | null>(null);
  const [chambreId, setChambreId] = useState<string | null>(chambreInitiale ?? null);
  const [client, setClient] = useState<Client | null>(clientInitial ?? null);
  const [prixDifferent, setPrixDifferent] = useState(false);
  const [prixSaisi, setPrixSaisi] = useState("");
  const [notes, setNotes] = useState("");
  const { lancer, enCours, erreur, setErreur } = useAction(api.creerReservation);

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
  useEffect(() => setErreur(null), [setErreur, arrivee, nuits, chambreId, client, prixDifferent, prixSaisi]);

  const chambre = libres?.find((c) => c.id === chambreId) ?? null;
  const choixPlusLibre = libres !== null && chambreId !== null && chambre === null;
  const prixNormal = chambre ? chambre.prix_nuit_defaut * nuits : null;
  const prixRetenu = prixDifferent ? lireMontant(prixSaisi, devise.decimales) : prixNormal;

  // Taxes habituelles de l'hôtel : elles s'appliqueront à cette réservation.
  const [taxesHotel, setTaxesHotel] = useState<Taxe[]>([]);
  useEffect(() => {
    api.taxesHotel().then(setTaxesHotel, () => setTaxesHotel([]));
  }, []);
  const [detailTaxes, setDetailTaxes] = useState<DetailTaxes | null>(null);
  useEffect(() => {
    if (taxesHotel.length === 0 || prixRetenu === null) return setDetailTaxes(null);
    let annule = false;
    api.calculerTaxes(prixRetenu, nuits, taxesHotel).then(
      (d) => !annule && setDetailTaxes(d),
      () => !annule && setDetailTaxes(null),
    );
    return () => {
      annule = true;
    };
  }, [taxesHotel, prixRetenu, nuits]);

  const enregistrer = async () => {
    if (!chambre) return setErreur("Choisissez une chambre (étape 2).");
    if (!client) return setErreur("Choisissez le client (étape 3).");
    let prix: number | null = null;
    if (prixDifferent) {
      prix = lireMontant(prixSaisi, devise.decimales);
      if (prix === null) return setErreur("Le prix n'est pas lisible. Écrivez seulement des chiffres, par exemple 40000.");
    }
    const r = await lancer({
      chambre_id: chambre.id,
      client_id: client.id,
      date_arrivee: arrivee,
      date_depart: depart,
      prix_total: prix,
      notes: notes.trim() || null,
    });
    if (r) onCree(r);
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
              <Alerte type="attention">La chambre choisie n'est pas libre à ces dates. Choisissez-en une autre, ou changez les dates.</Alerte>
            </div>
          )}
          {libres?.length === 0 && <AucuneChambreLibre />}
          {libres && libres.length > 0 && (
            <>
              <p className="mb-3 text-gris">{pluriel(libres.length, "chambre")} libre{libres.length > 1 ? "s" : ""} à ces dates :</p>
              <div role="radiogroup" aria-label="Chambre" className="grid grid-cols-2 gap-3">
                {libres.map((c) => (
                  <label
                    key={c.id}
                    className="flex min-h-16 cursor-pointer items-center gap-3 rounded-xl border-2 border-trait px-4 py-2 has-[:checked]:border-marque has-[:checked]:bg-marque-clair hover:border-marque/40"
                  >
                    <input type="radio" name="chambre" className="size-5 accent-marque" checked={chambreId === c.id} onChange={() => setChambreId(c.id)} />
                    <span>
                      <span className="block text-lg font-bold">Chambre {c.numero}</span>
                      <span className="block text-gris">
                        {c.type_nom} · {formatMontant(c.prix_nuit_defaut, devise)} par nuit
                      </span>
                    </span>
                  </label>
                ))}
              </div>
            </>
          )}
        </Etape>

        <Etape numero={3} titre="Le client">
          <ChoixClient client={client} onChange={setClient} />
        </Etape>

        <Etape numero={4} titre="Le prix">
          {prixNormal === null ? (
            <p className="text-gris">Le prix s'affichera quand la chambre sera choisie.</p>
          ) : (
            <p className="text-lg">
              {pluriel(nuits, "nuit")} × {formatMontant(chambre!.prix_nuit_defaut, devise)} ={" "}
              <strong className="text-2xl">{formatMontant(prixNormal, devise)}</strong>
            </p>
          )}
          <label className="mt-4 flex min-h-12 cursor-pointer items-center gap-3 font-semibold">
            <input type="checkbox" className="size-5 accent-marque" checked={prixDifferent} onChange={(e) => setPrixDifferent(e.target.checked)} />
            Appliquer un autre prix (prix convenu avec le client)
          </label>
          {prixDifferent && (
            <Champ
              className="mt-2 max-w-sm"
              libelle="Prix total convenu"
              value={prixSaisi}
              onChange={(e) => setPrixSaisi(e.target.value)}
              inputMode="decimal"
              aide={`Pour tout le séjour, en ${devise.code}.`}
            />
          )}
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

        {chambre && client && (
          <Alerte type="info" titre="Vérifiez avant d'enregistrer">
            {nomComplet(client)} — chambre {chambre.numero} — {formatSejour(arrivee, depart)} ({pluriel(nuits, "nuit")}).
          </Alerte>
        )}
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <div className="flex justify-end gap-3 pb-6">
          <Bouton variante="secondaire" onClick={onRetour} grand>
            Annuler
          </Bouton>
          <Bouton icone={Check} enCours={enCours} onClick={() => void enregistrer()} grand>
            Enregistrer la réservation
          </Bouton>
        </div>
      </div>
    </>
  );
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
