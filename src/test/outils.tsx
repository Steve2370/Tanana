/** Outils de test : simule le côté Rust et ouvre une session. */
import { render } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { useEffect, useState, type ReactNode } from "react";

import type { Erreur } from "../bindings/Erreur";
import type { Parametres } from "../bindings/Parametres";
import type { Permission } from "../bindings/Permission";
import type { SessionOuverte } from "../bindings/SessionOuverte";
import type { Utilisateur } from "../bindings/Utilisateur";
import { jetonSession } from "../lib/api";
import { TOUTES_PERMISSIONS } from "../lib/permissions";
import { FournisseurSession, useSession } from "../lib/session";

export type Gestionnaire = (args: Record<string, unknown>) => unknown;

export interface Appel {
  commande: string;
  args: Record<string, unknown>;
}

/** Remplace Rust : chaque commande répond via son gestionnaire. Une commande
 *  inattendue fait échouer le test. Retourne la liste des appels reçus. */
export function simulerRust(gestionnaires: Record<string, Gestionnaire>): Appel[] {
  const appels: Appel[] = [];
  mockIPC((commande, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    appels.push({ commande, args: a });
    const g = gestionnaires[commande];
    if (!g) throw { code: "BASE", details: `commande non simulée : ${commande}` } satisfies Erreur;
    return g(a);
  });
  return appels;
}

/** Simule une erreur Rust sérialisée. */
export function erreurRust(e: Erreur): never {
  throw e;
}

export const PARAMETRES: Parametres = {
  nom_hotel: "Hôtel Test",
  devise: "XAF",
  decimales_devise: 0,
  delai_inactivite_minutes: 15,
  adresse_hotel: null,
  telephone_hotel: null,
  email_hotel: null,
  mentions_documents: null,
};

export function utilisateur(partiel: Partial<Utilisateur> = {}): Utilisateur {
  return {
    id: "u-admin",
    identifiant: "admin",
    nom: "Beltran",
    role_id: "role-admin",
    role_nom: "Administrateur",
    statut: "actif",
    doit_changer_mdp: false,
    a_un_pin: false,
    cree_le: "2026-03-10T14:00:00.000Z",
    derniere_connexion: null,
    ...partiel,
  };
}

export function session(permissions: Permission[] = TOUTES_PERMISSIONS, u: Partial<Utilisateur> = {}): SessionOuverte {
  return { token: "jeton-test", utilisateur: utilisateur(u), permissions };
}

function OuvrirSession({ s, children }: { s: SessionOuverte; children: ReactNode }) {
  const { ouvrir } = useSession();
  const [pret, setPret] = useState(false);
  useEffect(() => {
    void ouvrir(s).then(() => setPret(true));
  }, [ouvrir, s]);
  // Les enfants restent affichés après une déconnexion, pour pouvoir la vérifier.
  return pret ? <>{children}</> : null;
}

/** Rend `ui` avec une session déjà ouverte. Ajoute `parametres` aux gestionnaires. */
export function rendreConnecte(ui: ReactNode, s: SessionOuverte = session()) {
  jetonSession.definir(null);
  return render(
    <FournisseurSession>
      <OuvrirSession s={s}>{ui}</OuvrirSession>
    </FournisseurSession>,
  );
}
