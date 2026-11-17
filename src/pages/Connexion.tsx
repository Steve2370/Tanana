import { useEffect, useRef, useState, type FormEvent } from "react";
import { ArrowLeft, KeyRound, LogIn } from "lucide-react";

import { EcranCentre } from "../App";
import { Alerte, Bouton, Carte, Champ, ChampMotDePasse, ClavierPin, ReglesMotDePasse, useAction } from "../components/ui";
import { api } from "../lib/api";
import { useSession } from "../lib/session";

type Mode = "mot_de_passe" | "pin" | "recuperation";

/** Seul l'identifiant est retenu sur ce poste (jamais le mot de passe), pour
 *  éviter de le retaper à chaque fois. */
const CLE_IDENTIFIANT = "tanana.dernier-identifiant";

function identifiantRetenu(): string {
  try {
    return localStorage.getItem(CLE_IDENTIFIANT) ?? "";
  } catch {
    return "";
  }
}

function retenirIdentifiant(id: string) {
  try {
    localStorage.setItem(CLE_IDENTIFIANT, id);
  } catch {
    // Stockage indisponible : sans conséquence
  }
}

export function Connexion({ nomHotel, version }: { nomHotel: string | null; version: string }) {
  const { ouvrir, messageSortie } = useSession();
  const [mode, setMode] = useState<Mode>("mot_de_passe");
  const [identifiant, setIdentifiant] = useState(identifiantRetenu);
  const [secret, setSecret] = useState("");

  const connexion = useAction(async () => {
    const id = identifiant.trim();
    const s = mode === "pin" ? await api.connexionPin(id, secret) : await api.connexion(id, secret);
    retenirIdentifiant(id);
    await ouvrir(s);
  });

  const soumettre = (e?: FormEvent) => {
    e?.preventDefault();
    if (!identifiant.trim()) return connexion.setErreur("Écrivez votre identifiant.");
    if (!secret) return connexion.setErreur(mode === "pin" ? "Tapez votre code PIN." : "Écrivez votre mot de passe.");
    void connexion.lancer();
  };

  // Entrée au clavier physique valide aussi le PIN (la fonction change à chaque saisie).
  const soumettreRef = useRef(() => soumettre());
  soumettreRef.current = () => soumettre();

  // En mode PIN, les chiffres du clavier physique fonctionnent aussi.
  useEffect(() => {
    if (mode !== "pin") return;
    const surTouche = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      if (/^\d$/.test(e.key)) setSecret((s) => (s.length < 6 ? s + e.key : s));
      if (e.key === "Backspace") setSecret((s) => s.slice(0, -1));
      if (e.key === "Enter") {
        e.preventDefault(); // sinon l'onglet encore sélectionné se réactive et efface le PIN
        soumettreRef.current();
      }
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [mode]);

  const changerMode = (m: Mode) => {
    setMode(m);
    setSecret("");
    connexion.setErreur(null);
  };

  if (mode === "recuperation") return <Recuperation onRetour={() => changerMode("mot_de_passe")} />;

  return (
    <EcranCentre>
      <Carte>
        <h1 className="text-2xl font-bold text-marque-fonce">{nomHotel ?? "Bienvenue"}</h1>
        <p className="mb-6 text-lg text-gris">Identifiez-vous pour commencer.</p>

        {messageSortie && (
          <div className="mb-5">
            <Alerte type="attention">{messageSortie}</Alerte>
          </div>
        )}

        <div role="tablist" aria-label="Façon de se connecter" className="mb-6 grid grid-cols-2 gap-1 rounded-xl bg-sable p-1.5">
          {(["mot_de_passe", "pin"] as const).map((m) => (
            <button
              key={m}
              role="tab"
              type="button"
              aria-selected={mode === m}
              onClick={() => changerMode(m)}
              className={`min-h-12 rounded-lg text-base font-semibold transition-colors ${
                mode === m ? "bg-white text-marque-fonce shadow-sm" : "text-gris hover:text-encre"
              }`}
            >
              {m === "pin" ? "Code PIN" : "Mot de passe"}
            </button>
          ))}
        </div>

        <form onSubmit={soumettre} className="grid gap-5" noValidate>
          <Champ
            libelle="Identifiant"
            value={identifiant}
            onChange={(e) => setIdentifiant(e.target.value)}
            autoComplete="username"
            autoCapitalize="off"
            spellCheck={false}
            autoFocus={!identifiant}
          />
          {mode === "pin" ? (
            <div>
              <p className="mb-2 font-semibold">Code PIN</p>
              <ClavierPin valeur={secret} onChange={setSecret} />
            </div>
          ) : (
            <ChampMotDePasse
              libelle="Mot de passe"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              autoComplete="current-password"
              autoFocus={!!identifiant}
            />
          )}
          {connexion.erreur && <Alerte type="erreur">{connexion.erreur}</Alerte>}
          <Bouton type="submit" enCours={connexion.enCours} icone={LogIn} grand>
            Se connecter
          </Bouton>
        </form>

        <div className="mt-6 flex items-center justify-between border-t border-trait pt-4 text-base">
          <button
            type="button"
            className="inline-flex min-h-11 items-center gap-2 rounded-lg px-2 text-marque underline-offset-4 hover:underline"
            onClick={() => changerMode("recuperation")}
          >
            <KeyRound className="size-5" aria-hidden />
            Mot de passe administrateur oublié ?
          </button>
          <span className="text-gris">Version {version}</span>
        </div>
      </Carte>
    </EcranCentre>
  );
}

function Recuperation({ onRetour }: { onRetour: () => void }) {
  const [code, setCode] = useState("");
  const [identifiant, setIdentifiant] = useState("");
  const [mdp, setMdp] = useState("");
  const [nouveauCode, setNouveauCode] = useState<string | null>(null);
  const { lancer, enCours, erreur } = useAction(api.recupererAccesAdmin);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    const c = await lancer(code, identifiant.trim(), mdp);
    if (c) setNouveauCode(c);
  };

  return (
    <EcranCentre>
      <Carte titre="Retrouver l'accès administrateur">
        {nouveauCode ? (
          <div className="grid gap-5">
            <Alerte type="succes" titre="C'est fait : votre nouveau mot de passe est enregistré.">
              L'ancien code de récupération ne fonctionne plus.
            </Alerte>
            <p className="text-lg">Voici votre nouveau code de récupération. Notez-le et rangez-le en lieu sûr :</p>
            <p className="selectionnable rounded-xl border-2 border-dashed border-or bg-or-clair py-4 text-center font-mono text-2xl font-bold tracking-widest">
              {nouveauCode}
            </p>
            <Bouton onClick={onRetour} grand>
              Retour à la connexion
            </Bouton>
          </div>
        ) : (
          <form onSubmit={soumettre} className="grid gap-5">
            <p className="text-lg text-gris">
              Utilisez le code de récupération qui vous a été remis lors de l'installation (sur papier).
            </p>
            <Champ
              libelle="Code de récupération"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="XXXXX-XXXXX-XXXXX-XXXXX"
              aide="Les tirets et les majuscules sont facultatifs."
              autoCapitalize="characters"
              spellCheck={false}
              required
            />
            <Champ libelle="Identifiant administrateur" value={identifiant} onChange={(e) => setIdentifiant(e.target.value)} required />
            <ChampMotDePasse
              libelle="Nouveau mot de passe"
              value={mdp}
              onChange={(e) => setMdp(e.target.value)}
              autoComplete="new-password"
              aide={<ReglesMotDePasse motDePasse={mdp} />}
              required
            />
            {erreur && <Alerte type="erreur">{erreur}</Alerte>}
            <div className="flex justify-between gap-3">
              <Bouton variante="secondaire" icone={ArrowLeft} onClick={onRetour}>
                Retour
              </Bouton>
              <Bouton type="submit" enCours={enCours}>
                Enregistrer le nouveau mot de passe
              </Bouton>
            </div>
          </form>
        )}
      </Carte>
    </EcranCentre>
  );
}
