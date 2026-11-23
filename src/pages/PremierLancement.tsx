import { useState, type FormEvent } from "react";
import { CircleCheck, History, Hotel, MonitorSmartphone, Printer, Sparkles, type LucideIcon } from "lucide-react";

import { EcranCentre } from "../App";
import {
  Alerte,
  Bouton,
  Carte,
  Champ,
  ChampMotDePasse,
  ReglesMotDePasse,
  Selection,
  motDePasseValide,
  useAction,
} from "../components/ui";
import { api } from "../lib/api";
import { DEVISES } from "../lib/format";
import { RemettreCopie } from "../components/restauration";
import { ADRESSE_RELAIS_DEFAUT, CODE_INVITATION_DEFAUT } from "../lib/distance";
import { RetrouverHotel, TravaillerADistance } from "./distance/ConsultationDistante";

/** Le mot de passe de l'administrateur protège aussi l'accès à distance : un peu plus long. */
export const LONGUEUR_MDP_ADMIN = 10;

export function PremierLancement({ onTermine }: { onTermine: () => void }) {
  const [nomHotel, setNomHotel] = useState("");
  const [devise, setDevise] = useState("XAF");
  const [identifiant, setIdentifiant] = useState("admin");
  const [nom, setNom] = useState("");
  const [mdp, setMdp] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [code, setCode] = useState<string | null>(null);
  const [codeNote, setCodeNote] = useState(false);
  const [choix, setChoix] = useState<"creer" | "retrouver" | "distance" | null>(null);
  const [restauration, setRestauration] = useState<"choix" | "faite" | null>(null);
  const { lancer, enCours, erreur, setErreur } = useAction(api.initialiser);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    if (!motDePasseValide(mdp) || mdp.length < LONGUEUR_MDP_ADMIN)
      return setErreur("Le mot de passe ne respecte pas encore toutes les règles (voir la liste).");
    if (mdp !== confirmation) return setErreur("Les deux mots de passe ne sont pas identiques. Retapez-les.");
    const decimales = DEVISES.find((d) => d.code === devise)?.decimales ?? 0;
    // L'hôtel se relie tout seul au serveur dès qu'Internet est là : rien à régler.
    const c = await lancer(
      {
        nom_hotel: nomHotel,
        devise,
        decimales_devise: decimales,
        admin_identifiant: identifiant,
        admin_nom: nom,
        admin_mot_de_passe: mdp,
      },
      ADRESSE_RELAIS_DEFAUT,
      CODE_INVITATION_DEFAUT,
    );
    if (c) setCode(c);
  };

  if (choix === "retrouver") return <RetrouverHotel onTermine={onTermine} onRetour={() => setChoix(null)} />;
  if (choix === "distance") return <TravaillerADistance onTermine={onTermine} onRetour={() => setChoix(null)} />;

  if (restauration === "faite") {
    return (
      <EcranCentre>
        <Carte titre="L'hôtel est de retour">
          <p className="mb-6 flex items-center gap-2 text-lg">
            <CircleCheck className="size-6 text-succes" aria-hidden />
            Les données de l'hôtel sont sur cet ordinateur. Connectez-vous comme avant.
          </p>
          <div className="flex justify-end">
            <Bouton onClick={onTermine} grand>
              Continuer
            </Bouton>
          </div>
        </Carte>
      </EcranCentre>
    );
  }
  if (restauration === "choix") {
    return (
      <EcranCentre large>
        <Carte titre="Reprendre l'hôtel à partir d'une copie de sécurité">
          <p className="mb-5 text-lg">
            Cet ordinateur remplace celui de l'hôtel (panne, vol) ? Remettez la dernière copie de sécurité, faite sur une
            clé USB.
          </p>
          <RemettreCopie hotelInstalle={false} onAnnuler={() => setRestauration(null)} onFaite={() => setRestauration("faite")} />
        </Carte>
      </EcranCentre>
    );
  }

  if (code) {
    return (
      <EcranCentre large>
        <Carte titre="Étape importante : votre code de secours">
          <p className="text-lg">
            Si un jour le mot de passe administrateur est oublié, ce code est le <strong>seul moyen</strong> de
            retrouver l'accès. Il ne sera <strong>plus jamais affiché</strong>.
          </p>
          <p
            data-testid="code-recuperation"
            className="selectionnable my-6 rounded-xl border-2 border-dashed border-or bg-or-clair py-5 text-center font-mono text-3xl font-bold tracking-widest"
          >
            {code}
          </p>
          <Alerte type="attention">Imprimez-le ou recopiez-le sur papier, puis rangez-le à l'abri, loin de l'ordinateur.</Alerte>
          <label className="mt-6 flex min-h-12 cursor-pointer items-center gap-3 rounded-xl border-2 border-trait px-4 text-lg font-semibold has-[:checked]:border-succes has-[:checked]:bg-succes-clair">
            <input type="checkbox" className="size-6 accent-marque" checked={codeNote} onChange={(e) => setCodeNote(e.target.checked)} />
            J'ai noté ce code en lieu sûr
          </label>
          <div className="mt-6 flex justify-end gap-3">
            <Bouton variante="secondaire" icone={Printer} onClick={() => window.print()}>
              Imprimer
            </Bouton>
            <Bouton disabled={!codeNote} onClick={onTermine} grand>
              Continuer
            </Bouton>
          </div>
        </Carte>
      </EcranCentre>
    );
  }

  if (choix === null) {
    return (
      <EcranCentre large>
        <h1 className="mb-2 text-3xl font-bold text-marque-fonce">Bienvenue dans Tanana</h1>
        <p className="mb-6 text-lg">Que voulez-vous faire sur cet ordinateur ?</p>
        <div className="grid gap-4">
          <Choix
            icone={Sparkles}
            titre="Créer mon hôtel"
            texte="Première fois : vous configurez votre hôtel, ses chambres et vos premières réservations."
            onClick={() => setChoix("creer")}
          />
          <Choix
            icone={Hotel}
            titre="Retrouver mon hôtel"
            texte="Sur l'ordinateur de l'hôtel : tout ce que vous avez déjà préparé sur un autre ordinateur arrive ici."
            onClick={() => setChoix("retrouver")}
          />
          <Choix
            icone={MonitorSmartphone}
            titre="Travailler à distance"
            texte="Sur votre ordinateur personnel : suivre et modifier l'hôtel depuis chez vous ou depuis l'étranger."
            onClick={() => setChoix("distance")}
          />
        </div>
        <button
          type="button"
          onClick={() => setRestauration("choix")}
          className="mt-6 inline-flex min-h-11 items-center gap-2 rounded-lg px-1 text-base text-gris underline-offset-4 hover:underline"
        >
          <History className="size-5" aria-hidden />
          Cet ordinateur remplace celui de l'hôtel et vous avez une copie de sécurité sur clé USB ?
        </button>
      </EcranCentre>
    );
  }

  const identiques = confirmation.length > 0 && mdp === confirmation;

  return (
    <EcranCentre large>
      <Bouton variante="secondaire" onClick={() => setChoix(null)} className="mb-5">
        Retour
      </Bouton>
      <Carte titre="Configurons votre hôtel">
        <form onSubmit={soumettre} className="grid gap-5" noValidate>
          <Champ libelle="Nom de l'hôtel" value={nomHotel} onChange={(e) => setNomHotel(e.target.value)} required />
          <Selection
            libelle="Monnaie utilisée"
            valeur={devise}
            onChange={setDevise}
            options={DEVISES.map((d) => ({ valeur: d.code, libelle: `${d.nom} (${d.code})` }))}
            aide="Elle ne pourra plus être changée ensuite."
          />

          <div className="mt-2 border-t border-trait pt-5">
            <h2 className="text-xl font-bold">Votre compte administrateur</h2>
            <p className="text-gris">C'est le compte qui gère les employés et voit tout l'historique.</p>
          </div>
          <div className="grid grid-cols-2 gap-5">
            <Champ libelle="Votre nom" value={nom} onChange={(e) => setNom(e.target.value)} required />
            <Champ
              libelle="Identifiant"
              value={identifiant}
              onChange={(e) => setIdentifiant(e.target.value)}
              autoComplete="username"
              aide="Le nom à taper pour se connecter."
              required
            />
          </div>
          <ChampMotDePasse
            libelle="Mot de passe"
            value={mdp}
            onChange={(e) => setMdp(e.target.value)}
            autoComplete="new-password"
            aide={
              <>
                <ReglesMotDePasse motDePasse={mdp} />
                <span className={`flex items-center gap-2 ${mdp.length >= LONGUEUR_MDP_ADMIN ? "text-succes" : "text-gris"}`}>
                  {mdp.length >= LONGUEUR_MDP_ADMIN ? (
                    <CircleCheck className="size-5" aria-hidden />
                  ) : (
                    <span className="mx-[3px] size-3.5 rounded-full border-2 border-current" aria-hidden />
                  )}
                  Au moins {LONGUEUR_MDP_ADMIN} caractères : il sert aussi à travailler à distance
                </span>
              </>
            }
          />
          <ChampMotDePasse
            libelle="Retapez le mot de passe"
            value={confirmation}
            onChange={(e) => setConfirmation(e.target.value)}
            autoComplete="new-password"
            aide={
              identiques ? (
                <span className="flex items-center gap-2 text-succes">
                  <CircleCheck className="size-5" aria-hidden /> Les deux mots de passe sont identiques
                </span>
              ) : undefined
            }
          />
          {erreur && <Alerte type="erreur">{erreur}</Alerte>}
          <Bouton type="submit" enCours={enCours} grand>
            Créer l'hôtel
          </Bouton>
        </form>
      </Carte>

    </EcranCentre>
  );
}

/** Une grande carte de choix, facile à viser. */
function Choix({ icone: Icone, titre, texte, onClick }: { icone: LucideIcon; titre: string; texte: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-5 rounded-2xl border-2 border-trait bg-white px-6 py-5 text-left shadow-sm transition hover:border-marque hover:bg-marque-clair/40 focus-visible:border-marque"
    >
      <Icone className="size-10 shrink-0 text-marque" aria-hidden />
      <span>
        <span className="block text-xl font-bold text-encre">{titre}</span>
        <span className="block text-lg text-gris">{texte}</span>
      </span>
    </button>
  );
}
