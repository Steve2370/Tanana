import { useCallback, useEffect, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";

import type { EntreeJournal } from "../bindings/EntreeJournal";
import type { Utilisateur } from "../bindings/Utilisateur";
import { Alerte, Bouton, Carte, EnTete, Pastille, Selection } from "../components/ui";
import { api } from "../lib/api";
import { versErreur } from "../lib/erreurs";
import { formatHorodatage } from "../lib/format";
import { libelleAction } from "../lib/permissions";
import { useSession } from "../lib/session";

const PAGE = 50;

const TYPES = [
  { valeur: "", libelle: "Toutes les actions" },
  { valeur: "auth.", libelle: "Connexions" },
  { valeur: "reservation.", libelle: "Réservations" },
  { valeur: "sejour.", libelle: "Arrivées et départs" },
  { valeur: "paiement.", libelle: "Paiements" },
  { valeur: "depense.", libelle: "Dépenses" },
  { valeur: "utilisateur.", libelle: "Comptes des employés" },
  { valeur: "role.", libelle: "Rôles" },
  { valeur: "acces.", libelle: "Accès refusés" },
];

export function Journal() {
  const { peut } = useSession();
  const [entrees, setEntrees] = useState<EntreeJournal[]>([]);
  const [utilisateurs, setUtilisateurs] = useState<Utilisateur[]>([]);
  const [utilisateurId, setUtilisateurId] = useState("");
  const [prefixe, setPrefixe] = useState("");
  const [sensibles, setSensibles] = useState(false);
  const [fin, setFin] = useState(false);
  const [ouverte, setOuverte] = useState<number | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  const charger = useCallback(
    async (avant: number | null) => {
      try {
        const page = await api.journal({
          utilisateur_id: utilisateurId || null,
          action_prefixe: prefixe || null,
          sensibles_seulement: sensibles,
          avant_seq: avant,
          limite: PAGE,
        });
        setEntrees((anciennes) => (avant === null ? page : [...anciennes, ...page]));
        setFin(page.length < PAGE);
      } catch (e) {
        setErreur(versErreur(e).message);
      }
    },
    [utilisateurId, prefixe, sensibles],
  );

  useEffect(() => {
    void charger(null);
  }, [charger]);

  useEffect(() => {
    if (peut("utilisateur.gerer")) api.listerUtilisateurs().then(setUtilisateurs, () => {});
  }, [peut]);

  return (
    <>
      <EnTete
        titre="Historique"
        sousTitre="Tout ce qui a été fait dans Tanana, par qui et quand. Personne ne peut modifier ni effacer cet historique."
      />
      <Carte>
        <div className="mb-6 grid grid-cols-3 items-end gap-5">
          <Selection
            libelle="Employé"
            valeur={utilisateurId}
            onChange={setUtilisateurId}
            options={[{ valeur: "", libelle: "Tout le monde" }, ...utilisateurs.map((u) => ({ valeur: u.id, libelle: u.nom }))]}
          />
          <Selection libelle="Type d'action" valeur={prefixe} onChange={setPrefixe} options={TYPES} />
          <label className="flex min-h-12 cursor-pointer items-center gap-3 rounded-xl border-2 border-trait px-4 font-semibold has-[:checked]:border-orange has-[:checked]:bg-orange-clair">
            <input type="checkbox" className="size-5 accent-marque" checked={sensibles} onChange={(e) => setSensibles(e.target.checked)} />
            Seulement les actions à vérifier
          </label>
        </div>
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <ul className="divide-y divide-trait border-y border-trait">
          {entrees.map((e) => (
            <LigneJournal key={e.seq} entree={e} ouverte={ouverte === e.seq} basculer={() => setOuverte(ouverte === e.seq ? null : e.seq)} />
          ))}
        </ul>
        {entrees.length === 0 && <p className="py-8 text-center text-lg text-gris">Aucune action ne correspond à ces choix.</p>}
        {!fin && entrees.length > 0 && (
          <div className="mt-5 text-center">
            <Bouton variante="secondaire" onClick={() => void charger(entrees.at(-1)?.seq ?? null)}>
              Afficher les actions plus anciennes
            </Bouton>
          </div>
        )}
      </Carte>
    </>
  );
}

function LigneJournal({ entree: e, ouverte, basculer }: { entree: EntreeJournal; ouverte: boolean; basculer: () => void }) {
  const details = e.avant !== null || e.apres !== null;
  return (
    <li className="py-3">
      <div className="flex min-h-12 items-center gap-4">
        <div className="flex-1">
          <p className="flex flex-wrap items-center gap-2 text-lg font-semibold">
            {libelleAction(e.action)}
            {e.sensible && <Pastille ton="alerte">à vérifier</Pastille>}
          </p>
          <p className="text-base text-gris">
            {e.utilisateur_nom ?? "Tanana"} · {formatHorodatage(e.date)}
          </p>
        </div>
        {details && (
          <Bouton variante="discret" onClick={basculer} aria-expanded={ouverte} icone={ouverte ? ChevronUp : ChevronDown}>
            {ouverte ? "Masquer" : "Détail"}
          </Bouton>
        )}
      </div>
      {ouverte && <Detail avant={e.avant} apres={e.apres} />}
    </li>
  );
}

type Objet = Record<string, unknown>;
const estObjet = (v: unknown): v is Objet => typeof v === "object" && v !== null && !Array.isArray(v);
const lisible = (cle: string) => cle.replaceAll("_", " ").replace(/^./, (c) => c.toUpperCase());
/** Valeurs techniques stockées sans accents → mots lisibles. */
const MOTS: Record<string, string> = {
  desactive: "désactivé",
  confirmee: "confirmée",
  arrivee: "arrivé(e)",
  partie: "parti(e)",
  annulee: "annulée",
  hors_service: "hors service",
  especes: "espèces",
  mot_de_passe: "mot de passe",
  pin: "code PIN",
  true: "oui",
  false: "non",
};
const valeur = (v: unknown): string => {
  if (v === null || v === undefined || v === "") return "—";
  if (typeof v === "object") return JSON.stringify(v);
  const texte = String(v);
  return MOTS[texte] ?? texte;
};

/** Avant / après en tableau lisible, avec les valeurs modifiées mises en évidence. */
export function Detail({ avant, apres }: { avant: unknown; apres: unknown }) {
  const a = estObjet(avant) ? avant : {};
  const b = estObjet(apres) ? apres : {};
  const cles = [...new Set([...Object.keys(a), ...Object.keys(b)])].filter((k) => k !== "id" && !k.endsWith("_id"));
  if (cles.length === 0) return null;
  return (
    <table className="mt-3 mb-1 w-full overflow-hidden rounded-xl bg-sable/60 text-base">
      <thead>
        <tr className="text-left text-gris">
          <th className="px-4 py-2 font-semibold">Information</th>
          {estObjet(avant) && <th className="px-4 py-2 font-semibold">Avant</th>}
          {estObjet(apres) && <th className="px-4 py-2 font-semibold">Après</th>}
        </tr>
      </thead>
      <tbody className="selectionnable">
        {cles.map((k) => {
          const change = estObjet(avant) && estObjet(apres) && valeur(a[k]) !== valeur(b[k]);
          return (
            <tr key={k} className={change ? "bg-or-clair font-semibold" : ""}>
              <td className="px-4 py-1.5">{lisible(k)}</td>
              {estObjet(avant) && <td className="px-4 py-1.5">{valeur(a[k])}</td>}
              {estObjet(apres) && <td className="px-4 py-1.5">{valeur(b[k])}</td>}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
