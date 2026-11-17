import { useCallback, useEffect, useRef, useState } from "react";
import { Download, RefreshCw, Sparkles } from "lucide-react";

import type { InfosMiseAJour } from "../bindings/InfosMiseAJour";
import { api } from "../lib/api";
import { versErreur } from "../lib/erreurs";
import { chercherMiseAJour, PREMIERE_RECHERCHE_MS, RECHERCHE_MS, redemarrer, type NouvelleVersion } from "../lib/miseAJour";
import { Alerte, Bouton, Carte } from "./ui";

type Etape =
  | { nom: "proposee" }
  | { nom: "confirmation" }
  | { nom: "copie" }
  | { nom: "telechargement"; pourcent: number | null }
  | { nom: "redemarrage" }
  | { nom: "echec"; message: string; detail: string | null };

/** Nos erreurs (copie de sécurité impossible…) ont un message prêt ; celles du téléchargement sont techniques. */
function messageEchec(e: unknown): { message: string; detail: string | null } {
  if (typeof e === "object" && e !== null && "code" in e) return { message: versErreur(e).message, detail: null };
  const detail = e instanceof Error ? e.message : String(e);
  return {
    message: "Le téléchargement a été interrompu, ou le fichier reçu n'est pas authentique (il a donc été refusé).",
    detail,
  };
}

/**
 * Installe une nouvelle version : confirmation, copie de sécurité, téléchargement
 * avec progression, redémarrage. En cas d'échec, rien n'a changé.
 */
export function InstallerMiseAJour({ maj, onPlusTard }: { maj: NouvelleVersion; onPlusTard: () => void }) {
  const [etape, setEtape] = useState<Etape>({ nom: "proposee" });

  const installer = async () => {
    try {
      setEtape({ nom: "copie" });
      await api.sauvegarderAvantMiseAJour(maj.version);
      setEtape({ nom: "telechargement", pourcent: 0 });
      await maj.installer((pourcent) => setEtape({ nom: "telechargement", pourcent }));
      setEtape({ nom: "redemarrage" });
      await redemarrer();
    } catch (e) {
      setEtape({ nom: "echec", ...messageEchec(e) });
    }
  };

  switch (etape.nom) {
    case "proposee":
      return (
        <div className="grid gap-3">
          <p className="flex items-center gap-2 text-lg font-bold text-marque-fonce">
            <Sparkles className="size-5" aria-hidden />
            Une nouvelle version de Tanana est prête : {maj.version}
          </p>
          {maj.notes && <p className="text-base whitespace-pre-line text-gris">{maj.notes}</p>}
          <div className="flex flex-wrap gap-3">
            <Bouton icone={Download} onClick={() => setEtape({ nom: "confirmation" })}>
              Installer maintenant
            </Bouton>
            <Bouton variante="secondaire" onClick={onPlusTard}>
              Plus tard
            </Bouton>
          </div>
        </div>
      );
    case "confirmation":
      return (
        <div className="grid gap-3">
          <p className="text-lg">
            Tanana va faire une copie de sécurité, installer la version {maj.version}, puis <strong>redémarrer</strong>.
            Cela prend une minute ou deux. Choisissez un moment calme : personne ne doit être en train d'encaisser.
          </p>
          <div className="flex flex-wrap gap-3">
            <Bouton icone={Download} onClick={() => void installer()}>
              Oui, installer et redémarrer
            </Bouton>
            <Bouton variante="secondaire" onClick={() => setEtape({ nom: "proposee" })}>
              Pas maintenant
            </Bouton>
          </div>
        </div>
      );
    case "copie":
      return <p role="status" className="text-lg">Copie de sécurité en cours…</p>;
    case "telechargement":
      return (
        <div role="status" className="grid gap-2">
          <p className="text-lg">
            Téléchargement de la nouvelle version{etape.pourcent === null ? "…" : ` : ${etape.pourcent} %`}
          </p>
          <div className="h-3 overflow-hidden rounded-full bg-sable" aria-hidden>
            <div
              className="h-full rounded-full bg-marque transition-[width]"
              style={{ width: `${etape.pourcent ?? 30}%` }}
            />
          </div>
          <p className="text-base text-gris">Ne fermez pas Tanana.</p>
        </div>
      );
    case "redemarrage":
      return (
        <p role="status" className="text-lg">
          Installation terminée. Tanana redémarre…
        </p>
      );
    case "echec":
      return (
        <div className="grid gap-3">
          <Alerte type="attention" titre="La mise à jour n'a pas pu se faire">
            <p>{etape.message}</p>
            <p className="mt-1">Rien n'a changé : Tanana continue de fonctionner normalement. Vous pourrez réessayer plus tard.</p>
            {etape.detail && <p className="selectionnable mt-1 text-sm opacity-80">Détail pour le support : {etape.detail}</p>}
          </Alerte>
          <div className="flex flex-wrap gap-3">
            <Bouton variante="secondaire" icone={RefreshCw} onClick={() => setEtape({ nom: "confirmation" })}>
              Réessayer
            </Bouton>
            <Bouton variante="secondaire" onClick={onPlusTard}>
              Plus tard
            </Bouton>
          </div>
        </div>
      );
  }
}

/**
 * En haut de l'écran : cherche une nouvelle version peu après l'ouverture, puis
 * toutes les 6 heures. Sans Internet, ne dit rien. « Plus tard » la cache
 * jusqu'à la prochaine ouverture de Tanana.
 */
export function BandeauMiseAJour() {
  const [maj, setMaj] = useState<NouvelleVersion | null>(null);
  const [cachee, setCachee] = useState(false);

  useEffect(() => {
    let annule = false;
    let minuterie: number | undefined;
    const chercher = () =>
      chercherMiseAJour().then(
        (m) => !annule && m && setMaj(m),
        () => {}, // pas d'Internet : on réessaiera
      );
    void api.infosMiseAJour().then(
      (infos) => {
        if (annule || !infos.configuree) return;
        minuterie = window.setTimeout(() => {
          void chercher();
          minuterie = window.setInterval(() => void chercher(), RECHERCHE_MS);
        }, PREMIERE_RECHERCHE_MS);
      },
      () => {},
    );
    return () => {
      annule = true;
      window.clearTimeout(minuterie);
      window.clearInterval(minuterie);
    };
  }, []);

  if (!maj || cachee) return null;
  return (
    <div
      role="region"
      aria-label="Nouvelle version"
      className="mb-6 rounded-2xl border-2 border-or/60 bg-or-clair px-5 py-4"
    >
      <InstallerMiseAJour maj={maj} onPlusTard={() => setCachee(true)} />
    </div>
  );
}

/** Réglages › Mises à jour : version actuelle et vérification à la demande. */
export function CarteMisesAJour() {
  const [infos, setInfos] = useState<InfosMiseAJour | null>(null);
  const [maj, setMaj] = useState<NouvelleVersion | null>(null);
  const [message, setMessage] = useState<{ type: "succes" | "attention"; texte: string } | null>(null);
  const [enCours, setEnCours] = useState(false);
  const monte = useRef(true);

  useEffect(() => {
    monte.current = true;
    api.infosMiseAJour().then(setInfos, () => {});
    return () => {
      monte.current = false;
    };
  }, []);

  const verifier = useCallback(async () => {
    setEnCours(true);
    setMessage(null);
    try {
      const m = await chercherMiseAJour();
      if (!monte.current) return;
      setMaj(m);
      if (!m) setMessage({ type: "succes", texte: "Tanana est à jour." });
    } catch {
      if (monte.current)
        setMessage({
          type: "attention",
          texte: "Impossible de vérifier pour l'instant (pas d'Internet ?). Tanana réessaiera tout seul.",
        });
    } finally {
      if (monte.current) setEnCours(false);
    }
  }, []);

  if (!infos) return null;
  return (
    <Carte titre="Mises à jour">
      <div className="grid gap-4">
        <p className="text-lg">
          Version installée : <strong>{infos.version}</strong>
        </p>
        {infos.configuree ? (
          <>
            <p className="text-base text-gris">
              Tanana cherche les nouvelles versions toute seule quand Internet est là, et vous propose de les installer.
              Rien ne s'installe sans votre accord.
            </p>
            {maj ? (
              <InstallerMiseAJour maj={maj} onPlusTard={() => setMaj(null)} />
            ) : (
              <Bouton variante="secondaire" icone={RefreshCw} enCours={enCours} onClick={() => void verifier()} className="justify-self-start">
                Vérifier maintenant
              </Bouton>
            )}
            {message && <Alerte type={message.type}>{message.texte}</Alerte>}
          </>
        ) : (
          <p className="text-base text-gris">
            Version de développement : la recherche de mises à jour est active seulement dans les versions publiées.
          </p>
        )}
      </div>
    </Carte>
  );
}
