import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Globe, KeyRound, RefreshCcw, Send, Unplug } from "lucide-react";

import type { CodesDistance } from "../../bindings/CodesDistance";
import type { EtatDistance } from "../../bindings/EtatDistance";
import { CodesANoter } from "../../components/distance";
import { Alerte, Bouton, Carte, Champ, ChampMotDePasse, useAction, useConfirmation } from "../../components/ui";
import { api } from "../../lib/api";
import { pluriel } from "../../lib/dates";
import { ADRESSE_RELAIS_DEFAUT, CODE_INVITATION_DEFAUT, adresseCourte, nettoyerAdresse } from "../../lib/distance";
import { versErreur } from "../../lib/erreurs";
import { formatHorodatage } from "../../lib/format";

/** L'état se rafraîchit tout seul pendant que la carte est affichée. */
const RAFRAICHISSEMENT_MS = 30_000;

/** Réglages › Travail à distance (ordinateur de l'hôtel). */
export function ReglagesDistance() {
  const [etat, setEtat] = useState<EtatDistance | null>(null);
  const [erreurChargement, setErreurChargement] = useState<string | null>(null);
  const [codes, setCodes] = useState<CodesDistance | null>(null);
  /** Après « Arrêter » : la carte repasse au formulaire, le message doit rester. */
  const [arret, setArret] = useState<{ type: "succes" | "attention"; texte: string } | null>(null);

  const charger = useCallback(async () => {
    try {
      setEtat(await api.etatDistance());
      setErreurChargement(null);
    } catch (e) {
      setErreurChargement(versErreur(e).message);
    }
  }, []);

  useEffect(() => {
    void charger();
    const minuterie = window.setInterval(() => void charger(), RAFRAICHISSEMENT_MS);
    return () => window.clearInterval(minuterie);
  }, [charger]);

  return (
    <Carte titre="Travail à distance">
      <div className="grid gap-4">
        <p className="text-base text-gris">
          Un administrateur peut suivre et modifier l'hôtel depuis un autre ordinateur, même à l'étranger, avec son
          identifiant et son mot de passe. Tanana continue de fonctionner sans Internet : les changements partent dès
          que la connexion revient. Tout est chiffré sur cet ordinateur ; même le serveur ne peut rien lire.
        </p>
        {erreurChargement && <Alerte type="erreur">{erreurChargement}</Alerte>}
        {arret && <Alerte type={arret.type}>{arret.texte}</Alerte>}
        {codes ? (
          <CodesANoter codes={codes} onMasquer={() => setCodes(null)} />
        ) : (
          etat &&
          (etat.active ? (
            <Active
              etat={etat}
              onChange={charger}
              onCodes={setCodes}
              onArrete={(m) => {
                setArret(m);
                void charger();
              }}
            />
          ) : (
            <Activation
              onActivee={() => {
                setArret({
                  type: "succes",
                  texte:
                    "L'hôtel est relié au serveur. Vous pouvez maintenant travailler à distance avec votre identifiant et votre mot de passe.",
                });
                void charger();
              }}
            />
          ))
        )}
      </div>
    </Carte>
  );
}

function Activation({ onActivee }: { onActivee: (c: CodesDistance) => void }) {
  const [adresse, setAdresse] = useState(ADRESSE_RELAIS_DEFAUT);
  const [code, setCode] = useState(CODE_INVITATION_DEFAUT ?? "");
  const [avance, setAvance] = useState(!ADRESSE_RELAIS_DEFAUT);
  const { lancer, enCours, erreur, setErreur } = useAction(api.activerDistance);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    const a = nettoyerAdresse(adresse);
    if (!a) {
      setAvance(true);
      return setErreur("Écrivez l'adresse du serveur.");
    }
    const c = await lancer(a, code.trim() || null);
    if (c) onActivee(c);
  };

  return (
    <form onSubmit={soumettre} noValidate aria-label="Relier l'hôtel au serveur" className="grid gap-4">
      <p className="text-base">
        Cet hôtel n'est pas encore relié au serveur (pas d'Internet à sa création ?). Tanana réessaie toute seule ; vous
        pouvez aussi le faire maintenant.
      </p>
      <Champ
        libelle="Code d'invitation (facultatif)"
        aide="Donné avec Tanana, s'il vous en faut un."
        autoCapitalize="characters"
        spellCheck={false}
        value={code}
        onChange={(e) => {
          setCode(e.target.value);
          setErreur(null);
        }}
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
          className="justify-self-start rounded-lg px-1 text-base text-marque underline-offset-4 hover:underline"
        >
          Serveur : {adresseCourte(ADRESSE_RELAIS_DEFAUT)} (changer)
        </button>
      )}
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      <div className="flex justify-end">
        <Bouton type="submit" icone={Globe} enCours={enCours} grand>
          Relier l'hôtel au serveur
        </Bouton>
      </div>
    </form>
  );
}

function Active({
  etat,
  onChange,
  onCodes,
  onArrete,
}: {
  etat: EtatDistance;
  onChange: () => Promise<void>;
  onCodes: (c: CodesDistance) => void;
  onArrete: (message: { type: "succes" | "attention"; texte: string }) => void;
}) {
  const { confirmer, fenetre } = useConfirmation();
  const [message, setMessage] = useState<{ type: "succes" | "attention"; texte: string } | null>(null);
  const [demandeCle, setDemandeCle] = useState(false);
  const [mdp, setMdp] = useState("");

  const action = useAction(async (f: () => Promise<string | { type: "succes" | "attention"; texte: string }>) => {
    setMessage(null);
    const r = await f();
    setMessage(typeof r === "string" ? { type: "succes", texte: r } : r);
    await onChange();
  });
  const cle = useAction(api.afficherCleDistance);
  const arret = useAction(api.desactiverDistance);

  const envoyer = () =>
    void action.lancer(async () => {
      const b = await api.envoyerDistance();
      return b.changements > 0
        ? `C'est envoyé : ${pluriel(b.changements, "changement")}.`
        : "Rien de nouveau à envoyer. Le serveur sait que l'hôtel fonctionne.";
    });

  const toutRenvoyer = async () => {
    const ok = await confirmer({
      titre: "Tout renvoyer ?",
      message:
        "À utiliser seulement si l'administrateur à distance ne voit pas certaines informations. Tout l'hôtel sera renvoyé ; cela peut prendre du temps avec une connexion lente.",
      libelleConfirmer: "Oui, tout renvoyer",
    });
    if (ok)
      void action.lancer(async () => {
        const n = await api.renvoyerToutDistance();
        return `Envoi complet préparé (${pluriel(n, "information")}). Il part tout seul dans la minute.`;
      });
  };

  const arreter = async () => {
    const ok = await confirmer({
      titre: "Arrêter le travail à distance ?",
      message:
        "Plus personne ne pourra voir ni modifier l'hôtel à distance, et les données seront effacées du serveur. Il faudra tout relier à nouveau pour reprendre.",
      libelleConfirmer: "Oui, arrêter",
      danger: true,
    });
    if (!ok) return;
    setMessage(null);
    const efface = await arret.lancer();
    if (efface === undefined) return;
    onArrete(
      efface
        ? { type: "succes", texte: "Le travail à distance est arrêté. Les données ont été effacées du serveur." }
        : {
            type: "attention",
            texte:
              "Le travail à distance est arrêté ici. Le serveur n'a pas pu être joint pour effacer les données ; elles ne sont plus mises à jour et restent illisibles sans la clé.",
          },
    );
  };

  const afficherCle = async (e: FormEvent) => {
    e.preventDefault();
    if (!mdp) return cle.setErreur("Écrivez votre mot de passe.");
    const c = await cle.lancer(mdp);
    setMdp("");
    if (c) {
      setDemandeCle(false);
      onCodes(c);
    }
  };

  return (
    <div className="grid gap-4">
      {fenetre}
      <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-lg">
        <dt className="text-gris">Identifiant de l'hôtel</dt>
        <dd className="selectionnable font-mono font-bold tracking-wider">{etat.hotel_id}</dd>
        <dt className="text-gris">Dernier envoi</dt>
        <dd>{etat.dernier_envoi_le ? formatHorodatage(etat.dernier_envoi_le) : "pas encore"}</dd>
        <dt className="text-gris">En attente</dt>
        <dd>{etat.en_attente === 0 ? "rien, tout est parti" : pluriel(etat.en_attente, "changement")}</dd>
      </dl>
      {etat.derniere_erreur ? (
        <Alerte type="attention" titre="Le dernier envoi n'a pas pu se faire">
          <p>{etat.derniere_erreur}</p>
          <p className="mt-1">
            Ce n'est pas grave : rien n'est perdu, et Tanana réessaie toute seule. L'hôtel continue de fonctionner
            normalement.
          </p>
        </Alerte>
      ) : (
        etat.dernier_envoi_le && <Alerte type="succes">Le travail à distance fonctionne.</Alerte>
      )}
      {message && <Alerte type={message.type}>{message.texte}</Alerte>}
      {(action.erreur ?? arret.erreur) && <Alerte type="erreur">{action.erreur ?? arret.erreur}</Alerte>}

      {demandeCle ? (
        <form onSubmit={afficherCle} noValidate className="grid gap-3 rounded-xl border-2 border-trait p-4">
          <ChampMotDePasse
            libelle="Par sécurité, retapez votre mot de passe"
            value={mdp}
            onChange={(e) => {
              setMdp(e.target.value);
              cle.setErreur(null);
            }}
            autoFocus
          />
          {cle.erreur && <Alerte type="erreur">{cle.erreur}</Alerte>}
          <div className="flex justify-end gap-3">
            <Bouton variante="secondaire" onClick={() => setDemandeCle(false)}>
              Annuler
            </Bouton>
            <Bouton type="submit" icone={KeyRound} enCours={cle.enCours}>
              Afficher la clé
            </Bouton>
          </div>
        </form>
      ) : (
        <div className="flex flex-wrap gap-3">
          <Bouton icone={Send} enCours={action.enCours} onClick={envoyer}>
            Envoyer maintenant
          </Bouton>
          <Bouton variante="secondaire" icone={KeyRound} onClick={() => setDemandeCle(true)}>
            Clé de la page web
          </Bouton>
          <Bouton variante="secondaire" icone={RefreshCcw} onClick={() => void toutRenvoyer()}>
            Tout renvoyer
          </Bouton>
          <Bouton variante="secondaire" icone={Unplug} enCours={arret.enCours} onClick={() => void arreter()}>
            Arrêter
          </Bouton>
        </div>
      )}
    </div>
  );
}
