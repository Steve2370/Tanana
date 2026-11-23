import { useCallback, useEffect, useState, type ReactNode } from "react";
import { ShieldCheck, ShieldAlert, Users, TriangleAlert, type LucideIcon } from "lucide-react";

import type { EntreeJournal } from "../bindings/EntreeJournal";
import type { RapportIntegrite } from "../bindings/RapportIntegrite";
import type { SessionActive } from "../bindings/SessionActive";
import { Alerte, Bouton, Carte, EnTete, useConfirmation } from "../components/ui";
import { api } from "../lib/api";
import { versErreur } from "../lib/erreurs";
import { formatHorodatage } from "../lib/format";
import { libelleAction } from "../lib/permissions";
import { useLectureSeule, useSession } from "../lib/session";
import type { IdPage } from "./Coquille";

function salutation(d = new Date()): string {
  const h = d.getHours();
  return h >= 18 || h < 5 ? "Bonsoir" : "Bonjour";
}

export function TableauDeBord({ allerA }: { allerA: (p: IdPage) => void }) {
  const { session, peut } = useSession();
  const lectureSeule = useLectureSeule();
  return (
    <>
      <EnTete
        titre={lectureSeule ? salutation() : `${salutation()} ${session?.utilisateur.nom ?? ""}`}
        sousTitre={new Intl.DateTimeFormat("fr-FR", { weekday: "long", day: "numeric", month: "long", year: "numeric" }).format(new Date())}
      />
      {peut("journal.voir") || peut("utilisateur.gerer") ? (
        <VueAdministrateur allerA={allerA} />
      ) : (
        <Carte>
          <p className="text-lg">Choisissez ce que vous voulez faire dans le menu, à gauche de l'écran.</p>
        </Carte>
      )}
    </>
  );
}

/** Ce que l'admin voit en arrivant : historique protégé, qui est connecté, actions à vérifier. */
function VueAdministrateur({ allerA }: { allerA: (p: IdPage) => void }) {
  const { peut, session } = useSession();
  // À distance, les connexions de l'hôtel ne sont pas transmises : ne pas afficher un « 0 » trompeur.
  const voirConnexions = peut("utilisateur.gerer") && !useLectureSeule();
  const { confirmer, fenetre } = useConfirmation();
  const [integrite, setIntegrite] = useState<RapportIntegrite | null>(null);
  const [sessions, setSessions] = useState<SessionActive[]>([]);
  const [sensibles, setSensibles] = useState<EntreeJournal[]>([]);
  const [recentes, setRecentes] = useState<EntreeJournal[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);

  const charger = useCallback(async () => {
    try {
      if (peut("journal.voir")) {
        const [i, s, r] = await Promise.all([
          api.verifierIntegriteJournal(),
          api.journal({ sensibles_seulement: true, limite: 6 }),
          api.journal({ limite: 10 }),
        ]);
        setIntegrite(i);
        setSensibles(s);
        setRecentes(r);
      }
      if (voirConnexions) setSessions(await api.sessionsActives());
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  }, [peut, voirConnexions]);

  useEffect(() => {
    void charger();
  }, [charger]);

  const deconnecter = async (s: SessionActive) => {
    const ok = await confirmer({
      titre: `Déconnecter ${s.utilisateur_nom} ?`,
      message: "Cette personne devra se reconnecter. Ce qu'elle était en train de saisir sera perdu.",
      libelleConfirmer: "Oui, déconnecter",
    });
    if (!ok) return;
    try {
      await api.fermerSession(s.session_id);
      await charger();
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  };

  return (
    <div className="grid gap-7">
      {fenetre}
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      {integrite && !integrite.intacte && (
        <Alerte type="erreur" titre="Attention : l'historique a été modifié">
          Quelqu'un a modifié ou effacé des lignes directement dans les fichiers de Tanana (à partir de l'entrée n°{" "}
          {integrite.premiere_rupture}). Ce n'est pas possible depuis l'application : contactez votre support.
        </Alerte>
      )}

      <div className={`grid gap-5 ${voirConnexions ? "grid-cols-3" : "grid-cols-2"}`}>
        <Indicateur
          titre="Historique protégé"
          icone={integrite && !integrite.intacte ? ShieldAlert : ShieldCheck}
          ton={integrite && !integrite.intacte ? "danger" : "succes"}
        >
          {integrite ? (integrite.intacte ? "Intact" : "Modifié !") : "-"}
        </Indicateur>
        {voirConnexions && (
          <Indicateur titre="Personnes connectées" icone={Users} ton="marque">
            {sessions.length}
          </Indicateur>
        )}
        <Indicateur titre="Actions à vérifier" icone={TriangleAlert} ton={sensibles.length ? "orange" : "marque"}>
          {sensibles.length}
        </Indicateur>
      </div>

      <div className={`grid gap-6 ${voirConnexions ? "grid-cols-2" : ""}`}>
        {voirConnexions && (
          <Carte titre="Personnes connectées">
            {sessions.length === 0 ? (
              <p className="text-gris">Personne n'est connecté.</p>
            ) : (
              <ul className="divide-y divide-trait">
                {sessions.map((s) => (
                  <li key={s.session_id} className="flex min-h-14 items-center justify-between gap-3 py-2">
                    <span>
                      <span className="font-semibold">{s.utilisateur_nom}</span>
                      <span className="block text-base text-gris">Actif {formatHorodatage(s.derniere_activite)}</span>
                    </span>
                    {s.utilisateur_id === session?.utilisateur.id ? (
                      <span className="text-base font-semibold text-gris">C'est vous</span>
                    ) : (
                      <Bouton variante="secondaire" onClick={() => void deconnecter(s)}>
                        Déconnecter
                      </Bouton>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Carte>
        )}
        <Carte
          titre="Actions à vérifier"
          actions={
            <Bouton variante="discret" onClick={() => allerA("journal")}>
              Tout voir
            </Bouton>
          }
        >
          <p className="mb-3 text-base text-gris">Annulations, remboursements, accès refusés, changements de comptes…</p>
          <ListeActions entrees={sensibles} vide="Rien à signaler." />
        </Carte>
      </div>

      <Carte titre="Dernières actions">
        <ListeActions entrees={recentes} vide="Aucune action pour le moment." />
      </Carte>
    </div>
  );
}

const TONS = {
  succes: "bg-succes-clair text-succes",
  danger: "bg-danger-clair text-danger",
  marque: "bg-marque-clair text-marque",
  orange: "bg-orange-clair text-orange-texte",
};

function Indicateur({
  titre,
  icone: Icone,
  ton,
  children,
}: {
  titre: string;
  icone: LucideIcon;
  ton: keyof typeof TONS;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center gap-4 rounded-2xl border border-trait bg-white p-5 shadow-sm">
      <span className={`flex size-14 shrink-0 items-center justify-center rounded-xl ${TONS[ton]}`}>
        <Icone className="size-7" aria-hidden />
      </span>
      <div>
        <p className="text-base leading-snug font-semibold text-gris">{titre}</p>
        <p className="text-2xl font-bold">{children}</p>
      </div>
    </div>
  );
}

export function ListeActions({ entrees, vide }: { entrees: EntreeJournal[]; vide: string }) {
  if (entrees.length === 0) return <p className="text-gris">{vide}</p>;
  return (
    <ul className="divide-y divide-trait">
      {entrees.map((e) => (
        <li key={e.seq} className="flex min-h-12 items-center justify-between gap-4 py-2">
          <span className="flex items-center gap-2.5">
            {e.sensible && <span className="size-2.5 shrink-0 rounded-full bg-orange" aria-label="à vérifier" />}
            <span>
              <span className="font-semibold">{libelleAction(e.action, e.apres)}</span>
              <span className="text-gris"> - {e.utilisateur_nom ?? "Tanana"}</span>
            </span>
          </span>
          <span className="shrink-0 text-base text-gris">{formatHorodatage(e.date)}</span>
        </li>
      ))}
    </ul>
  );
}
