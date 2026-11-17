import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import type { Parametres } from "../bindings/Parametres";
import type { Permission } from "../bindings/Permission";
import type { SessionOuverte } from "../bindings/SessionOuverte";
import { Dialogue } from "../components/ui";
import type { Devise } from "./format";
import { api, dernierContactServeur, jetonSession, quandSessionPerdue } from "./api";

interface ContexteSession {
  session: SessionOuverte | null;
  parametres: Parametres | null;
  /** Message affiché sur l'écran de connexion après une sortie forcée. */
  messageSortie: string | null;
  peut: (p: Permission) => boolean;
  ouvrir: (s: SessionOuverte) => Promise<void>;
  fermer: (message?: string) => Promise<void>;
  rafraichir: () => Promise<void>;
}

const Contexte = createContext<ContexteSession | null>(null);

/** Prévenir une minute avant le verrouillage automatique. */
export const AVERTISSEMENT_MS = 60_000;
/** Pendant que la personne travaille, prolonger la session côté Rust au plus toutes les 4 min. */
const PROLONGATION_MS = 4 * 60_000;

export function FournisseurSession({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<SessionOuverte | null>(null);
  const [parametres, setParametres] = useState<Parametres | null>(null);
  const [messageSortie, setMessageSortie] = useState<string | null>(null);
  const [secondesRestantes, setSecondesRestantes] = useState<number | null>(null);
  const derniereActivite = useRef(Date.now());
  const avertissementVisible = useRef(false);
  const derniereProlongation = useRef(0);

  const vider = useCallback((message?: string) => {
    jetonSession.definir(null);
    setSession(null);
    setParametres(null);
    setSecondesRestantes(null);
    avertissementVisible.current = false;
    setMessageSortie(message ?? null);
  }, []);

  const ouvrir = useCallback(async (s: SessionOuverte) => {
    jetonSession.definir(s.token);
    derniereActivite.current = Date.now();
    setMessageSortie(null);
    setSession(s);
    if (!s.utilisateur.doit_changer_mdp) {
      setParametres(await api.parametres());
    }
  }, []);

  const fermer = useCallback(
    async (message?: string) => {
      try {
        if (jetonSession.lire()) await api.deconnexion();
      } catch {
        // Déjà fermée côté serveur : rien à faire
      }
      vider(message);
    },
    [vider],
  );

  const rafraichir = useCallback(async () => {
    const s = await api.maSession();
    setSession(s);
    if (!s.utilisateur.doit_changer_mdp) setParametres(await api.parametres());
  }, []);

  /** « Je suis là » : repart pour un délai complet, côté écran et côté Rust. */
  const resterConnecte = useCallback(() => {
    derniereActivite.current = Date.now();
    avertissementVisible.current = false;
    setSecondesRestantes(null);
    void api.maSession().catch(() => {});
  }, []);

  // Toute commande qui répond « session perdue » ramène à l'écran de connexion.
  useEffect(() => {
    quandSessionPerdue((e) => vider(e.message));
    return () => quandSessionPerdue(null);
  }, [vider]);

  // Inactivité : avertir une minute avant, puis verrouiller.
  useEffect(() => {
    if (!session || !parametres) return;
    const delaiMs = parametres.delai_inactivite_minutes * 60_000;

    const activite = () => {
      // Pendant l'avertissement, seul le bouton « Je suis là » compte :
      // un geste involontaire ne doit pas cacher le message.
      if (avertissementVisible.current) return;
      const maintenant = Date.now();
      derniereActivite.current = maintenant;
      const dernierEchange = Math.max(dernierContactServeur(), derniereProlongation.current);
      if (maintenant - dernierEchange > PROLONGATION_MS) {
        derniereProlongation.current = maintenant;
        void api.maSession().catch(() => {});
      }
    };
    window.addEventListener("pointerdown", activite);
    window.addEventListener("keydown", activite);

    const minuterie = window.setInterval(() => {
      const reste = delaiMs - (Date.now() - derniereActivite.current);
      if (reste <= 0) {
        void fermer(
          `Par sécurité, vous avez été déconnecté après ${parametres.delai_inactivite_minutes} minutes sans activité. Reconnectez-vous pour continuer.`,
        );
      } else if (reste <= AVERTISSEMENT_MS) {
        avertissementVisible.current = true;
        setSecondesRestantes(Math.ceil(reste / 1000));
      }
    }, 1000);

    return () => {
      window.removeEventListener("pointerdown", activite);
      window.removeEventListener("keydown", activite);
      window.clearInterval(minuterie);
    };
  }, [session, parametres, fermer]);

  const valeur = useMemo<ContexteSession>(
    () => ({
      session,
      parametres,
      messageSortie,
      peut: (p) => session?.permissions.includes(p) ?? false,
      ouvrir,
      fermer,
      rafraichir,
    }),
    [session, parametres, messageSortie, ouvrir, fermer, rafraichir],
  );

  return (
    <Contexte.Provider value={valeur}>
      {children}
      {secondesRestantes !== null && session && (
        <Dialogue
          titre="Êtes-vous toujours là ?"
          libelleConfirmer="Oui, je suis là"
          libelleAnnuler="Me déconnecter"
          onConfirmer={resterConnecte}
          onAnnuler={() => void fermer()}
          focusSur="confirmer"
          onEchap={resterConnecte}
        >
          <p>
            Par sécurité, Tanana va vous déconnecter dans{" "}
            <strong className="text-encre">
              {secondesRestantes} seconde{secondesRestantes > 1 ? "s" : ""}
            </strong>
            .
          </p>
          <p className="mt-2">Pour continuer votre travail, appuyez sur « Oui, je suis là ».</p>
        </Dialogue>
      )}
    </Contexte.Provider>
  );
}

export function useSession(): ContexteSession {
  const c = useContext(Contexte);
  if (!c) throw new Error("useSession doit être utilisé dans <FournisseurSession>");
  return c;
}

/** Affiche son contenu seulement si l'utilisateur a la permission.
 *  Confort d'affichage uniquement : Rust revérifie chaque commande. */
export function SiPermis({ permission, children }: { permission: Permission; children: ReactNode }) {
  const { peut } = useSession();
  return peut(permission) ? <>{children}</> : null;
}

/** Monnaie de l'hôtel, pour afficher et lire les montants. */
export function useDevise(): Devise {
  const { parametres } = useSession();
  return useMemo(
    () => ({ code: parametres?.devise ?? "XAF", decimales: parametres?.decimales_devise ?? 0 }),
    [parametres?.devise, parametres?.decimales_devise],
  );
}

// ───────────── Consultation à distance : tout l'écran en lecture seule ─────────────

const ContexteLectureSeule = createContext(false);

/** Entoure l'application quand cet ordinateur consulte un hôtel à distance. */
export const FournisseurLectureSeule = ContexteLectureSeule.Provider;

/** Vrai si rien ne peut être modifié d'ici (consultation à distance). Rust refuse de toute façon. */
export function useLectureSeule(): boolean {
  return useContext(ContexteLectureSeule);
}
