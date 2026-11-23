import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { ArrowLeft, CircleCheck, Globe, Hotel, KeyRound, LogIn, MonitorSmartphone, RefreshCw } from "lucide-react";

import { EcranCentre } from "../../App";
import type { EtatCopie } from "../../bindings/EtatCopie";
import type { HotelTrouve } from "../../bindings/HotelTrouve";
import { Alerte, Bouton, Carte, Champ, ChampMotDePasse, useAction } from "../../components/ui";
import { api } from "../../lib/api";
import {
  ADRESSE_RELAIS_DEFAUT,
  MISE_A_JOUR_AUTO_MS,
  SILENCE_INQUIETANT_MS,
  adresseCourte,
  depuis,
  nettoyerAdresse,
} from "../../lib/distance";
import { versErreur } from "../../lib/erreurs";
import { formatHorodatage } from "../../lib/format";
import { useSession } from "../../lib/session";

// ───────────────────────────── Identifiant, mot de passe et serveur ─────────────────────────────

interface Identifiants {
  adresse: string;
  identifiant: string;
  motDePasse: string;
}

/**
 * Les mêmes identifiant et mot de passe qu'à l'hôtel. L'adresse du serveur est
 * déjà remplie ; on ne la montre que si la personne veut la changer.
 */
function FormulaireIdentifiants({
  titre,
  explication,
  libelleBouton,
  icone,
  enCours,
  erreur,
  setErreur,
  onValider,
}: {
  titre: string;
  explication: ReactNode;
  libelleBouton: string;
  icone: typeof Globe;
  enCours: boolean;
  erreur: string | null;
  setErreur: (e: string | null) => void;
  onValider: (i: Identifiants) => void;
}) {
  const [identifiant, setIdentifiant] = useState("");
  const [mdp, setMdp] = useState("");
  const [adresse, setAdresse] = useState(ADRESSE_RELAIS_DEFAUT);
  const [avance, setAvance] = useState(false);

  const soumettre = (e: FormEvent) => {
    e.preventDefault();
    const a = nettoyerAdresse(adresse);
    if (!identifiant.trim()) return setErreur("Écrivez votre identifiant (le même qu'à l'hôtel).");
    if (!mdp) return setErreur("Écrivez votre mot de passe.");
    if (!a) {
      setAvance(true);
      return setErreur("Écrivez l'adresse du serveur.");
    }
    onValider({ adresse: a, identifiant: identifiant.trim(), motDePasse: mdp });
  };

  return (
    <Carte titre={titre}>
      <form onSubmit={soumettre} noValidate aria-label={titre} className="grid gap-5">
        <div className="text-lg">{explication}</div>
        <Champ
          libelle="Identifiant"
          value={identifiant}
          onChange={(e) => {
            setIdentifiant(e.target.value);
            setErreur(null);
          }}
          autoComplete="username"
          autoCapitalize="off"
          spellCheck={false}
          autoFocus
        />
        <ChampMotDePasse
          libelle="Mot de passe"
          value={mdp}
          onChange={(e) => {
            setMdp(e.target.value);
            setErreur(null);
          }}
          autoComplete="current-password"
        />
        {avance ? (
          <Champ
            libelle="Adresse du serveur"
            placeholder="https://…"
            inputMode="url"
            autoCapitalize="off"
            spellCheck={false}
            value={adresse}
            onChange={(e) => {
              setAdresse(e.target.value);
              setErreur(null);
            }}
          />
        ) : (
          <button
            type="button"
            onClick={() => setAvance(true)}
            className="justify-self-start rounded-lg px-1 text-base text-gris underline-offset-4 hover:underline"
          >
            Serveur : {adresseCourte(ADRESSE_RELAIS_DEFAUT)} (changer)
          </button>
        )}
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <Bouton type="submit" icone={icone} enCours={enCours} grand>
          {libelleBouton}
        </Bouton>
      </form>
    </Carte>
  );
}

// ───────────────────────────── Retrouver mon hôtel (ordinateur de l'hôtel) ─────────────────────────────

/**
 * Sur l'ordinateur de l'hôtel : tout ce qui a été préparé ailleurs (au Canada)
 * arrive ici, et cet ordinateur devient celui de l'hôtel.
 */
export function RetrouverHotel({ onTermine, onRetour }: { onTermine: () => void; onRetour: () => void }) {
  const [trouve, setTrouve] = useState<HotelTrouve | null>(null);
  const [fini, setFini] = useState(false);
  const recherche = useAction(api.retrouverHotel);
  const reprise = useAction(api.confirmerReprise);

  if (fini) {
    return (
      <EcranCentre>
        <Carte titre="Votre hôtel est sur cet ordinateur">
          <p className="mb-4 flex items-center gap-2 text-lg">
            <CircleCheck className="size-6 text-succes" aria-hidden />
            Tout ce que vous aviez préparé est là.
          </p>
          <p className="mb-6 text-lg">
            Cet ordinateur est maintenant celui de l'hôtel. Il fonctionne même sans Internet ; les changements partent
            vers le serveur dès que la connexion revient. Connectez-vous avec votre identifiant et votre mot de passe.
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

  if (trouve) {
    return (
      <EcranCentre large>
        <Carte titre="Votre hôtel a été trouvé">
          <div className="grid gap-5 text-lg">
            <p>
              Dernière copie complète reçue par le serveur : <strong>{formatHorodatage(trouve.copie_du)}</strong>. Ce qui
              a été fait ensuite arrive aussi.
            </p>
            <Alerte type="attention" titre="Avant de continuer">
              Si vous avez travaillé sur un autre ordinateur (votre portable), ouvrez-y Tanana une minute avec Internet,
              pour qu'il envoie ses derniers changements. Ensuite, c'est cet ordinateur qui gardera l'hôtel ; l'autre
              servira à travailler à distance.
            </Alerte>
            {reprise.erreur && <Alerte type="erreur">{reprise.erreur}</Alerte>}
            <div className="flex flex-wrap justify-end gap-3">
              <Bouton variante="secondaire" onClick={() => setTrouve(null)}>
                Retour
              </Bouton>
              <Bouton
                icone={Hotel}
                enCours={reprise.enCours}
                grand
                onClick={async () => {
                  if ((await reprise.lancer()) !== undefined) setFini(true);
                }}
              >
                Installer l'hôtel sur cet ordinateur
              </Bouton>
            </div>
          </div>
        </Carte>
      </EcranCentre>
    );
  }

  return (
    <EcranCentre large>
      <Bouton variante="secondaire" icone={ArrowLeft} onClick={onRetour} className="mb-5">
        Retour
      </Bouton>
      <FormulaireIdentifiants
        titre="Retrouver mon hôtel"
        explication={
          <p>
            Vous avez déjà créé votre hôtel sur un autre ordinateur ? Tapez votre identifiant et votre mot de passe
            d'administrateur : tout ce que vous avez préparé arrivera sur cet ordinateur.
          </p>
        }
        libelleBouton="Retrouver mon hôtel"
        icone={Hotel}
        enCours={recherche.enCours}
        erreur={recherche.erreur}
        setErreur={recherche.setErreur}
        onValider={async (i) => {
          const r = await recherche.lancer(i.adresse, i.identifiant, i.motDePasse);
          if (r) setTrouve(r);
        }}
      />
    </EcranCentre>
  );
}

// ───────────────────────────── Travailler à distance ─────────────────────────────

/** Sur le portable ou l'ordinateur personnel de l'administratrice. */
export function TravaillerADistance({ onTermine, onRetour }: { onTermine: () => void; onRetour: () => void }) {
  const [fini, setFini] = useState<{ avertissement: string | null } | null>(null);
  const { lancer, enCours, erreur, setErreur } = useAction(api.travaillerADistance);

  if (fini) {
    return (
      <EcranCentre>
        <Carte titre="C'est prêt">
          <p className="mb-4 flex items-center gap-2 text-lg">
            <CircleCheck className="size-6 text-succes" aria-hidden />
            Cet ordinateur peut maintenant suivre et modifier l'hôtel à distance.
          </p>
          {fini.avertissement && (
            <div className="mb-4">
              <Alerte type="attention" titre="Les informations n'ont pas encore pu être téléchargées">
                {fini.avertissement} Tanana réessaiera toute seule.
              </Alerte>
            </div>
          )}
          <p className="mb-6 text-base text-gris">À chaque ouverture, Tanana demandera votre mot de passe.</p>
          <div className="flex justify-end">
            <Bouton onClick={onTermine} grand>
              Continuer
            </Bouton>
          </div>
        </Carte>
      </EcranCentre>
    );
  }

  return (
    <EcranCentre large>
      <Bouton variante="secondaire" icone={ArrowLeft} onClick={onRetour} className="mb-5">
        Retour
      </Bouton>
      <FormulaireIdentifiants
        titre="Travailler à distance"
        explication={
          <p>
            Pour suivre l'hôtel depuis chez vous ou depuis l'étranger, et y faire des changements. Tapez votre
            identifiant et votre mot de passe d'administrateur, les mêmes qu'à l'hôtel.
          </p>
        }
        libelleBouton="Relier cet ordinateur à l'hôtel"
        icone={MonitorSmartphone}
        enCours={enCours}
        erreur={erreur}
        setErreur={setErreur}
        onValider={async (i) => {
          const r = await lancer(i.adresse, i.identifiant, i.motDePasse);
          if (r !== undefined) setFini({ avertissement: r });
        }}
      />
    </EcranCentre>
  );
}

// ───────────────────────────── Connexion sur le poste à distance ─────────────────────────────

export function ConnexionDistante({
  nomHotel,
  version,
  onOublie,
  onConnecte,
}: {
  nomHotel: string | null;
  version: string;
  /** L'hôtel a été oublié : revenir au premier lancement. */
  onOublie: () => void;
  /** `modifications` : l'hôtel a répondu ; sinon, consultation seulement (et pourquoi). */
  onConnecte: (modifications: boolean, raison: string | null) => void;
}) {
  const { ouvrir, messageSortie } = useSession();
  const [mdp, setMdp] = useState("");
  const [oubli, setOubli] = useState(false);
  const connexion = useAction(async (m: string) => {
    const c = await api.connexionDistante(m);
    onConnecte(c.modifications_possibles, c.raison);
    await ouvrir(c.session);
  });

  const soumettre = (e: FormEvent) => {
    e.preventDefault();
    if (!mdp) return connexion.setErreur("Écrivez votre mot de passe.");
    void connexion.lancer(mdp);
  };

  if (oubli) {
    return (
      <OublierHotel titre="Mot de passe oublié" onOublie={onOublie} onAnnuler={() => setOubli(false)}>
        Rien n'est perdu : les informations de l'hôtel sont gardées sur le serveur et à l'hôtel. Si le mot de passe a été
        changé à l'hôtel, tapez simplement le nouveau sur l'écran précédent.
      </OublierHotel>
    );
  }

  return (
    <EcranCentre>
      <Carte>
        <h1 className="text-2xl font-bold text-marque-fonce">{nomHotel ?? "Travail à distance"}</h1>
        <p className="mb-6 flex items-center gap-2 text-lg text-gris">
          <Globe className="size-5" aria-hidden /> Travail à distance
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
            aide="Le même qu'à l'hôtel."
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

// ───────────────────────────── Tout oublier pour relier à nouveau ─────────────────────────────

/** Clé de protection perdue sur le poste à distance (trousseau effacé, nouvel ordinateur). */
export function CleManquanteCopie({ onOublie }: { onOublie: () => void }) {
  return (
    <OublierHotel titre="Cet ordinateur doit être relié à nouveau" onOublie={onOublie}>
      Cet ordinateur suivait un hôtel à distance, mais il ne retrouve plus sa clé de protection (par exemple après un
      changement d'ordinateur). Rien n'est perdu : les informations de l'hôtel sont gardées sur le serveur.
    </OublierHotel>
  );
}

/** Efface la copie de cet ordinateur. Sans danger : elle ne contient que ce que le serveur garde. */
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
            Pour continuer, reliez à nouveau cet ordinateur avec votre <strong>identifiant</strong> et votre{" "}
            <strong>mot de passe</strong> d'administrateur. Mot de passe oublié aussi à l'hôtel ? Le{" "}
            <strong>code de secours</strong> noté à la création de l'hôtel permet d'en choisir un nouveau, sur
            l'ordinateur de l'hôtel.
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
              Relier à nouveau
            </Bouton>
          </div>
        </div>
      </Carte>
    </EcranCentre>
  );
}

// ───────────────────────────── Un autre ordinateur a repris l'hôtel ─────────────────────────────

/** L'ancien ordinateur principal (le portable préparé au Canada) apprend qu'il a été remplacé. */
export function PosteRemplace({ onFait }: { onFait: () => void }) {
  const { lancer, enCours, erreur } = useAction(api.abandonnerPoste);
  return (
    <EcranCentre large>
      <Carte titre="L'hôtel est maintenant sur un autre ordinateur">
        <div className="grid gap-4 text-lg">
          <p>
            Votre hôtel a été installé sur un autre ordinateur (celui de l'hôtel). Pour éviter deux versions différentes,
            cet ordinateur ne peut plus rien y enregistrer.
          </p>
          <p>
            Pour continuer à suivre et modifier l'hôtel d'ici, passez en <strong>travail à distance</strong>. Les
            informations de cet ordinateur ne sont pas effacées : elles sont gardées dans le dossier des copies de
            sécurité.
          </p>
          {erreur && <Alerte type="erreur">{erreur}</Alerte>}
          <div className="flex justify-end">
            <Bouton
              icone={MonitorSmartphone}
              enCours={enCours}
              grand
              onClick={async () => {
                if ((await lancer()) !== undefined) onFait();
              }}
            >
              Passer en travail à distance
            </Bouton>
          </div>
        </div>
      </Carte>
    </EcranCentre>
  );
}

// ───────────────────────────── Bandeau en haut de l'écran ─────────────────────────────

/**
 * Rappelle d'où viennent les chiffres et si les modifications sont possibles.
 * La copie se met à jour toute seule toutes les 15 secondes ; pour ne pas
 * changer la page sous les yeux de la personne, les nouveautés s'affichent
 * quand elle appuie sur « Afficher » (« Mettre à jour » les affiche tout de suite).
 */
export function BandeauConsultation({
  onActualiser,
  modifications,
  onLien,
}: {
  onActualiser: () => void;
  /** Les modifications partent vers l'hôtel (il a répondu). */
  modifications: boolean;
  /** L'hôtel devient joignable ou ne l'est plus. */
  onLien: (modifications: boolean) => void;
}) {
  const [etat, setEtat] = useState<EtatCopie | null>(null);
  const [nouvelles, setNouvelles] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const [enCours, setEnCours] = useState(false);
  const occupe = useRef(false);
  const dernierDonnees = useRef<string | null | undefined>(undefined);

  const mettreAJour = useCallback(
    async (manuel: boolean) => {
      if (occupe.current) return;
      occupe.current = true;
      setEnCours(manuel);
      try {
        const n = manuel ? await api.mettreAJourCopie() : 0;
        setErreur(null);
        const e = await api.etatCopie().catch(() => null);
        setEtat(e);
        // La copie se met à jour toute seule : du nouveau si la date des données a changé
        const changees = dernierDonnees.current !== undefined && e?.donnees_du !== dernierDonnees.current;
        dernierDonnees.current = e?.donnees_du;
        if (manuel) {
          setNouvelles(false);
          if (n > 0 || changees) onActualiser();
        } else if (changees) setNouvelles(true);
        const lien = await api.lienHotel().catch(() => null);
        if (lien) onLien(lien.modifications_possibles);
      } catch (e) {
        const err = versErreur(e);
        setErreur(
          err.code === "HOTEL_INCONNU_DU_RELAIS"
            ? "L'hôtel n'est plus relié au serveur : il n'y aura plus de nouvelles informations. Celles affichées ici sont celles du dernier envoi."
            : err.message,
        );
      } finally {
        occupe.current = false;
        setEnCours(false);
      }
    },
    [onActualiser, onLien],
  );

  useEffect(() => {
    void mettreAJour(false);
    const minuterie = window.setInterval(() => void mettreAJour(false), MISE_A_JOUR_AUTO_MS);
    return () => window.clearInterval(minuterie);
  }, [mettreAJour]);

  const silence =
    etat?.dernier_contact != null && Date.now() - new Date(etat.dernier_contact).getTime() > SILENCE_INQUIETANT_MS;

  return (
    <div
      role="region"
      aria-label="Travail à distance"
      className="mb-6 grid gap-3 rounded-2xl border-2 border-marque/25 bg-marque-clair/60 px-5 py-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="min-w-0 flex-1 basis-80">
          <p className="flex items-center gap-2 text-lg font-bold text-marque-fonce">
            <Globe className="size-5" aria-hidden />
            {modifications
              ? "Travail à distance : vos changements partent directement à l'hôtel."
              : "Travail à distance : l'hôtel n'est pas joignable, consultation seulement."}
          </p>
          <p className="text-base">
            {etat?.donnees_du
              ? `Informations de l'hôtel du ${formatHorodatage(etat.donnees_du)}.`
              : "Aucune information reçue pour l'instant : l'ordinateur de l'hôtel doit être allumé et connecté à Internet."}
            {etat?.dernier_contact && !silence && ` Dernier signe de l'hôtel ${depuis(etat.dernier_contact)}.`}
            {!modifications && " Tanana réessaie toute seule chaque minute."}
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
