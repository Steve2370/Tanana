import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { ArrowLeft, CircleCheck, Globe, KeyRound, LogIn, RefreshCw } from "lucide-react";

import { EcranCentre } from "../../App";
import type { EtatCopie } from "../../bindings/EtatCopie";
import { Alerte, Bouton, Carte, Champ, ChampMotDePasse, ReglesMotDePasse, motDePasseValide, useAction } from "../../components/ui";
import { api } from "../../lib/api";
import {
  ADRESSE_RELAIS_DEFAUT,
  MISE_A_JOUR_AUTO_MS,
  SILENCE_INQUIETANT_MS,
  LONGUEUR_CLE,
  caracteresCle,
  depuis,
  nettoyerAdresse,
  nettoyerIdentifiantHotel,
} from "../../lib/distance";
import { versErreur } from "../../lib/erreurs";
import { formatHorodatage } from "../../lib/format";
import { useSession } from "../../lib/session";

// ───────────────────────────── Rattacher cet ordinateur à un hôtel ─────────────────────────────

/** Premier lancement chez le propriétaire : cet ordinateur suivra un hôtel à distance. */
export function RattacherHotel({ onTermine, onRetour }: { onTermine: () => void; onRetour: () => void }) {
  const [adresse, setAdresse] = useState(ADRESSE_RELAIS_DEFAUT);
  const [hotelId, setHotelId] = useState("");
  const [cle, setCle] = useState("");
  const [mdp, setMdp] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [fini, setFini] = useState<{ avertissement: string | null } | null>(null);
  const { lancer, enCours, erreur, setErreur } = useAction(api.consulterHotelDistant);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    const a = nettoyerAdresse(adresse);
    if (!a) return setErreur("Écrivez l'adresse du serveur, notée sur le papier de l'hôtel.");
    if (!hotelId.trim()) return setErreur("Écrivez l'identifiant de l'hôtel (par exemple HK7Q-F29X).");
    if (caracteresCle(cle) !== LONGUEUR_CLE)
      return setErreur(
        `La clé secrète compte ${LONGUEUR_CLE} caractères (6 groupes de 4, puis 2). Vous en avez tapé ${caracteresCle(cle)}. Vérifiez sur le papier.`,
      );
    if (!motDePasseValide(mdp)) return setErreur("Le mot de passe ne respecte pas encore toutes les règles (voir la liste).");
    if (mdp !== confirmation) return setErreur("Les deux mots de passe ne sont pas identiques. Retapez-les.");
    const r = await lancer(a, nettoyerIdentifiantHotel(hotelId), cle, mdp);
    if (r !== undefined) setFini({ avertissement: r });
  };

  if (fini) {
    return (
      <EcranCentre>
        <Carte titre="C'est prêt">
          <p className="mb-4 flex items-center gap-2 text-lg">
            <CircleCheck className="size-6 text-succes" aria-hidden />
            Cet ordinateur peut maintenant consulter l'hôtel.
          </p>
          {fini.avertissement && (
            <div className="mb-4">
              <Alerte type="attention" titre="Les données n'ont pas encore pu être téléchargées">
                {fini.avertissement} Tanana réessaiera en arrière-plan.
              </Alerte>
            </div>
          )}
          <p className="mb-6 text-base text-gris">
            À chaque ouverture, Tanana demandera le mot de passe que vous venez de choisir.
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

  const identiques = confirmation.length > 0 && mdp === confirmation;
  const efface = <T,>(f: (v: T) => void) => (v: T) => {
    f(v);
    setErreur(null);
  };

  return (
    <EcranCentre large>
      <Bouton variante="secondaire" icone={ArrowLeft} onClick={onRetour} className="mb-5">
        Retour
      </Bouton>
      <Carte titre="Consulter un hôtel à distance">
        <form onSubmit={soumettre} noValidate aria-label="Consulter un hôtel à distance" className="grid gap-5">
          <p className="text-lg">
            Recopiez les informations notées à l'hôtel (Réglages › Consultation à distance). Vous pourrez tout voir,
            sans rien pouvoir modifier.
          </p>
          <Champ
            libelle="Adresse du serveur"
            placeholder="https://…"
            inputMode="url"
            autoCapitalize="off"
            spellCheck={false}
            value={adresse}
            onChange={(e) => efface(setAdresse)(e.target.value)}
          />
          <Champ
            libelle="Identifiant de l'hôtel"
            placeholder="Exemple : HK7Q-F29X"
            autoCapitalize="characters"
            spellCheck={false}
            value={hotelId}
            onChange={(e) => efface(setHotelId)(e.target.value)}
          />
          <Champ
            libelle="Clé secrète"
            placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XX"
            autoCapitalize="characters"
            autoComplete="off"
            spellCheck={false}
            aide="6 groupes de 4 caractères, puis 2. Les tirets et les espaces ne comptent pas ; O et 0, I et 1 sont acceptés."
            value={cle}
            onChange={(e) => efface(setCle)(e.target.value)}
          />

          <div className="mt-2 border-t border-trait pt-5">
            <h2 className="text-xl font-bold">Votre mot de passe sur cet ordinateur</h2>
            <p className="text-gris">Il protège l'accès aux informations de l'hôtel sur cet ordinateur.</p>
          </div>
          <ChampMotDePasse
            libelle="Mot de passe"
            value={mdp}
            onChange={(e) => efface(setMdp)(e.target.value)}
            autoComplete="new-password"
            aide={<ReglesMotDePasse motDePasse={mdp} />}
          />
          <ChampMotDePasse
            libelle="Retapez le mot de passe"
            value={confirmation}
            onChange={(e) => efface(setConfirmation)(e.target.value)}
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
          <Bouton type="submit" icone={Globe} enCours={enCours} grand>
            Consulter l'hôtel
          </Bouton>
        </form>
      </Carte>
    </EcranCentre>
  );
}

// ───────────────────────────── Connexion : mot de passe seul ─────────────────────────────

export function ConnexionCopie({
  nomHotel,
  version,
  onOublie,
}: {
  nomHotel: string | null;
  version: string;
  /** L'hôtel a été oublié : revenir au premier lancement. */
  onOublie: () => void;
}) {
  const { ouvrir, messageSortie } = useSession();
  const [mdp, setMdp] = useState("");
  const [oubli, setOubli] = useState(false);
  const connexion = useAction(async (m: string) => ouvrir(await api.connexionCopie(m)));

  const soumettre = (e: FormEvent) => {
    e.preventDefault();
    if (!mdp) return connexion.setErreur("Écrivez votre mot de passe.");
    void connexion.lancer(mdp);
  };

  if (oubli) {
    return (
      <OublierHotel titre="Mot de passe oublié" onOublie={onOublie} onAnnuler={() => setOubli(false)}>
        Le mot de passe de cet ordinateur ne peut pas être retrouvé. Mais rien n'est perdu : les informations de
        l'hôtel sont gardées sur le serveur.
      </OublierHotel>
    );
  }

  return (
    <EcranCentre>
      <Carte>
        <h1 className="text-2xl font-bold text-marque-fonce">{nomHotel ?? "Consultation à distance"}</h1>
        <p className="mb-6 flex items-center gap-2 text-lg text-gris">
          <Globe className="size-5" aria-hidden /> Consultation à distance
        </p>
        {messageSortie && (
          <div className="mb-5">
            <Alerte type="attention">{messageSortie}</Alerte>
          </div>
        )}
        <form onSubmit={soumettre} className="grid gap-5" noValidate>
          <ChampMotDePasse
            libelle="Mot de passe"
            value={mdp}
            onChange={(e) => {
              setMdp(e.target.value);
              connexion.setErreur(null);
            }}
            autoComplete="current-password"
            autoFocus
          />
          {connexion.erreur && <Alerte type="erreur">{connexion.erreur}</Alerte>}
          <Bouton type="submit" enCours={connexion.enCours} icone={LogIn} grand>
            Se connecter
          </Bouton>
        </form>
        <div className="mt-6 flex items-center justify-between border-t border-trait pt-4 text-base">
          <button
            type="button"
            className="inline-flex min-h-11 items-center gap-2 rounded-lg px-2 text-marque underline-offset-4 hover:underline"
            onClick={() => setOubli(true)}
          >
            <KeyRound className="size-5" aria-hidden />
            Mot de passe oublié ?
          </button>
          <span className="text-gris">Version {version}</span>
        </div>
      </Carte>
    </EcranCentre>
  );
}

// ───────────────────────────── Tout oublier pour rattacher à nouveau ─────────────────────────────

/** Clé de protection perdue sur le poste du propriétaire (nouvel ordinateur, trousseau effacé). */
export function CleManquanteCopie({ onOublie }: { onOublie: () => void }) {
  return (
    <OublierHotel titre="Cet ordinateur doit être rattaché à nouveau" onOublie={onOublie}>
      Cet ordinateur consultait un hôtel à distance, mais il ne retrouve plus sa clé de protection (par exemple après un
      changement d'ordinateur). Rien n'est perdu : les informations de l'hôtel sont gardées sur le serveur.
    </OublierHotel>
  );
}

/**
 * Efface la copie de cet ordinateur. Sans danger : elle ne contient que ce que
 * le serveur garde, et la rattacher demande la clé secrète notée à l'hôtel.
 */
function OublierHotel({
  titre,
  children,
  onOublie,
  onAnnuler,
}: {
  titre: string;
  children: ReactNode;
  onOublie: () => void;
  onAnnuler?: () => void;
}) {
  const { lancer, enCours, erreur } = useAction(api.oublierHotelDistant);
  return (
    <EcranCentre>
      <Carte titre={titre}>
        <div className="grid gap-4 text-lg">
          <p>{children}</p>
          <p>
            Pour continuer, rattachez l'hôtel avec l'<strong>identifiant</strong> et la <strong>clé secrète</strong>{" "}
            notés sur papier, et choisissez un nouveau mot de passe. Si vous ne les avez plus, l'hôtel peut les
            réafficher : Réglages › Consultation à distance › Afficher la clé.
          </p>
          {erreur && <Alerte type="erreur">{erreur}</Alerte>}
          <div className="flex flex-wrap justify-end gap-3">
            {onAnnuler && (
              <Bouton variante="secondaire" onClick={onAnnuler}>
                Retour
              </Bouton>
            )}
            <Bouton
              icone={Globe}
              enCours={enCours}
              grand
              onClick={async () => {
                if ((await lancer()) !== undefined) onOublie();
              }}
            >
              Rattacher l'hôtel à nouveau
            </Bouton>
          </div>
        </div>
      </Carte>
    </EcranCentre>
  );
}

// ───────────────────────────── Bandeau en haut de l'écran ─────────────────────────────

/**
 * Rappelle d'où viennent les chiffres et de quand ils datent. Télécharge les
 * nouveautés à l'ouverture puis toutes les 5 minutes. Pour ne pas changer la
 * page sous les yeux de la personne, les nouveautés reçues automatiquement
 * s'affichent quand elle appuie sur « Afficher » ; « Mettre à jour » les affiche tout de suite.
 */
export function BandeauConsultation({ onActualiser }: { onActualiser: () => void }) {
  const [etat, setEtat] = useState<EtatCopie | null>(null);
  const [nouvelles, setNouvelles] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const [enCours, setEnCours] = useState(false);
  const occupe = useRef(false);

  const mettreAJour = useCallback(
    async (manuel: boolean) => {
      if (occupe.current) return;
      occupe.current = true;
      setEnCours(manuel);
      try {
        const n = await api.mettreAJourCopie();
        setErreur(null);
        if (n > 0) {
          if (manuel) onActualiser();
          else setNouvelles(true);
        }
        if (manuel) setNouvelles(false);
      } catch (e) {
        const err = versErreur(e);
        setErreur(
          err.code === "HOTEL_INCONNU_DU_RELAIS"
            ? "L'hôtel a arrêté la consultation à distance : il n'y aura plus de nouvelles informations. Celles affichées ici sont celles du dernier envoi. Pour reprendre, l'hôtel doit la réactiver et vous donner une nouvelle clé."
            : err.message,
        );
      } finally {
        occupe.current = false;
        setEnCours(false);
        setEtat(await api.etatCopie().catch(() => null));
      }
    },
    [onActualiser],
  );

  useEffect(() => {
    void api.etatCopie().then(setEtat, () => {});
    void mettreAJour(false);
    const minuterie = window.setInterval(() => void mettreAJour(false), MISE_A_JOUR_AUTO_MS);
    return () => window.clearInterval(minuterie);
  }, [mettreAJour]);

  const silence =
    etat?.dernier_contact != null && Date.now() - new Date(etat.dernier_contact).getTime() > SILENCE_INQUIETANT_MS;

  return (
    <div
      role="region"
      aria-label="Consultation à distance"
      className="mb-6 grid gap-3 rounded-2xl border-2 border-marque/25 bg-marque-clair/60 px-5 py-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="min-w-0 flex-1 basis-80">
          <p className="flex items-center gap-2 text-lg font-bold text-marque-fonce">
            <Globe className="size-5" aria-hidden />
            Consultation à distance : rien ne peut être modifié d'ici.
          </p>
          <p className="text-base">
            {etat?.donnees_du
              ? `Informations de l'hôtel du ${formatHorodatage(etat.donnees_du)}.`
              : "Aucune information reçue pour l'instant : l'ordinateur de l'hôtel doit être allumé et connecté à Internet."}
            {etat?.dernier_contact && !silence && ` Dernier signe de l'hôtel ${depuis(etat.dernier_contact)}.`}
          </p>
        </div>
        <div className="flex gap-3">
          {nouvelles && (
            <Bouton
              onClick={() => {
                setNouvelles(false);
                onActualiser();
              }}
            >
              Afficher les nouveautés
            </Bouton>
          )}
          <Bouton variante="secondaire" icone={RefreshCw} enCours={enCours} onClick={() => void mettreAJour(true)}>
            Mettre à jour
          </Bouton>
        </div>
      </div>
      {silence && etat?.dernier_contact && (
        <Alerte type="attention" titre={`L'hôtel n'a pas donné de nouvelles depuis ${formatHorodatage(etat.dernier_contact)}`}>
          L'ordinateur de l'hôtel est peut-être éteint, ou Internet est coupé là-bas. Les informations ci-dessous datent
          de ce moment-là.
        </Alerte>
      )}
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
    </div>
  );
}
