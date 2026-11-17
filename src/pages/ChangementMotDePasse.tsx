import { useState, type FormEvent } from "react";
import { CircleCheck } from "lucide-react";

import { EcranCentre } from "../App";
import { Alerte, Bouton, Carte, ChampMotDePasse, ReglesMotDePasse, motDePasseValide, useAction } from "../components/ui";
import { api } from "../lib/api";
import { useSession } from "../lib/session";

/** Écran imposé tant que le mot de passe temporaire n'a pas été changé. */
export function ChangementMotDePasse() {
  const { session, rafraichir, fermer } = useSession();
  const [actuel, setActuel] = useState("");
  const [nouveau, setNouveau] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const { lancer, enCours, erreur, setErreur } = useAction(async () => {
    await api.changerMonMotDePasse(actuel, nouveau);
    await rafraichir();
  });

  const soumettre = (e: FormEvent) => {
    e.preventDefault();
    if (!motDePasseValide(nouveau)) return setErreur("Le nouveau mot de passe ne respecte pas encore toutes les règles.");
    if (nouveau !== confirmation) return setErreur("Les deux nouveaux mots de passe ne sont pas identiques. Retapez-les.");
    void lancer();
  };

  return (
    <EcranCentre>
      <Carte titre={`Bonjour ${session?.utilisateur.nom ?? ""}`}>
        <p className="mb-6 text-lg text-gris">
          On vous a donné un mot de passe provisoire. Choisissez maintenant votre mot de passe personnel : vous seul
          le connaîtrez.
        </p>
        <form onSubmit={soumettre} className="grid gap-5" noValidate>
          <ChampMotDePasse
            libelle="Mot de passe provisoire"
            value={actuel}
            onChange={(e) => setActuel(e.target.value)}
            autoComplete="current-password"
            autoFocus
          />
          <ChampMotDePasse
            libelle="Nouveau mot de passe"
            value={nouveau}
            onChange={(e) => setNouveau(e.target.value)}
            autoComplete="new-password"
            aide={<ReglesMotDePasse motDePasse={nouveau} />}
          />
          <ChampMotDePasse
            libelle="Retapez le nouveau mot de passe"
            value={confirmation}
            onChange={(e) => setConfirmation(e.target.value)}
            autoComplete="new-password"
            aide={
              confirmation && nouveau === confirmation ? (
                <span className="flex items-center gap-2 text-succes">
                  <CircleCheck className="size-5" aria-hidden /> Les deux mots de passe sont identiques
                </span>
              ) : undefined
            }
          />
          {erreur && <Alerte type="erreur">{erreur}</Alerte>}
          <div className="flex justify-between gap-3">
            <Bouton variante="secondaire" onClick={() => void fermer()}>
              Annuler
            </Bouton>
            <Bouton type="submit" enCours={enCours} grand>
              Enregistrer mon mot de passe
            </Bouton>
          </div>
        </form>
      </Carte>
    </EcranCentre>
  );
}
