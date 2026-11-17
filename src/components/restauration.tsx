import { useEffect, useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, History, KeyRound } from "lucide-react";

import type { ApercuSauvegarde } from "../bindings/ApercuSauvegarde";
import type { RestaurationFaite } from "../bindings/RestaurationFaite";
import type { SauvegardeDisponible } from "../bindings/SauvegardeDisponible";
import { api } from "../lib/api";
import { pluriel } from "../lib/dates";
import { versErreur } from "../lib/erreurs";
import { formatHorodatage } from "../lib/format";
import { Alerte, Bouton, ChampMotDePasse, useAction } from "./ui";

/** Nom lisible d'une copie du dossier des copies de sécurité. */
export function libelleCopie(s: SauvegardeDisponible): string {
  const quand = formatHorodatage(s.date);
  if (s.nom.startsWith("tanana-avant-restauration-")) return `État d'avant une restauration, ${quand}`;
  if (s.nom.startsWith("tanana-illisible-")) return `Données illisibles mises de côté, ${quand}`;
  if (s.nom.startsWith("tanana-avant-mise-a-jour-")) return `État d'avant une mise à jour, ${quand}`;
  return `Copie automatique, ${quand}`;
}

/** La copie ne s'ouvre pas avec la clé de cet ordinateur : il faut celle de l'ancien. */
function demandeLaCle(e: unknown): boolean {
  const err = versErreur(e);
  return err.code === "VALIDATION" && /clé/.test(err.message);
}

/**
 * Remettre une copie de sécurité, en trois temps : choisir le fichier, voir ce
 * qu'il contient, confirmer. `hotelInstalle` : l'administrateur doit retaper
 * son mot de passe, et il est prévenu que l'état actuel sera remplacé.
 */
export function RemettreCopie({
  hotelInstalle,
  onFaite,
  onAnnuler,
}: {
  hotelInstalle: boolean;
  onFaite: (faite: RestaurationFaite) => void;
  onAnnuler?: () => void;
}) {
  const [copies, setCopies] = useState<SauvegardeDisponible[]>([]);
  const [chemin, setChemin] = useState<string | null>(null);
  const [besoinCle, setBesoinCle] = useState(false);
  const [cle, setCle] = useState("");
  const [apercu, setApercu] = useState<ApercuSauvegarde | null>(null);
  const [mdp, setMdp] = useState("");
  const examen = useAction(api.examinerSauvegarde);
  const remise = useAction(api.restaurerSauvegarde);

  useEffect(() => {
    api.listerSauvegardes().then(setCopies, () => setCopies([]));
  }, []);

  const examiner = async (c: string, cleTapee: string | null) => {
    setChemin(c);
    setApercu(null);
    examen.setErreur(null);
    try {
      setApercu(await api.examinerSauvegarde(c, cleTapee));
    } catch (e) {
      if (!cleTapee && demandeLaCle(e)) {
        setBesoinCle(true);
      } else {
        examen.setErreur(versErreur(e).message);
      }
    }
  };

  const choisirFichier = async () => {
    const f = await open({
      title: "Choisissez la copie de sécurité",
      multiple: false,
      directory: false,
      filters: [{ name: "Copie de sécurité Tanana", extensions: ["db"] }],
    });
    if (typeof f === "string") {
      setBesoinCle(false);
      setCle("");
      await examiner(f, null);
    }
  };

  const avecCle = (e: FormEvent) => {
    e.preventDefault();
    if (!chemin) return;
    if (cle.replace(/[\s-]/g, "").length !== 64)
      return examen.setErreur(
        `La clé compte 64 caractères. Vous en avez tapé ${cle.replace(/[\s-]/g, "").length}. Vérifiez sur le papier.`,
      );
    void examiner(chemin, cle);
  };

  const confirmer = async (e: FormEvent) => {
    e.preventDefault();
    if (!chemin) return;
    if (hotelInstalle && !mdp) return remise.setErreur("Retapez votre mot de passe pour confirmer.");
    const r = await remise.lancer(chemin, besoinCle ? cle : null, hotelInstalle ? mdp : null);
    setMdp("");
    if (r) onFaite(r);
  };

  // 3. Ce que contient la copie, puis confirmation
  if (apercu && chemin) {
    return (
      <form onSubmit={confirmer} noValidate aria-label="Remettre cette copie" className="grid gap-4">
        <div data-testid="apercu-copie" className="rounded-xl border-2 border-trait bg-sable/50 p-4">
          <p className="text-xl font-bold">{apercu.nom_hotel}</p>
          <p className="text-lg">
            {apercu.derniere_activite
              ? `L'hôtel tel qu'il était ${formatHorodatage(apercu.derniere_activite)}.`
              : "Date inconnue."}
          </p>
          <p className="text-base text-gris">
            {pluriel(apercu.reservations, "réservation")}, {pluriel(apercu.clients, "client")},{" "}
            {pluriel(apercu.comptes, "compte")} · monnaie {apercu.devise}
          </p>
        </div>
        {!apercu.journal_intact && (
          <Alerte type="attention">
            L'historique de cette copie a été modifié en dehors de Tanana. Elle reste utilisable, mais signalez-le à
            votre support.
          </Alerte>
        )}
        {hotelInstalle ? (
          <>
            <Alerte type="attention" titre="Tout ce qui a été fait depuis sera remplacé">
              L'état actuel de l'hôtel n'est pas effacé : il est gardé dans le dossier des copies de sécurité, au cas
              où. Tout le monde devra se reconnecter.
            </Alerte>
            <ChampMotDePasse
              libelle="Pour confirmer, retapez votre mot de passe"
              value={mdp}
              onChange={(e) => {
                setMdp(e.target.value);
                remise.setErreur(null);
              }}
              autoComplete="current-password"
            />
          </>
        ) : (
          <p className="text-lg">Cet ordinateur va reprendre l'hôtel tel qu'il était dans cette copie.</p>
        )}
        {remise.erreur && <Alerte type="erreur">{remise.erreur}</Alerte>}
        <div className="flex flex-wrap justify-end gap-3">
          <Bouton variante="secondaire" onClick={() => setApercu(null)}>
            Choisir une autre copie
          </Bouton>
          <Bouton type="submit" variante={hotelInstalle ? "danger" : "primaire"} icone={History} enCours={remise.enCours} grand>
            Remettre cette copie
          </Bouton>
        </div>
      </form>
    );
  }

  // 2. La copie vient d'un autre ordinateur : sa clé
  if (besoinCle) {
    return (
      <form onSubmit={avecCle} noValidate aria-label="Clé de la copie" className="grid gap-4">
        <Alerte type="info" titre="Cette copie vient d'un autre ordinateur">
          Il faut la clé de protection de cet ordinateur-là : 64 caractères, sur le papier remis à l'installation
          (Réglages › Clé de protection des données).
        </Alerte>
        <label className="font-semibold" htmlFor="cle-copie">
          Clé de protection (64 caractères)
        </label>
        <textarea
          id="cle-copie"
          rows={3}
          spellCheck={false}
          value={cle}
          onChange={(e) => {
            setCle(e.target.value);
            examen.setErreur(null);
          }}
          className="rounded-xl border-2 border-bordure-champ/60 bg-white p-4 font-mono text-lg focus:border-marque focus:ring-4 focus:ring-marque/15 focus:outline-none"
        />
        {examen.erreur && <Alerte type="erreur">{examen.erreur}</Alerte>}
        <div className="flex flex-wrap justify-end gap-3">
          <Bouton
            variante="secondaire"
            onClick={() => {
              setBesoinCle(false);
              setChemin(null);
              examen.setErreur(null);
            }}
          >
            Retour
          </Bouton>
          <Bouton type="submit" icone={KeyRound} grand>
            Ouvrir la copie
          </Bouton>
        </div>
      </form>
    );
  }

  // 1. Choisir la copie
  return (
    <div className="grid gap-4">
      {copies.length > 0 && (
        <div>
          <p className="mb-2 font-semibold">Copies de sécurité de cet ordinateur</p>
          <ul className="grid max-h-80 gap-1.5 overflow-y-auto">
            {copies.map((c) => (
              <li key={c.chemin}>
                <button
                  type="button"
                  onClick={() => void examiner(c.chemin, null)}
                  className="flex min-h-14 w-full items-center gap-3 rounded-xl border-2 border-trait px-4 text-left text-lg hover:border-marque/50 hover:bg-marque-clair/50"
                >
                  <History className="size-5 shrink-0 text-marque" aria-hidden />
                  {libelleCopie(c)}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      <p className="text-base text-gris">
        {copies.length > 0
          ? "Ou une copie faite sur une clé USB :"
          : "Branchez la clé USB (ou le disque) où se trouve la copie de sécurité, puis choisissez le fichier. Son nom commence par « tanana-sauvegarde »."}
      </p>
      <Bouton variante="secondaire" icone={FolderOpen} onClick={() => void choisirFichier()} className="justify-self-start">
        Choisir le fichier…
      </Bouton>
      {examen.erreur && <Alerte type="erreur">{examen.erreur}</Alerte>}
      {onAnnuler && (
        <div className="flex justify-end">
          <Bouton variante="secondaire" onClick={onAnnuler}>
            Annuler
          </Bouton>
        </div>
      )}
    </div>
  );
}

