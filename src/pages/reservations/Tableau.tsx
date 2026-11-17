import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, CalendarPlus } from "lucide-react";

import type { Chambre } from "../../bindings/Chambre";
import type { Reservation } from "../../bindings/Reservation";
import type { StatutReservation } from "../../bindings/StatutReservation";
import { Alerte, Bouton, Carte, EnTete } from "../../components/ui";
import { api } from "../../lib/api";
import { ajouterJours, enTeteJour, formatJourLong, formatJourMoyen, formatSejour, joursEntre, plage } from "../../lib/dates";
import { versErreur } from "../../lib/erreurs";
import { aujourdhui, formatMontant, type Devise } from "../../lib/format";
import { LIBELLE_STATUT_RESERVATION } from "../../lib/libelles";
import { useDevise, useSession } from "../../lib/session";

/** Couleur ET texte : on ne compte jamais sur la couleur seule. */
const STYLE_STATUT: Record<Exclude<StatutReservation, "annulee">, string> = {
  confirmee: "border-marque bg-marque-clair text-marque-fonce hover:bg-marque/20",
  arrivee: "border-orange bg-orange-clair text-orange-texte hover:bg-orange/25",
  partie: "border-bordure-champ/50 bg-sable text-gris hover:bg-trait/60",
};

function resumePaiement(r: Reservation, devise: Devise): string {
  if (r.solde > 0) return `Reste ${formatMontant(r.solde, devise)}`;
  if (r.solde < 0) return "Trop payé";
  return "Payé ✓";
}

export function Tableau({
  debut,
  onDebut,
  onNouvelle,
  onOuvrir,
}: {
  debut: string;
  onDebut: (iso: string) => void;
  onNouvelle: (chambreId?: string, arrivee?: string) => void;
  onOuvrir: (reservationId: string) => void;
}) {
  const { peut } = useSession();
  const devise = useDevise();
  const [nbJours, setNbJours] = useState<7 | 14>(7);
  const [chambres, setChambres] = useState<Chambre[] | null>(null);
  const [reservations, setReservations] = useState<Reservation[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);

  const jours = plage(debut, nbJours);
  const fin = ajouterJours(debut, nbJours);
  const ceJour = aujourdhui();
  const peutReserver = peut("reservation.creer");

  useEffect(() => {
    let annule = false;
    // On lit depuis la veille pour connaître aussi les départs du premier jour affiché.
    Promise.all([api.listerChambres(), api.planning(ajouterJours(debut, -1), fin)]).then(
      ([c, r]) => {
        if (annule) return;
        setChambres(c);
        setReservations(r);
        setErreur(null);
      },
      (e) => !annule && setErreur(versErreur(e).message),
    );
    return () => {
      annule = true;
    };
  }, [debut, fin]);

  const colonnes = { gridTemplateColumns: `9.5rem repeat(${nbJours}, minmax(0, 1fr))` };
  const aujourdhuiVisible = ceJour >= debut && ceJour < fin;
  const arrivees = reservations.filter((r) => r.date_arrivee === ceJour && r.statut === "confirmee");
  const departs = reservations.filter((r) => r.date_depart === ceJour && r.statut === "arrivee");

  return (
    <>
      <EnTete
        titre="Réservations"
        sousTitre={
          peutReserver
            ? "Cliquez sur une case vide pour réserver, ou sur un nom pour ouvrir la réservation."
            : "Cliquez sur un nom pour ouvrir la réservation."
        }
        actions={
          peutReserver && (
            <Bouton icone={CalendarPlus} onClick={() => onNouvelle()} grand>
              Nouvelle réservation
            </Bouton>
          )
        }
      />
      {erreur && (
        <div className="mb-5">
          <Alerte type="erreur">{erreur}</Alerte>
        </div>
      )}

      {aujourdhuiVisible && (arrivees.length > 0 || departs.length > 0) && (
        <div className="mb-6 grid grid-cols-2 gap-5">
          <AFaire titre="Arrivées prévues aujourd'hui" vide="Aucune arrivée prévue." reservations={arrivees} onOuvrir={onOuvrir} />
          <AFaire titre="Départs prévus aujourd'hui" vide="Aucun départ prévu." reservations={departs} onOuvrir={onOuvrir} />
        </div>
      )}

      <Carte>
        <div className="mb-5">
          <div className="flex flex-wrap gap-2">
            <Bouton variante="secondaire" icone={ArrowLeft} onClick={() => onDebut(ajouterJours(debut, -nbJours))}>
              Jours précédents
            </Bouton>
            <Bouton variante="secondaire" onClick={() => onDebut(ceJour)} disabled={debut === ceJour}>
              Aujourd'hui
            </Bouton>
            <Bouton variante="secondaire" onClick={() => onDebut(ajouterJours(debut, nbJours))}>
              Jours suivants
              <ArrowRight className="size-5" aria-hidden strokeWidth={2.25} />
            </Bouton>
          </div>
        </div>
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-xl font-bold">
            Du {formatJourMoyen(debut)} au {formatJourLong(ajouterJours(fin, -1))}
          </h2>
          <div role="group" aria-label="Nombre de jours affichés" className="grid grid-cols-2 gap-1 rounded-xl bg-sable p-1.5">
            {([7, 14] as const).map((n) => (
              <button
                key={n}
                type="button"
                aria-pressed={nbJours === n}
                onClick={() => setNbJours(n)}
                className={`min-h-10 rounded-lg px-4 text-base font-semibold ${nbJours === n ? "bg-white text-marque-fonce shadow-sm" : "text-gris hover:text-encre"}`}
              >
                {n === 7 ? "1 semaine" : "2 semaines"}
              </button>
            ))}
          </div>
        </div>

        {chambres?.length === 0 ? (
          <p className="py-6 text-lg text-gris">
            Aucune chambre pour le moment. Ajoutez d'abord vos chambres dans l'écran « Chambres ».
          </p>
        ) : (
          <div role="table" aria-label="Tableau des réservations" className="overflow-hidden rounded-xl border-2 border-trait">
            <div role="row" className="grid bg-sable" style={colonnes}>
              <div role="columnheader" className="flex items-center px-3 font-bold">
                Chambre
              </div>
              {jours.map((j) => {
                const e = enTeteJour(j);
                return (
                  <div
                    key={j}
                    role="columnheader"
                    aria-label={formatJourMoyen(j) + (j === ceJour ? " (aujourd'hui)" : "")}
                    className={`border-l border-trait py-2 text-center leading-tight ${j === ceJour ? "bg-or font-bold text-encre" : ""}`}
                  >
                    <span className="block text-sm">{e.semaine}</span>
                    <span className="block text-xl font-bold">{e.jour}</span>
                    {nbJours === 7 && <span className="block text-sm">{e.mois}</span>}
                  </div>
                );
              })}
            </div>
            {(chambres ?? []).map((c) => (
              <LigneChambre
                key={c.id}
                chambre={c}
                jours={jours}
                colonnes={colonnes}
                ceJour={ceJour}
                reservations={reservations.filter((r) => r.chambre_id === c.id)}
                devise={devise}
                compact={nbJours === 14}
                onNouvelle={peutReserver ? onNouvelle : undefined}
                onOuvrir={onOuvrir}
              />
            ))}
          </div>
        )}

        <ul className="mt-5 flex flex-wrap gap-x-6 gap-y-2 text-base" aria-label="Légende">
          <li className="font-semibold">Une case = une nuit.</li>
          {(["confirmee", "arrivee", "partie"] as const).map((s) => (
            <li key={s} className="flex items-center gap-2">
              <span className={`inline-block size-5 rounded border-2 ${STYLE_STATUT[s].split(" hover:")[0]}`} aria-hidden />
              {LIBELLE_STATUT_RESERVATION[s]}
            </li>
          ))}
          <li className="flex items-center gap-2">
            <span className="inline-block size-5 rounded border-2 border-or bg-or" aria-hidden />
            Aujourd'hui
          </li>
        </ul>
      </Carte>
    </>
  );
}

function AFaire({
  titre,
  vide,
  reservations,
  onOuvrir,
}: {
  titre: string;
  vide: string;
  reservations: Reservation[];
  onOuvrir: (id: string) => void;
}) {
  return (
    <Carte titre={titre}>
      {reservations.length === 0 ? (
        <p className="text-gris">{vide}</p>
      ) : (
        <ul className="grid gap-2">
          {reservations.map((r) => (
            <li key={r.id}>
              <button
                type="button"
                onClick={() => onOuvrir(r.id)}
                className="flex min-h-12 w-full items-center justify-between gap-3 rounded-xl border-2 border-trait px-4 text-left hover:border-marque/40 hover:bg-marque-clair/50"
              >
                <span className="font-semibold">{r.client_nom}</span>
                <span className="text-gris">Chambre {r.chambre_numero}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Carte>
  );
}

const LIBELLE_INDISPONIBLE = { maintenance: "En maintenance", hors_service: "Hors service" } as const;

function LigneChambre({
  chambre,
  jours,
  colonnes,
  ceJour,
  reservations,
  devise,
  compact,
  onNouvelle,
  onOuvrir,
}: {
  chambre: Chambre;
  jours: string[];
  colonnes: React.CSSProperties;
  ceJour: string;
  reservations: Reservation[];
  devise: Devise;
  compact: boolean;
  onNouvelle?: (chambreId: string, arrivee: string) => void;
  onOuvrir: (id: string) => void;
}) {
  const debut = jours[0]!;
  const n = jours.length;
  const louable = chambre.statut === "disponible";
  const occupee = (jour: string) => reservations.some((r) => r.date_arrivee <= jour && jour < r.date_depart);

  return (
    <div role="row" className="grid min-h-18 border-t-2 border-trait" style={colonnes}>
      <div role="rowheader" className="flex flex-col justify-center px-3 py-1" style={{ gridRow: 1, gridColumn: 1 }}>
        <span className="text-xl leading-tight font-bold">{chambre.numero}</span>
        <span className="truncate text-sm text-gris">
          {chambre.statut === "disponible" ? chambre.type_nom : LIBELLE_INDISPONIBLE[chambre.statut]}
        </span>
      </div>

      {jours.map((jour, i) => {
        const fond = jour === ceJour ? "bg-or-clair" : !louable ? "bg-sable" : "";
        const place = { gridRow: 1, gridColumn: i + 2 };
        if (occupee(jour) || !louable || !onNouvelle) {
          return <div key={jour} role="cell" className={`border-l border-trait ${fond}`} style={place} />;
        }
        return (
          <button
            key={jour}
            type="button"
            style={place}
            onClick={() => onNouvelle(chambre.id, jour)}
            aria-label={`Réserver la chambre ${chambre.numero} pour la nuit du ${formatJourMoyen(jour)}`}
            className={`group border-l border-trait text-2xl font-bold text-transparent transition-colors hover:bg-marque-clair hover:text-marque focus-visible:text-marque ${fond}`}
          >
            <span aria-hidden>+</span>
          </button>
        );
      })}

      {reservations.map((r) => {
        if (r.statut === "annulee") return null;
        const depuis = joursEntre(debut, r.date_arrivee);
        const jusqua = joursEntre(debut, r.date_depart);
        const col1 = Math.max(0, depuis);
        const col2 = Math.min(n, jusqua);
        if (col2 <= col1) return null;
        return (
          <button
            key={r.id}
            type="button"
            onClick={() => onOuvrir(r.id)}
            style={{ gridRow: 1, gridColumn: `${col1 + 2} / ${col2 + 2}` }}
            title={`${r.client_nom} — ${formatSejour(r.date_arrivee, r.date_depart)}`}
            aria-label={`${r.client_nom}, chambre ${r.chambre_numero}, ${formatSejour(r.date_arrivee, r.date_depart)}. ${LIBELLE_STATUT_RESERVATION[r.statut]}. ${resumePaiement(r, devise)}`}
            className={`z-10 m-1 flex min-w-0 flex-col justify-center overflow-hidden border-2 px-2 text-left leading-tight transition-colors ${STYLE_STATUT[r.statut]} ${depuis < 0 ? "rounded-l-none border-l-0" : "rounded-l-lg"} ${jusqua > n ? "rounded-r-none border-r-0" : "rounded-r-lg"}`}
          >
            <span className="truncate font-bold">{r.client_nom}</span>
            {!(compact && col2 - col1 < 2) && <span className="truncate text-sm">{resumePaiement(r, devise)}</span>}
          </button>
        );
      })}
    </div>
  );
}
