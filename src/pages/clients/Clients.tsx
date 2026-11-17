import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, CalendarPlus, Pencil, Search, UserPlus } from "lucide-react";

import type { Client } from "../../bindings/Client";
import type { DonneesClient } from "../../bindings/DonneesClient";
import type { Reservation } from "../../bindings/Reservation";
import { Alerte, Bouton, Carte, Champ, EnTete, Pastille } from "../../components/ui";
import { api } from "../../lib/api";
import { formatSejour, pluriel } from "../../lib/dates";
import { versErreur } from "../../lib/erreurs";
import { formatHorodatage, formatMontant } from "../../lib/format";
import { LIBELLE_STATUT_RESERVATION } from "../../lib/libelles";
import { useDevise, useSession } from "../../lib/session";
import { nomComplet } from "../reservations/ChoixClient";
import { FicheReservation } from "../reservations/FicheReservation";
import { NouvelleReservation } from "../reservations/NouvelleReservation";
import { donneesDe, FormulaireClient } from "./FormulaireClient";

/** Nombre maximum de clients renvoyés par une recherche (voir le service Rust). */
export const LIMITE_RECHERCHE = 200;

type Vue =
  | { type: "liste" }
  | { type: "fiche"; id: string; message?: string }
  | { type: "reservation"; clientId: string; reservationId: string; message?: string }
  | { type: "reserver"; client: Client };

/** Écran Clients : une chose à la fois, comme pour les réservations. */
export function Clients() {
  const [vue, setVue] = useState<Vue>({ type: "liste" });
  // La recherche est gardée ici pour la retrouver en revenant d'une fiche.
  const [recherche, setRecherche] = useState("");
  const haut = useRef<HTMLDivElement>(null);
  useEffect(() => {
    haut.current?.closest("main")?.scrollTo?.({ top: 0 });
  }, [vue]);

  let contenu: ReactNode;
  if (vue.type === "fiche") {
    contenu = (
      <FicheClient
        key={vue.id}
        id={vue.id}
        messageInitial={vue.message}
        onRetour={() => setVue({ type: "liste" })}
        onOuvrirReservation={(reservationId) => setVue({ type: "reservation", clientId: vue.id, reservationId })}
        onReserver={(client) => setVue({ type: "reserver", client })}
      />
    );
  } else if (vue.type === "reservation") {
    contenu = (
      <FicheReservation
        key={vue.reservationId}
        id={vue.reservationId}
        messageInitial={vue.message}
        libelleRetour="Retour à la fiche du client"
        onRetour={() => setVue({ type: "fiche", id: vue.clientId })}
      />
    );
  } else if (vue.type === "reserver") {
    contenu = (
      <NouvelleReservation
        clientInitial={vue.client}
        libelleRetour="Retour à la fiche du client"
        onRetour={() => setVue({ type: "fiche", id: vue.client.id })}
        onCree={(r) =>
          setVue({
            type: "reservation",
            clientId: vue.client.id,
            reservationId: r.id,
            message: `La réservation de ${r.client_nom} est enregistrée (chambre ${r.chambre_numero}, ${pluriel(r.nuits, "nuit")}). Si le client verse un acompte, encaissez-le ci-dessous.`,
          })
        }
      />
    );
  } else {
    contenu = (
      <ListeClients
        recherche={recherche}
        onRecherche={setRecherche}
        onOuvrir={(id, message) => setVue({ type: "fiche", id, message })}
      />
    );
  }
  return (
    <>
      <div ref={haut} />
      {contenu}
    </>
  );
}

// ───────────────────────────────── Liste ─────────────────────────────────

function ListeClients({
  recherche,
  onRecherche,
  onOuvrir,
}: {
  recherche: string;
  onRecherche: (r: string) => void;
  onOuvrir: (id: string, message?: string) => void;
}) {
  const { peut } = useSession();
  const [clients, setClients] = useState<Client[] | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [creation, setCreation] = useState(false);

  useEffect(() => {
    let annule = false;
    api.rechercherClients(recherche.trim()).then(
      (c) => {
        if (annule) return;
        setClients(c);
        setErreur(null);
      },
      (e) => !annule && setErreur(versErreur(e).message),
    );
    return () => {
      annule = true;
    };
  }, [recherche]);

  const q = recherche.trim();

  return (
    <>
      <EnTete
        titre="Clients"
        sousTitre="Retrouvez un client pour voir sa fiche et ses séjours."
        actions={
          peut("client.gerer") &&
          !creation && (
            <Bouton icone={UserPlus} onClick={() => setCreation(true)} grand>
              Nouveau client
            </Bouton>
          )
        }
      />
      {creation && (
        <div className="mb-6">
          <NouveauClient
            nomPropose={/^[\d\s+().-]+$/.test(q) ? "" : q}
            telephonePropose={/^[\d\s+().-]+$/.test(q) ? q : ""}
            onOuvrir={onOuvrir}
            onFermer={() => setCreation(false)}
          />
        </div>
      )}

      <Carte>
        <div className="relative mb-5">
          <Champ
            libelle="Chercher un client"
            value={recherche}
            onChange={(e) => onRecherche(e.target.value)}
            placeholder="Nom, prénom, téléphone ou courriel"
            spellCheck={false}
            autoFocus
          />
          <Search className="pointer-events-none absolute top-11 right-4 size-6 text-gris" aria-hidden />
        </div>
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        {clients?.length === 0 &&
          (q ? (
            <p className="text-lg text-gris">Aucun client ne correspond à « {q} ». Vérifiez l'orthographe, ou cherchez par téléphone.</p>
          ) : (
            <p className="text-lg text-gris">Aucun client pour le moment. Ils s'ajoutent ici, ou directement en faisant une réservation.</p>
          ))}
        {clients && clients.length > 0 && (
          <>
            <p className="mb-3 text-gris">
              {q ? `${pluriel(clients.length, "client")} trouvé${clients.length > 1 ? "s" : ""}` : "Tous les clients, par ordre alphabétique"}
            </p>
            <ul className="grid gap-2" aria-label="Clients">
              {clients.map((c) => (
                <li key={c.id}>
                  <button
                    type="button"
                    onClick={() => onOuvrir(c.id)}
                    className="flex min-h-16 w-full flex-wrap items-center justify-between gap-x-4 rounded-xl border-2 border-trait px-4 py-2 text-left hover:border-marque/40 hover:bg-marque-clair/50"
                  >
                    <span className="text-lg font-bold">{nomComplet(c)}</span>
                    <span className="text-gris">{c.telephone ?? c.email ?? ""}</span>
                  </button>
                </li>
              ))}
            </ul>
            {clients.length >= LIMITE_RECHERCHE && (
              <p className="mt-4 text-gris">Seuls les {LIMITE_RECHERCHE} premiers sont affichés : tapez un nom pour trouver les autres.</p>
            )}
          </>
        )}
      </Carte>
    </>
  );
}

// ─────────────────────────── Nouveau client ───────────────────────────

const sansAccents = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase()
    .replace(/\s+/g, " ")
    .trim();
const chiffres = (s: string | null) => (s ?? "").replace(/\D/g, "");

/** Clients existants qui ressemblent à celui qu'on va créer (même nom complet, ou même téléphone). */
export function doublonsPossibles(d: DonneesClient, existants: Client[]): Client[] {
  const nom = sansAccents(`${d.nom} ${d.prenom ?? ""}`);
  const tel = chiffres(d.telephone);
  return existants.filter((c) => {
    const memeNom = sansAccents(`${c.nom} ${c.prenom ?? ""}`) === nom;
    const memeTel = tel.length >= 6 && chiffres(c.telephone).length >= 6 && chiffres(c.telephone).endsWith(tel.slice(-8));
    return memeNom || memeTel;
  });
}

function NouveauClient({
  nomPropose,
  telephonePropose,
  onOuvrir,
  onFermer,
}: {
  nomPropose: string;
  telephonePropose: string;
  onOuvrir: (id: string, message?: string) => void;
  onFermer: () => void;
}) {
  const [enCours, setEnCours] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const [doublons, setDoublons] = useState<Client[]>([]);
  /** Données pour lesquelles on a déjà prévenu : « Enregistrer quand même » crée sans redemander. */
  const [dejaPrevenu, setDejaPrevenu] = useState<string | null>(null);

  const enregistrer = async (d: DonneesClient) => {
    setEnCours(true);
    setErreur(null);
    try {
      const cle = JSON.stringify(d);
      if (dejaPrevenu !== cle) {
        const recherches = [api.rechercherClients(d.nom)];
        if (d.telephone) recherches.push(api.rechercherClients(d.telephone));
        const trouves = doublonsPossibles(d, (await Promise.all(recherches)).flat());
        const uniques = [...new Map(trouves.map((c) => [c.id, c])).values()];
        if (uniques.length > 0) {
          setDoublons(uniques);
          setDejaPrevenu(cle);
          return;
        }
      }
      const c = await api.creerClient(d);
      onOuvrir(c.id, `La fiche de ${nomComplet(c)} est enregistrée.`);
    } catch (e) {
      setErreur(versErreur(e).message);
    } finally {
      setEnCours(false);
    }
  };

  return (
    <FormulaireClient
      titre="Nouveau client"
      initial={{ ...donneesDe(null), nom: nomPropose, telephone: telephonePropose || null }}
      libelleValider={doublons.length > 0 ? "Enregistrer quand même" : "Enregistrer le client"}
      enCours={enCours}
      erreur={erreur}
      onValider={(d) => void enregistrer(d)}
      onFermer={onFermer}
      avertissement={
        doublons.length > 0 && (
          <Alerte type="attention" titre="Ce client est peut-être déjà enregistré">
            <p>Vérifiez avant de créer une deuxième fiche pour la même personne :</p>
            <ul className="mt-3 grid gap-2">
              {doublons.map((c) => (
                <li key={c.id}>
                  <button
                    type="button"
                    onClick={() => onOuvrir(c.id)}
                    className="flex min-h-12 w-full items-center justify-between gap-3 rounded-xl border-2 border-or bg-white px-4 text-left text-encre hover:bg-or-clair"
                  >
                    <span className="font-bold">{nomComplet(c)}</span>
                    <span>{c.telephone ?? ""} · Ouvrir sa fiche</span>
                  </button>
                </li>
              ))}
            </ul>
            <p className="mt-3">Si c'est bien une autre personne, appuyez sur « Enregistrer quand même ».</p>
          </Alerte>
        )
      }
    />
  );
}

// ────────────────────────────── Fiche client ──────────────────────────────

function FicheClient({
  id,
  messageInitial,
  onRetour,
  onOuvrirReservation,
  onReserver,
}: {
  id: string;
  messageInitial?: string;
  onRetour: () => void;
  onOuvrirReservation: (reservationId: string) => void;
  onReserver: (c: Client) => void;
}) {
  const { peut } = useSession();
  const devise = useDevise();
  const [client, setClient] = useState<Client | null>(null);
  const [sejours, setSejours] = useState<Reservation[] | null>(null);
  const [modification, setModification] = useState(false);
  const [enCours, setEnCours] = useState(false);
  const [message, setMessage] = useState<string | null>(messageInitial ?? null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [erreurFormulaire, setErreurFormulaire] = useState<string | null>(null);
  const voitSejours = peut("reservation.voir");

  const charger = useCallback(async () => {
    const [c, s] = await Promise.all([api.obtenirClient(id), voitSejours ? api.reservationsDuClient(id) : null]);
    setClient(c);
    setSejours(s);
  }, [id, voitSejours]);

  useEffect(() => {
    charger().catch((e) => setErreur(versErreur(e).message));
  }, [charger]);

  const enregistrer = async (d: DonneesClient) => {
    setEnCours(true);
    setErreurFormulaire(null);
    try {
      setClient(await api.modifierClient(id, d));
      setModification(false);
      setMessage("Les changements sont enregistrés.");
    } catch (e) {
      setErreurFormulaire(versErreur(e).message);
    } finally {
      setEnCours(false);
    }
  };

  const retour = (
    <Bouton variante="secondaire" icone={ArrowLeft} onClick={onRetour} className="mb-5">
      Retour à la liste des clients
    </Bouton>
  );
  if (!client) {
    return (
      <>
        {retour}
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      </>
    );
  }

  const valides = (sejours ?? []).filter((r) => r.statut !== "annulee");
  const totalPaye = (sejours ?? []).reduce((s, r) => s + r.total_encaisse, 0);
  const resteDu = valides.reduce((s, r) => s + Math.max(0, r.solde), 0);
  const m = (montant: number) => formatMontant(montant, devise);

  return (
    <>
      {retour}
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight text-marque-fonce">{nomComplet(client)}</h1>
          <p className="mt-1.5 text-lg text-gris">Client depuis {formatHorodatage(client.cree_le)}</p>
        </div>
        {peut("reservation.creer") && (
          <Bouton icone={CalendarPlus} onClick={() => onReserver(client)} grand>
            Réserver pour ce client
          </Bouton>
        )}
      </div>

      <div className="grid max-w-5xl gap-6">
        {message && <Alerte type="succes">{message}</Alerte>}
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}

        {modification ? (
          <FormulaireClient
            titre="Modifier la fiche"
            initial={donneesDe(client)}
            libelleValider="Enregistrer les changements"
            enCours={enCours}
            erreur={erreurFormulaire}
            onValider={(d) => void enregistrer(d)}
            onFermer={() => {
              setModification(false);
              setErreurFormulaire(null);
            }}
          />
        ) : (
          <Carte
            titre="Coordonnées"
            actions={
              peut("client.gerer") && (
                <Bouton
                  variante="secondaire"
                  icone={Pencil}
                  onClick={() => {
                    setModification(true);
                    setMessage(null);
                  }}
                >
                  Modifier
                </Bouton>
              )
            }
          >
            <dl className="grid grid-cols-2 gap-x-8 gap-y-4 text-lg">
              <Info libelle="Téléphone">{client.telephone}</Info>
              <Info libelle="Courriel">{client.email}</Info>
              <Info libelle="Pièce d'identité">{client.piece_identite}</Info>
              <Info libelle="Remarques">{client.notes}</Info>
            </dl>
          </Carte>
        )}

        {sejours && (
          <Carte titre="Séjours">
            {sejours.length === 0 ? (
              <p className="text-lg text-gris">Aucun séjour pour le moment.</p>
            ) : (
              <>
                <div role="group" aria-label="Résumé des séjours" className="mb-5 grid grid-cols-3 gap-4">
                  <Chiffre libelle="Séjours">{valides.length}</Chiffre>
                  <Chiffre libelle="Total payé">{m(totalPaye)}</Chiffre>
                  <Chiffre libelle="Reste à payer" alerte={resteDu > 0}>
                    {resteDu > 0 ? m(resteDu) : "Rien"}
                  </Chiffre>
                </div>
                <ul className="grid gap-2" aria-label="Séjours du client">
                  {sejours.map((r) => (
                    <li key={r.id}>
                      <button
                        type="button"
                        onClick={() => onOuvrirReservation(r.id)}
                        className="flex min-h-16 w-full flex-wrap items-center justify-between gap-x-4 gap-y-1 rounded-xl border-2 border-trait px-4 py-2 text-left hover:border-marque/40 hover:bg-marque-clair/50"
                      >
                        <span>
                          <span className={`block text-lg font-semibold first-letter:uppercase ${r.statut === "annulee" ? "text-gris line-through" : ""}`}>
                            {formatSejour(r.date_arrivee, r.date_depart)}
                          </span>
                          <span className="block text-gris">
                            Chambre {r.chambre_numero} · {pluriel(r.nuits, "nuit")} · {m(r.prix_total)}
                          </span>
                        </span>
                        <span className="flex flex-wrap items-center gap-2">
                          {r.statut !== "annulee" && r.solde > 0 && <Pastille ton="alerte">Reste {m(r.solde)}</Pastille>}
                          <Pastille ton={r.statut === "annulee" ? "danger" : r.statut === "arrivee" ? "alerte" : r.statut === "confirmee" ? "succes" : "neutre"}>
                            {LIBELLE_STATUT_RESERVATION[r.statut]}
                          </Pastille>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </Carte>
        )}
      </div>
    </>
  );
}

function Info({ libelle, children }: { libelle: string; children: string | null }) {
  return (
    <div>
      <dt className="text-base font-semibold text-gris">{libelle}</dt>
      <dd className={`selectionnable ${children ? "" : "text-gris"}`}>{children ?? "Non renseigné"}</dd>
    </div>
  );
}

function Chiffre({ libelle, alerte = false, children }: { libelle: string; alerte?: boolean; children: ReactNode }) {
  return (
    <div className={`rounded-xl border-2 px-4 py-3 ${alerte ? "border-orange bg-orange-clair text-orange-texte" : "border-trait bg-white"}`}>
      <p className="text-base font-semibold">{libelle}</p>
      <p className="text-2xl font-bold">{children}</p>
    </div>
  );
}
