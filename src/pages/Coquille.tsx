import { useCallback, useState, type ReactNode } from "react";
import {
  BedDouble,
  CalendarDays,
  History,
  House,
  LogOut,
  UserRound,
  Settings,
  UserCog,
  Users,
  Wallet,
  type LucideIcon,
} from "lucide-react";

import type { Permission } from "../bindings/Permission";
import { LogoSymbole } from "../components/Logo";
import { BandeauMiseAJour } from "../components/miseAJour";
import { useLectureSeule, useSession } from "../lib/session";
import { BandeauConsultation } from "./distance/ConsultationDistante";
import { Caisse } from "./caisse/Caisse";
import { Clients } from "./clients/Clients";
import { Chambres } from "./Chambres";
import { Journal } from "./Journal";
import { MonCompte } from "./MonCompte";
import { Parametres } from "./Parametres";
import { Reservations } from "./reservations/Reservations";
import { TableauDeBord } from "./TableauDeBord";
import { Utilisateurs } from "./Utilisateurs";

export type IdPage =
  | "accueil"
  | "planning"
  | "clients"
  | "chambres"
  | "caisse"
  | "utilisateurs"
  | "journal"
  | "parametres"
  /** Hors menu : bouton « Mon compte » en bas, pour chaque employé. */
  | "compte";

interface EntreeMenu {
  id: IdPage;
  libelle: string;
  icone: LucideIcon;
  /** Il suffit d'une des permissions pour voir l'entrée. */
  permissions: Permission[] | null;
  section: "Au quotidien" | "Administration";
}

export const MENU: EntreeMenu[] = [
  { id: "accueil", libelle: "Accueil", icone: House, permissions: null, section: "Au quotidien" },
  { id: "planning", libelle: "Réservations", icone: CalendarDays, permissions: ["reservation.voir"], section: "Au quotidien" },
  { id: "clients", libelle: "Clients", icone: Users, permissions: ["client.voir"], section: "Au quotidien" },
  { id: "chambres", libelle: "Chambres", icone: BedDouble, permissions: ["chambre.voir"], section: "Au quotidien" },
  { id: "caisse", libelle: "Caisse", icone: Wallet, permissions: ["rapport.financier.voir", "depense.voir"], section: "Au quotidien" },
  { id: "utilisateurs", libelle: "Employés et accès", icone: UserCog, permissions: ["utilisateur.gerer"], section: "Administration" },
  { id: "journal", libelle: "Historique", icone: History, permissions: ["journal.voir"], section: "Administration" },
  { id: "parametres", libelle: "Réglages", icone: Settings, permissions: ["parametres.gerer", "sauvegarde.faire"], section: "Administration" },
];

export function Coquille({ nomHotel }: { nomHotel: string }) {
  const { session, peut, fermer } = useSession();
  const [page, setPage] = useState<IdPage>("accueil");
  const lectureSeule = useLectureSeule();
  // Consultation à distance : nouvelles données → la page se recharge.
  const [donnees, setDonnees] = useState(0);
  const actualiser = useCallback(() => setDonnees((n) => n + 1), []);
  const visibles = MENU.filter((m) => m.permissions === null || m.permissions.some(peut));
  const courante = visibles.some((m) => m.id === page) || (page === "compte" && !lectureSeule) ? page : "accueil";

  const contenu: Record<IdPage, ReactNode> = {
    accueil: <TableauDeBord allerA={setPage} />,
    planning: <Reservations />,
    clients: <Clients />,
    chambres: <Chambres />,
    caisse: <Caisse />,
    utilisateurs: <Utilisateurs />,
    journal: <Journal />,
    parametres: <Parametres />,
    compte: <MonCompte />,
  };

  return (
    <div className="flex h-full">
      <nav aria-label="Menu principal" className="flex w-72 shrink-0 flex-col border-r border-trait bg-white">
        <div className="flex items-center gap-3 border-b border-trait px-5 py-4">
          <LogoSymbole taille={46} />
          <span className="text-lg leading-tight font-bold text-marque-fonce" title={nomHotel}>
            {nomHotel}
          </span>
        </div>
        <div className="flex-1 overflow-y-auto px-3 py-4">
          {(["Au quotidien", "Administration"] as const).map((section) => {
            const entrees = visibles.filter((m) => m.section === section);
            if (entrees.length === 0) return null;
            return (
              <div key={section} className="mb-5">
                <p className="mb-2 px-3 text-sm font-bold tracking-wide text-gris uppercase">{section}</p>
                <ul className="grid gap-1">
                  {entrees.map((m) => {
                    const actif = courante === m.id;
                    return (
                      <li key={m.id}>
                        <button
                          type="button"
                          aria-current={actif ? "page" : undefined}
                          onClick={() => setPage(m.id)}
                          className={`relative flex min-h-12 w-full items-center gap-3 rounded-xl px-3 text-left text-lg transition-colors ${
                            actif ? "bg-marque-clair font-bold text-marque-fonce" : "text-encre hover:bg-sable"
                          }`}
                        >
                          {actif && <span className="absolute inset-y-2 left-0 w-1.5 rounded-full bg-orange" aria-hidden />}
                          <m.icone className="size-6 shrink-0" aria-hidden strokeWidth={2} />
                          {m.libelle}
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            );
          })}
        </div>
        <div className="border-t border-trait p-4">
          {lectureSeule ? (
            <>
              <p className="truncate text-lg font-bold">{session?.utilisateur.nom}</p>
              <p className="mb-3 text-base text-gris">{session?.utilisateur.role_nom}</p>
            </>
          ) : (
            <button
              type="button"
              aria-label="Mon compte (mot de passe, PIN)"
              title={session?.utilisateur.role_nom}
              aria-current={courante === "compte" ? "page" : undefined}
              onClick={() => setPage("compte")}
              className={`-mx-2 mb-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-xl px-2 py-1.5 text-left ${
                courante === "compte" ? "bg-marque-clair" : "hover:bg-sable"
              }`}
            >
              <UserRound className="size-6 shrink-0 text-marque" aria-hidden />
              <span className="min-w-0">
                <span className="block truncate text-lg font-bold">{session?.utilisateur.nom}</span>
                <span className="block truncate text-base font-semibold text-marque">Mon compte, code PIN</span>
              </span>
            </button>
          )}
          <button
            type="button"
            onClick={() => void fermer()}
            className="flex min-h-12 w-full items-center gap-3 rounded-xl border-2 border-trait px-3 text-base font-semibold text-encre hover:border-marque/40 hover:bg-marque-clair/50"
          >
            <LogOut className="size-5" aria-hidden />
            Se déconnecter
          </button>
        </div>
      </nav>
      <main className="flex-1 overflow-y-auto px-10 py-9">
        {(lectureSeule || peut("parametres.gerer")) && (
          <div className="mx-auto max-w-6xl">
            <BandeauMiseAJour />
          </div>
        )}
        {lectureSeule && (
          <div className="mx-auto max-w-6xl">
            <BandeauConsultation onActualiser={actualiser} />
          </div>
        )}
        <div key={`${courante}-${donnees}`} className="apparition mx-auto max-w-6xl">
          {contenu[courante]}
        </div>
      </main>
    </div>
  );
}
