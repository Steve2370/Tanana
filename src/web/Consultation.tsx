/**
 * Page web de consultation à distance (lecture seule).
 * Le propriétaire tape l'identifiant de l'hôtel et la clé secrète ; tout est
 * déchiffré dans ce navigateur et gardé en mémoire seulement : fermer
 * l'onglet efface tout.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react";
import { BedDouble, CalendarDays, LogIn, LogOut, RefreshCw, ScrollText, ShieldCheck, Wallet, type LucideIcon } from "lucide-react";

import { LogoComplet, LogoSymbole } from "../components/Logo";
import { Alerte, Bouton, Carte, Champ, Pastille } from "../components/ui";
import { ajouterJours, debutMois, debutMoisSuivant, formatSejour, pluriel } from "../lib/dates";
import { aujourdhui, decalageLocal, formatHorodatage, formatJour, formatMontant, type Devise } from "../lib/format";
import { LIBELLE_MODE, LIBELLE_STATUT_RESERVATION, LIBELLE_TYPE_PAIEMENT } from "../lib/libelles";
import { libelleAction } from "../lib/permissions";
import type { ModePaiement } from "../bindings/ModePaiement";
import type { TypePaiement } from "../bindings/TypePaiement";
import { CleIncorrecte, lireCle, normaliserIdentifiant, preparerCles, type Cles } from "./crypto";
import { Copie, ErreurConsultation, mettreAJour, verifierCle } from "./relais";
import { caisse, hotel, journal, journee, planning, reservations, type EtatChambre, type ReservationVue } from "./vues";

/** Au-delà, on prévient que l'hôtel ne donne plus de nouvelles. */
const SILENCE_MINUTES = 15;
const ACTUALISATION_MS = 60_000;

interface Acces {
  hotelId: string;
  cles: Cles;
  copie: Copie;
}

function message(e: unknown): string {
  if (e instanceof CleIncorrecte || e instanceof ErreurConsultation) return e.message;
  if (e instanceof Error && /mal recopiée/.test(e.message)) return e.message;
  return "Les données reçues sont illisibles. Réessayez ; si cela continue, réactivez la consultation à l'hôtel.";
}

export function PageConsultation({ lecteur = fetch.bind(globalThis) }: { lecteur?: typeof fetch }) {
  const [acces, setAcces] = useState<Acces | null>(null);
  const [version, setVersion] = useState(0);
  const [erreur, setErreur] = useState<string | null>(null);
  const [actualisation, setActualisation] = useState(false);

  const actualiser = useCallback(async () => {
    if (!acces) return;
    setActualisation(true);
    try {
      await mettreAJour(lecteur, acces.hotelId, acces.cles, acces.copie);
      setErreur(null);
      setVersion((v) => v + 1);
    } catch (e) {
      setErreur(message(e));
    } finally {
      setActualisation(false);
    }
  }, [acces, lecteur]);

  useEffect(() => {
    if (!acces) return;
    const minuterie = setInterval(() => {
      if (document.visibilityState === "visible") void actualiser();
    }, ACTUALISATION_MS);
    return () => clearInterval(minuterie);
  }, [acces, actualiser]);

  if (!acces) return <Saisie lecteur={lecteur} onOuvert={(a) => setAcces(a)} />;
  return (
    <VueHotel
      copie={acces.copie}
      version={version}
      erreur={erreur}
      actualisation={actualisation}
      onActualiser={() => void actualiser()}
      onFermer={() => {
        setAcces(null);
        setErreur(null);
      }}
    />
  );
}

// ─────────────────────────────── Saisie ───────────────────────────────

function Saisie({ lecteur, onOuvert }: { lecteur: typeof fetch; onOuvert: (a: Acces) => void }) {
  const [identifiant, setIdentifiant] = useState("");
  const [cle, setCle] = useState("");
  const [enCours, setEnCours] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  const ouvrir = async (e: FormEvent) => {
    e.preventDefault();
    if (enCours) return;
    const hotelId = normaliserIdentifiant(identifiant);
    if (hotelId.length < 4) {
      setErreur("Tapez l'identifiant de l'hôtel (2 groupes de 4 caractères, par exemple HK7Q-F29X).");
      return;
    }
    setErreur(null);
    setEnCours(true);
    try {
      const cles = await preparerCles(lireCle(cle));
      await verifierCle(lecteur, hotelId, cles);
      const copie = new Copie();
      await mettreAJour(lecteur, hotelId, cles, copie);
      onOuvert({ hotelId, cles, copie });
    } catch (err) {
      setErreur(message(err));
      setEnCours(false);
    }
  };

  return (
    <main className="mx-auto flex min-h-screen max-w-xl flex-col justify-center px-5 py-10">
      <LogoComplet hauteur={110} />
      <h1 className="mt-8 text-center text-3xl font-bold text-marque-fonce">Consulter mon hôtel</h1>
      <p className="mt-3 text-center text-lg text-gris">
        Tapez les deux informations affichées à l'hôtel, dans Tanana : <em>Réglages › Consultation à distance</em>.
      </p>
      <form onSubmit={ouvrir} className="mt-8 grid gap-5 rounded-2xl border border-trait bg-white p-6 shadow-sm" noValidate>
        <Champ
          libelle="Identifiant de l'hôtel"
          placeholder="HK7Q-F29X"
          value={identifiant}
          onChange={(e) => setIdentifiant(e.target.value)}
          autoComplete="off"
          autoCapitalize="characters"
          spellCheck={false}
          required
        />
        <Champ
          libelle="Clé secrète"
          aide="6 groupes de 4 caractères, puis 2. Les tirets sont facultatifs."
          placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XX"
          value={cle}
          onChange={(e) => setCle(e.target.value)}
          autoComplete="off"
          autoCapitalize="characters"
          spellCheck={false}
          required
        />
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <Bouton type="submit" grand icone={LogIn} enCours={enCours}>
          {enCours ? "Ouverture…" : "Voir mon hôtel"}
        </Bouton>
      </form>
      <p className="mt-5 flex items-start gap-2 text-base text-gris">
        <ShieldCheck className="mt-0.5 size-5 shrink-0 text-marque" aria-hidden />
        La clé secrète ne quitte pas cet appareil : les données sont déchiffrées ici, le serveur ne peut pas les lire.
        Rien n'est enregistré ; fermez la page pour tout effacer.
      </p>
    </main>
  );
}

// ─────────────────────────────── L'hôtel ───────────────────────────────

const ONGLETS: { id: Onglet; libelle: string; icone: LucideIcon }[] = [
  { id: "jour", libelle: "Aujourd'hui", icone: BedDouble },
  { id: "reservations", libelle: "Réservations", icone: CalendarDays },
  { id: "caisse", libelle: "Caisse", icone: Wallet },
  { id: "journal", libelle: "Historique", icone: ScrollText },
];
type Onglet = "jour" | "reservations" | "caisse" | "journal";

function VueHotel({
  copie,
  version,
  erreur,
  actualisation,
  onActualiser,
  onFermer,
}: {
  copie: Copie;
  version: number;
  erreur: string | null;
  actualisation: boolean;
  onActualiser: () => void;
  onFermer: () => void;
}) {
  const [onglet, setOnglet] = useState<Onglet>("jour");
  const jour = aujourdhui();
  // `version` change à chaque actualisation : on recalcule tout.
  const { infos, toutes } = useMemo(() => ({ infos: hotel(copie), toutes: reservations(copie) }), [copie, version]);
  const contact = copie.dernierContact ?? copie.donneesDu;
  const silence = contact ? (Date.now() - Date.parse(contact)) / 60_000 > SILENCE_MINUTES : false;
  const titre = useRef<HTMLHeadingElement>(null);
  useEffect(() => titre.current?.focus(), []);

  return (
    <div className="min-h-screen">
      <header className="border-b border-trait bg-white">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-4 px-5 py-4">
          <div className="flex min-w-0 items-center gap-3">
            <LogoSymbole taille={40} />
            <div className="min-w-0">
              <h1 ref={titre} tabIndex={-1} className="truncate text-2xl font-bold text-marque-fonce outline-none">
                {infos.nom}
              </h1>
              <p className="text-base text-gris">
                Consultation à distance (lecture seule)
                {copie.donneesDu && <> · données de l'hôtel du {formatHorodatage(copie.donneesDu)}</>}
              </p>
            </div>
          </div>
          <div className="flex gap-3">
            <Bouton variante="secondaire" icone={RefreshCw} enCours={actualisation} onClick={onActualiser}>
              Actualiser
            </Bouton>
            <Bouton variante="discret" icone={LogOut} onClick={onFermer}>
              Fermer
            </Bouton>
          </div>
        </div>
        <nav aria-label="Rubriques" className="mx-auto flex max-w-6xl gap-1 overflow-x-auto px-5" role="tablist">
          {ONGLETS.map(({ id, libelle, icone: Icone }) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={onglet === id}
              onClick={() => setOnglet(id)}
              className={`flex min-h-12 items-center gap-2 whitespace-nowrap border-b-4 px-4 text-lg font-semibold ${
                onglet === id ? "border-marque text-marque-fonce" : "border-transparent text-gris hover:text-encre"
              }`}
            >
              <Icone className="size-5" aria-hidden />
              {libelle}
            </button>
          ))}
        </nav>
      </header>
      <main className="mx-auto grid max-w-6xl gap-6 px-5 py-7">
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        {silence && contact && (
          <Alerte type="attention" titre="L'hôtel ne donne plus de nouvelles">
            Dernier contact {formatHorodatage(contact)}. Internet est peut-être coupé à l'hôtel, ou l'ordinateur éteint.
            Les informations ci-dessous datent de ce moment-là.
          </Alerte>
        )}
        {onglet === "jour" && <VueJour copie={copie} toutes={toutes} jour={jour} devise={infos.devise} />}
        {onglet === "reservations" && <VueReservations toutes={toutes} jour={jour} devise={infos.devise} />}
        {onglet === "caisse" && <VueCaisse copie={copie} toutes={toutes} jour={jour} devise={infos.devise} />}
        {onglet === "journal" && <VueJournal copie={copie} version={version} />}
      </main>
    </div>
  );
}

function Indicateur({ titre, children, detail }: { titre: string; children: ReactNode; detail?: ReactNode }) {
  return (
    <div className="rounded-2xl border border-trait bg-white p-5 shadow-sm">
      <p className="text-base font-semibold text-gris">{titre}</p>
      <p className="mt-1 text-3xl font-bold text-marque-fonce">{children}</p>
      {detail && <p className="mt-1 text-base text-gris">{detail}</p>}
    </div>
  );
}

const ETATS: Record<EtatChambre, { libelle: string; style: string }> = {
  occupee: { libelle: "Occupée", style: "border-marque bg-marque-clair text-marque-fonce" },
  depart: { libelle: "Départ aujourd'hui", style: "border-orange bg-orange-clair text-orange-texte" },
  attendue: { libelle: "Arrivée attendue", style: "border-or bg-or-clair text-alerte" },
  libre: { libelle: "Libre", style: "border-trait bg-white text-gris" },
  maintenance: { libelle: "En maintenance", style: "border-trait bg-sable text-gris" },
  hors_service: { libelle: "Hors service", style: "border-trait bg-sable text-gris" },
};

function VueJour({ copie, toutes, jour, devise }: { copie: Copie; toutes: ReservationVue[]; jour: string; devise: Devise }) {
  const j = journee(copie, toutes, jour);
  const c = caisse(copie, toutes, jour, ajouterJours(jour, 1), decalageLocal());
  const louables = j.chambres.filter((x) => x.etat !== "maintenance" && x.etat !== "hors_service").length;
  const occupees = j.chambres.filter((x) => x.etat === "occupee" || x.etat === "depart").length;
  return (
    <>
      <p className="text-xl font-semibold capitalize">
        {new Intl.DateTimeFormat("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" }).format(new Date())}
      </p>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Indicateur titre="Chambres occupées" detail={`sur ${pluriel(louables, "chambre")} disponible${louables > 1 ? "s" : ""}`}>
          {occupees}
        </Indicateur>
        <Indicateur titre="Arrivées aujourd'hui">{j.arrivees.length}</Indicateur>
        <Indicateur titre="Départs aujourd'hui">{j.departs.length}</Indicateur>
        <Indicateur titre="Encaissé aujourd'hui" detail={c.depenses ? `Dépenses : ${formatMontant(c.depenses, devise)}` : undefined}>
          {formatMontant(c.encaissements, devise)}
        </Indicateur>
      </div>
      <Carte titre="Les chambres">
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-3">
          {j.chambres.map((ch) => (
            <li key={ch.id} className={`rounded-xl border-2 p-3 ${ETATS[ch.etat].style}`}>
              <p className="text-xl font-bold">{ch.numero}</p>
              <p className="text-sm">{ch.type}</p>
              <p className="mt-1 font-semibold">{ETATS[ch.etat].libelle}</p>
              {ch.reservation && <p className="truncate text-sm">{ch.reservation.client_nom}</p>}
            </li>
          ))}
          {j.chambres.length === 0 && <li className="text-gris">Aucune chambre enregistrée.</li>}
        </ul>
      </Carte>
      <div className="grid gap-6 lg:grid-cols-2">
        <ListeSejours titre="Arrivées du jour" vide="Aucune arrivée prévue aujourd'hui." liste={j.arrivees} devise={devise} />
        <ListeSejours titre="Départs du jour" vide="Aucun départ aujourd'hui." liste={j.departs} devise={devise} />
      </div>
      {j.resteAEncaisser > 0 && (
        <Alerte type="info" titre="Reste à encaisser auprès des clients présents">
          {formatMontant(j.resteAEncaisser, devise)}
        </Alerte>
      )}
    </>
  );
}

function ListeSejours({ titre, vide, liste, devise }: { titre: string; vide: string; liste: ReservationVue[]; devise: Devise }) {
  return (
    <Carte titre={titre}>
      {liste.length === 0 ? (
        <p className="text-gris">{vide}</p>
      ) : (
        <ul className="divide-y divide-trait">
          {liste.map((r) => (
            <li key={r.id} className="flex flex-wrap items-center justify-between gap-2 py-3">
              <span>
                <span className="font-bold">Ch. {r.chambre_numero}</span> · {r.client_nom}
              </span>
              <ResteDu r={r} devise={devise} />
            </li>
          ))}
        </ul>
      )}
    </Carte>
  );
}

function ResteDu({ r, devise }: { r: ReservationVue; devise: Devise }) {
  // Chambre d'un groupe : un seul compte pour tout le groupe.
  const solde = r.groupe ? r.groupe.solde : r.solde;
  const qui = r.groupe ? `Groupe${r.groupe.nom ? ` « ${r.groupe.nom} »` : ""} (${r.groupe.chambres} ch.) : ` : "";
  if (solde > 0) return <Pastille ton="alerte">{qui}Reste {formatMontant(solde, devise)}</Pastille>;
  if (solde < 0) return <Pastille ton="danger">{qui}Trop perçu {formatMontant(-solde, devise)}</Pastille>;
  return <Pastille ton="succes">{qui}Payé</Pastille>;
}

const TON_STATUT = { confirmee: "neutre", arrivee: "succes", partie: "neutre", annulee: "danger" } as const;

function VueReservations({ toutes, jour, devise }: { toutes: ReservationVue[]; jour: string; devise: Devise }) {
  const [periode, setPeriode] = useState<"avenir" | "mois" | "passees" | "annulees">("avenir");
  const [recherche, setRecherche] = useState("");
  const choix = {
    avenir: { libelle: "En cours et à venir", liste: planning(toutes, jour, ajouterJours(jour, 366)) },
    mois: { libelle: "Ce mois-ci", liste: planning(toutes, debutMois(jour), debutMoisSuivant(jour)) },
    passees: { libelle: "Terminées", liste: toutes.filter((r) => r.statut === "partie" || (r.statut !== "annulee" && r.date_depart < jour)) },
    annulees: { libelle: "Annulées", liste: toutes.filter((r) => r.statut === "annulee") },
  } as const;
  const filtre = recherche.trim().toLowerCase();
  const liste = choix[periode].liste
    .filter((r) => !filtre || r.client_nom.toLowerCase().includes(filtre) || r.chambre_numero.toLowerCase().includes(filtre))
    .sort((a, b) => a.date_arrivee.localeCompare(b.date_arrivee));
  return (
    <Carte titre="Réservations">
      <div className="mb-5 flex flex-wrap items-end gap-3">
        <div className="flex flex-wrap gap-2" role="group" aria-label="Période">
          {(Object.keys(choix) as (keyof typeof choix)[]).map((p) => (
            <Bouton key={p} variante={periode === p ? "primaire" : "secondaire"} aria-pressed={periode === p} onClick={() => setPeriode(p)}>
              {choix[p].libelle}
            </Bouton>
          ))}
        </div>
        <Champ
          libelle="Chercher un client ou une chambre"
          className="min-w-64 flex-1"
          value={recherche}
          onChange={(e) => setRecherche(e.target.value)}
        />
      </div>
      {liste.length === 0 ? (
        <p className="text-gris">Aucune réservation ici.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left">
            <thead className="border-b-2 border-trait text-base text-gris">
              <tr>
                <th className="py-2 pr-4">Chambre</th>
                <th className="py-2 pr-4">Client</th>
                <th className="py-2 pr-4">Séjour</th>
                <th className="py-2 pr-4">État</th>
                <th className="py-2 pr-4 text-right">Total</th>
                <th className="py-2 text-right">Paiement</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-trait">
              {liste.map((r) => (
                <tr key={r.id}>
                  <td className="py-3 pr-4 font-bold">{r.chambre_numero}</td>
                  <td className="py-3 pr-4">
                    {r.client_nom}
                    {r.client_telephone && <span className="block text-sm text-gris">{r.client_telephone}</span>}
                  </td>
                  <td className="py-3 pr-4">
                    {formatSejour(r.date_arrivee, r.date_depart)}
                    <span className="block text-sm text-gris">{pluriel(r.nuits, "nuit")}</span>
                  </td>
                  <td className="py-3 pr-4">
                    <Pastille ton={TON_STATUT[r.statut]}>{LIBELLE_STATUT_RESERVATION[r.statut]}</Pastille>
                  </td>
                  <td className="whitespace-nowrap py-3 pr-4 text-right">{formatMontant(r.total_a_payer, devise)}</td>
                  <td className="py-3 text-right">{r.statut === "annulee" ? "-" : <ResteDu r={r} devise={devise} />}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Carte>
  );
}

function VueCaisse({ copie, toutes, jour, devise }: { copie: Copie; toutes: ReservationVue[]; jour: string; devise: Devise }) {
  const [periode, setPeriode] = useState<"jour" | "semaine" | "mois" | "moisDernier">("jour");
  const moisPasse = debutMois(ajouterJours(debutMois(jour), -1));
  const periodes = {
    jour: { libelle: "Aujourd'hui", debut: jour, fin: ajouterJours(jour, 1) },
    semaine: { libelle: "7 derniers jours", debut: ajouterJours(jour, -6), fin: ajouterJours(jour, 1) },
    mois: { libelle: "Ce mois-ci", debut: debutMois(jour), fin: ajouterJours(jour, 1) },
    moisDernier: { libelle: "Mois dernier", debut: moisPasse, fin: debutMois(jour) },
  } as const;
  const p = periodes[periode];
  const c = caisse(copie, toutes, p.debut, p.fin, decalageLocal());
  const m = (n: number) => formatMontant(n, devise);
  return (
    <>
      <div className="flex flex-wrap gap-2" role="group" aria-label="Période">
        {(Object.keys(periodes) as (keyof typeof periodes)[]).map((k) => (
          <Bouton key={k} variante={periode === k ? "primaire" : "secondaire"} aria-pressed={periode === k} onClick={() => setPeriode(k)}>
            {periodes[k].libelle}
          </Bouton>
        ))}
      </div>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Indicateur titre="Encaissé">{m(c.encaissements)}</Indicateur>
        <Indicateur titre="Remboursé">{m(c.remboursements)}</Indicateur>
        <Indicateur titre="Dépenses">{m(c.depenses)}</Indicateur>
        <Indicateur titre="Reste en caisse" detail="Encaissé − remboursé − dépenses">
          {m(c.net)}
        </Indicateur>
      </div>
      {c.encaissements_par_mode.length > 0 && (
        <Carte titre="Encaissé, par moyen de paiement">
          <ul className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {c.encaissements_par_mode.map((x) => (
              <li key={x.mode} className="flex justify-between rounded-xl bg-sable px-4 py-3">
                <span>{LIBELLE_MODE[x.mode as ModePaiement]}</span>
                <span className="font-bold">{m(x.montant)}</span>
              </li>
            ))}
          </ul>
        </Carte>
      )}
      <div className="grid gap-6 lg:grid-cols-2">
        <Carte titre="Paiements">
          {c.paiements.length === 0 ? (
            <p className="text-gris">Aucun paiement sur cette période.</p>
          ) : (
            <ul className="divide-y divide-trait">
              {c.paiements.map((x, i) => (
                <li key={i} className="flex flex-wrap justify-between gap-2 py-3">
                  <span>
                    <span className="font-semibold">{LIBELLE_TYPE_PAIEMENT[x.type as TypePaiement]}</span>
                    {x.reservation && ` · ch. ${x.reservation.chambre_numero}, ${x.reservation.client_nom}`}
                    <span className="block text-sm text-gris">
                      {formatHorodatage(x.date)} · {LIBELLE_MODE[x.mode as ModePaiement]}
                      {x.par && ` · par ${x.par}`}
                    </span>
                  </span>
                  <span className={`font-bold ${x.type === "remboursement" ? "text-danger" : ""}`}>
                    {x.type === "remboursement" ? "−" : ""}
                    {m(x.montant)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Carte>
        <Carte titre="Dépenses">
          {c.liste_depenses.length === 0 ? (
            <p className="text-gris">Aucune dépense sur cette période.</p>
          ) : (
            <ul className="divide-y divide-trait">
              {c.liste_depenses.map((x, i) => (
                <li key={i} className="flex flex-wrap justify-between gap-2 py-3">
                  <span>
                    <span className="font-semibold">{x.categorie}</span>
                    {x.description && ` · ${x.description}`}
                    <span className="block text-sm text-gris">{formatJour(x.date)}</span>
                  </span>
                  <span className="font-bold">{m(x.montant)}</span>
                </li>
              ))}
            </ul>
          )}
        </Carte>
      </div>
    </>
  );
}

function VueJournal({ copie, version }: { copie: Copie; version: number }) {
  const [sensibles, setSensibles] = useState(false);
  const [nombre, setNombre] = useState(50);
  const toutes = useMemo(() => journal(copie), [copie, version]);
  const liste = toutes.filter((e) => !sensibles || e.sensible);
  return (
    <Carte
      titre="Historique : qui a fait quoi"
      actions={
        <label className="flex items-center gap-2 text-base font-semibold">
          <input type="checkbox" className="size-5 accent-marque" checked={sensibles} onChange={(e) => setSensibles(e.target.checked)} />
          Seulement les actions à vérifier
        </label>
      }
    >
      {liste.length === 0 ? (
        <p className="text-gris">Rien à afficher.</p>
      ) : (
        <>
          <ul className="divide-y divide-trait">
            {liste.slice(0, nombre).map((e) => (
              <li key={e.seq} className="flex flex-wrap items-center justify-between gap-2 py-3">
                <span>
                  <span className="font-semibold">{libelleAction(e.action)}</span>
                  {e.sensible && (
                    <>
                      {" "}
                      <Pastille ton="alerte">À vérifier</Pastille>
                    </>
                  )}
                  <span className="block text-sm text-gris">{e.utilisateur}</span>
                </span>
                <span className="text-base text-gris">{formatHorodatage(e.date)}</span>
              </li>
            ))}
          </ul>
          {liste.length > nombre && (
            <Bouton variante="secondaire" className="mt-4" onClick={() => setNombre((n) => n + 50)}>
              Voir plus ancien
            </Bouton>
          )}
        </>
      )}
    </Carte>
  );
}
