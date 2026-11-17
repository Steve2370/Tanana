import { useCallback, useEffect, useState, type ReactNode } from "react";
import { History, RotateCw } from "lucide-react";

import type { EtatApplication } from "./bindings/EtatApplication";
import { LogoComplet } from "./components/Logo";
import { RemettreCopie } from "./components/restauration";
import { Alerte, Bouton, Carte } from "./components/ui";
import { api } from "./lib/api";
import { versErreur } from "./lib/erreurs";
import { FournisseurLectureSeule, useSession } from "./lib/session";
import { ChangementMotDePasse } from "./pages/ChangementMotDePasse";
import { CleManquante } from "./pages/CleManquante";
import { Connexion } from "./pages/Connexion";
import { Coquille } from "./pages/Coquille";
import { CleManquanteCopie, ConnexionCopie } from "./pages/distance/ConsultationDistante";
import { PremierLancement } from "./pages/PremierLancement";

/** Aiguillage au démarrage : clé manquante → premier lancement → connexion → app.
 *  Sur l'ordinateur d'un propriétaire qui consulte son hôtel à distance, même
 *  chemin, mais connexion par mot de passe seul et tout en lecture seule. */
export function App() {
  const { session } = useSession();
  const [etat, setEtat] = useState<EtatApplication | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  const charger = useCallback(async () => {
    try {
      setErreur(null);
      setEtat(await api.etatApplication());
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  }, []);

  useEffect(() => {
    void charger();
  }, [charger]);

  if (erreur || etat?.statut === "erreur") {
    return <ErreurDemarrage message={erreur ?? etat?.message ?? ""} base={etat?.statut === "erreur"} onReessayer={charger} />;
  }
  // Pas de texte « Chargement… » qui clignote : le démarrage est quasi instantané.
  if (!etat) return <div className="h-full bg-papier" />;
  if (etat.statut === "cle_manquante")
    return etat.consultation_distance ? <CleManquanteCopie onOublie={charger} /> : <CleManquante onOuverte={charger} />;
  if (!etat.initialise) return <PremierLancement onTermine={charger} />;
  if (etat.consultation_distance) {
    if (!session) return <ConnexionCopie nomHotel={etat.nom_hotel} version={etat.version} onOublie={charger} />;
    return (
      <FournisseurLectureSeule value>
        <Coquille nomHotel={etat.nom_hotel ?? "Tanana"} />
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
