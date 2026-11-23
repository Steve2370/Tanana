import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { History, RotateCw } from "lucide-react";

import type { EtatApplication } from "./bindings/EtatApplication";
import { LogoComplet } from "./components/Logo";
import { RemettreCopie } from "./components/restauration";
import { Alerte, Bouton, Carte } from "./components/ui";
import { api, definirModeDistant } from "./lib/api";
import { versErreur } from "./lib/erreurs";
import { FournisseurLectureSeule, useSession } from "./lib/session";
import { ChangementMotDePasse } from "./pages/ChangementMotDePasse";
import { CleManquante } from "./pages/CleManquante";
import { Connexion } from "./pages/Connexion";
import { Coquille } from "./pages/Coquille";
import { CleManquanteCopie, ConnexionDistante, PosteRemplace } from "./pages/distance/ConsultationDistante";
import { PremierLancement } from "./pages/PremierLancement";

/** Aiguillage au démarrage : clé manquante → premier lancement → connexion → app.
 *  Sur le poste à distance de l'administratrice, même chemin : connexion par
 *  mot de passe, modifications envoyées à l'hôtel quand il est joignable. */
export function App() {
  const { session, rafraichir } = useSession();
  const [etat, setEtat] = useState<EtatApplication | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [modifications, setModifications] = useState(false);

  const charger = useCallback(async () => {
    try {
      setErreur(null);
      const e = await api.etatApplication();
      definirModeDistant(e.consultation_distance);
      setEtat(e);
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  }, []);

  useEffect(() => {
    void charger();
  }, [charger]);

  // L'hôtel devient joignable (ou ne l'est plus) : droits et écran suivent.
  const modificationsActuelles = useRef(modifications);
  modificationsActuelles.current = modifications;
  const surLien = useCallback(
    (possibles: boolean) => {
      if (modificationsActuelles.current === possibles) return;
      setModifications(possibles);
      void rafraichir().catch(() => {});
    },
    [rafraichir],
  );

  // Ordinateur de l'hôtel : si un autre ordinateur l'a remplacé, on le saura en cours de route.
  const surveiller = etat?.statut === "prete" && etat.initialise && !etat.consultation_distance && !etat.poste_remplace;
  useEffect(() => {
    if (!surveiller) return;
    const minuterie = window.setInterval(async () => {
      const e = await api.etatApplication().catch(() => null);
      if (e?.poste_remplace) setEtat(e);
    }, 60_000);
    return () => window.clearInterval(minuterie);
  }, [surveiller]);

  if (erreur || etat?.statut === "erreur") {
    return <ErreurDemarrage message={erreur ?? etat?.message ?? ""} base={etat?.statut === "erreur"} onReessayer={charger} />;
  }
  // Pas de texte « Chargement… » qui clignote : le démarrage est quasi instantané.
  if (!etat) return <div className="h-full bg-papier" />;
  if (etat.statut === "cle_manquante")
    return etat.consultation_distance ? <CleManquanteCopie onOublie={charger} /> : <CleManquante onOuverte={charger} />;
  if (etat.poste_remplace) return <PosteRemplace onFait={charger} />;
  if (!etat.initialise) return <PremierLancement onTermine={charger} />;
  if (etat.consultation_distance) {
    if (!session)
      return (
        <ConnexionDistante
          nomHotel={etat.nom_hotel}
          version={etat.version}
          onOublie={charger}
          onConnecte={(m) => setModifications(m)}
        />
      );
    return (
      <FournisseurLectureSeule value={!modifications}>
        <Coquille nomHotel={etat.nom_hotel ?? "Tanana"} distant={{ modifications, onLien: surLien }} />
      </FournisseurLectureSeule>
    );
  }
  if (!session) return <Connexion nomHotel={etat.nom_hotel} version={etat.version} />;
  if (session.utilisateur.doit_changer_mdp) return <ChangementMotDePasse />;
  return <Coquille nomHotel={etat.nom_hotel ?? "Tanana"} />;
}

/** Écran d'accueil centré, avec le logo. */
export function EcranCentre({ children, large = false }: { children: ReactNode; large?: boolean }) {
  return (
    <main className="flex min-h-full items-center justify-center overflow-y-auto bg-gradient-to-b from-white to-sable px-6 py-8">
      <div className={`apparition w-full ${large ? "max-w-2xl" : "max-w-lg"}`}>
        <div className="mb-6">
          <LogoComplet hauteur={large ? 100 : 120} />
        </div>
        {children}
      </div>
    </main>
  );
}

/** Démarrage impossible. Si la base elle-même est en cause, une copie de sécurité peut la remplacer. */
function ErreurDemarrage({ message, base, onReessayer }: { message: string; base: boolean; onReessayer: () => void }) {
  const [restauration, setRestauration] = useState(false);
  if (restauration) {
    return (
      <EcranCentre large>
        <Carte titre="Remettre une copie de sécurité">
          <p className="mb-5 text-lg">
            Les données actuelles ne sont pas effacées : elles sont mises de côté dans le dossier des copies de sécurité.
          </p>
          <RemettreCopie hotelInstalle={false} onAnnuler={() => setRestauration(false)} onFaite={onReessayer} />
        </Carte>
      </EcranCentre>
    );
  }
  return (
    <EcranCentre>
      <Alerte type="erreur" titre="Tanana ne peut pas démarrer">
        <p className="selectionnable mt-1">{message}</p>
        <p className="mt-2">Fermez l'application et rouvrez-la. Si le problème continue, notez ce message pour le support.</p>
      </Alerte>
      <div className="mt-5 flex flex-wrap gap-3">
        <Bouton variante="secondaire" icone={RotateCw} onClick={onReessayer}>
          Réessayer
        </Bouton>
        {base && (
          <Bouton variante="secondaire" icone={History} onClick={() => setRestauration(true)}>
            Remettre une copie de sécurité
          </Bouton>
        )}
      </div>
    </EcranCentre>
  );
}
