import { useState, type FormEvent } from "react";
import { CircleCheck, Grid3x3, KeyRound } from "lucide-react";

import {
  Alerte,
  Bouton,
  Carte,
  ChampMotDePasse,
  EnTete,
  ReglesMotDePasse,
  motDePasseValide,
  useAction,
} from "../components/ui";
import { api } from "../lib/api";
import { useSession } from "../lib/session";

/** Ce que chaque employé règle lui-même : son mot de passe et son code PIN. */
export function MonCompte() {
  const { session } = useSession();
  return (
    <>
      <EnTete
        titre="Mon compte"
        sousTitre={`${session?.utilisateur.nom ?? ""} · identifiant « ${session?.utilisateur.identifiant ?? ""} »`}
      />
      <div className="grid max-w-3xl gap-6">
        <CodePin />
        <MotDePasse />
      </div>
    </>
  );
}

function CodePin() {
  const { session, rafraichir } = useSession();
  const aUnPin = session?.utilisateur.a_un_pin ?? false;
  const [mdp, setMdp] = useState("");
  const [pin, setPin] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [ok, setOk] = useState(false);
  const { lancer, enCours, erreur, setErreur } = useAction(async () => {
    await api.definirMonPin(mdp, pin);
    await rafraichir();
    return true;
  });

  const modifie = (f: (v: string) => void, chiffres = false) => (v: string) => {
    f(chiffres ? v.replace(/\D/g, "").slice(0, 6) : v);
    setOk(false);
    setErreur(null);
  };

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    if (pin.length < 4) return setErreur("Le code PIN compte 4 à 6 chiffres.");
    if (pin !== confirmation) return setErreur("Les deux codes PIN ne sont pas identiques. Retapez-les.");
    if (/^(\d)\1+$/.test(pin) || "0123456789".includes(pin) || "9876543210".includes(pin))
      return setErreur("Ce code est trop facile à deviner (chiffres qui se suivent ou tous pareils). Choisissez-en un autre.");
    if (!mdp) return setErreur("Pour confirmer, tapez votre mot de passe.");
    if ((await lancer()) !== undefined) {
      setOk(true);
      setMdp("");
      setPin("");
      setConfirmation("");
    }
  };

  return (
    <Carte titre="Code PIN">
      <form onSubmit={soumettre} noValidate aria-label="Code PIN" className="grid gap-4">
        <p className="text-base text-gris">
          Un code de 4 à 6 chiffres pour se connecter plus vite, sur le pavé de chiffres de l'écran de connexion (onglet
          « Code PIN »). Gardez-le pour vous.{" "}
          {aUnPin ? "Vous avez déjà un code PIN : vous pouvez le changer ici." : "Vous n'avez pas encore de code PIN."}
        </p>
        <div className="grid grid-cols-2 gap-4">
          <ChampMotDePasse
            libelle="Nouveau code PIN"
            inputMode="numeric"
            autoComplete="off"
            value={pin}
            onChange={(e) => modifie(setPin, true)(e.target.value)}
          />
          <ChampMotDePasse
            libelle="Retapez le code PIN"
            inputMode="numeric"
            autoComplete="off"
            value={confirmation}
            onChange={(e) => modifie(setConfirmation, true)(e.target.value)}
          />
        </div>
        <ChampMotDePasse
          libelle="Pour confirmer, votre mot de passe"
          autoComplete="current-password"
          value={mdp}
          onChange={(e) => modifie(setMdp)(e.target.value)}
        />
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        {ok && <Alerte type="succes">C'est enregistré : vous pouvez maintenant vous connecter avec votre code PIN.</Alerte>}
        <div className="flex justify-end">
          <Bouton type="submit" icone={Grid3x3} enCours={enCours} grand>
            {aUnPin ? "Changer mon code PIN" : "Enregistrer mon code PIN"}
          </Bouton>
        </div>
      </form>
    </Carte>
  );
}

function MotDePasse() {
  const [actuel, setActuel] = useState("");
  const [nouveau, setNouveau] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [ok, setOk] = useState(false);
  const { lancer, enCours, erreur, setErreur } = useAction(api.changerMonMotDePasse);

  const modifie = (f: (v: string) => void) => (v: string) => {
    f(v);
    setOk(false);
    setErreur(null);
  };

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    if (!actuel) return setErreur("Tapez d'abord votre mot de passe actuel.");
    if (!motDePasseValide(nouveau)) return setErreur("Le nouveau mot de passe ne respecte pas encore toutes les règles.");
    if (nouveau !== confirmation) return setErreur("Les deux nouveaux mots de passe ne sont pas identiques. Retapez-les.");
    if ((await lancer(actuel, nouveau)) !== undefined) {
      setOk(true);
      setActuel("");
      setNouveau("");
      setConfirmation("");
    }
  };

  return (
    <Carte titre="Changer mon mot de passe">
      <form onSubmit={soumettre} noValidate aria-label="Changer mon mot de passe" className="grid gap-4">
        <ChampMotDePasse
          libelle="Mot de passe actuel"
          autoComplete="current-password"
          value={actuel}
          onChange={(e) => modifie(setActuel)(e.target.value)}
        />
        <ChampMotDePasse
          libelle="Nouveau mot de passe"
          autoComplete="new-password"
          value={nouveau}
          onChange={(e) => modifie(setNouveau)(e.target.value)}
          aide={nouveau && <ReglesMotDePasse motDePasse={nouveau} />}
        />
        <ChampMotDePasse
          libelle="Retapez le nouveau mot de passe"
          autoComplete="new-password"
          value={confirmation}
          onChange={(e) => modifie(setConfirmation)(e.target.value)}
          aide={
            confirmation && nouveau === confirmation ? (
              <span className="flex items-center gap-2 text-succes">
                <CircleCheck className="size-5" aria-hidden /> Les deux mots de passe sont identiques
              </span>
            ) : undefined
          }
        />
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        {ok && <Alerte type="succes">C'est enregistré. Utilisez votre nouveau mot de passe à la prochaine connexion.</Alerte>}
        <div className="flex justify-end">
          <Bouton type="submit" icone={KeyRound} enCours={enCours} grand>
            Enregistrer mon mot de passe
          </Bouton>
        </div>
      </form>
    </Carte>
  );
}
