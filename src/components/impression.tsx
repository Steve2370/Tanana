import { useState } from "react";
import { FolderOpen } from "lucide-react";

import type { DocumentEnregistre } from "../bindings/DocumentEnregistre";
import { api } from "../lib/api";
import { versErreur } from "../lib/erreurs";
import { Alerte, Bouton } from "./ui";

const NOM = { recu: "reçu", facture: "facture" } as const;

/** Message après l'impression d'un reçu ou d'une facture : quoi faire maintenant, et où le retrouver. */
export function messageDocument(d: DocumentEnregistre): { titre: string; lignes: string[] } {
  const { type_document: type, numero, remplace_numero } = d.document;
  const article = type === "recu" ? "Le reçu" : "La facture";
  const lignes: string[] = [];
  if (d.duplicata) {
    lignes.push(
      type === "recu"
        ? "Ce reçu avait déjà été imprimé : cette copie porte la mention « Duplicata »."
        : "Rien n'a changé depuis cette facture : cette copie porte la mention « Duplicata ».",
    );
  }
  if (remplace_numero && !d.duplicata) {
    lignes.push(`Elle remplace la facture ${remplace_numero}, car le séjour ou les paiements ont changé depuis.`);
  }
  if (d.ouvert) {
    lignes.push("Pour l'imprimer, appuyez sur « Imprimer » dans la fenêtre qui vient de s'ouvrir.");
    lignes.push(`Une copie est rangée dans ${d.emplacement}.`);
    return { titre: `${article} ${numero} est ouvert${type === "facture" ? "e" : ""} à l'écran.`, lignes };
  }
  lignes.push(`Le fichier n'a pas pu s'ouvrir tout seul. Vous le trouverez dans ${d.emplacement}.`);
  return { titre: `${article} ${numero} est prêt${type === "facture" ? "e" : ""}.`, lignes };
}

/** Avis affiché après une impression, avec un bouton pour ouvrir le dossier. */
export function AvisDocument({ document: d }: { document: DocumentEnregistre }) {
  const { titre, lignes } = messageDocument(d);
  const [erreur, setErreur] = useState<string | null>(null);
  return (
    <Alerte type="succes" titre={titre}>
      {lignes.map((l) => (
        <p key={l}>{l}</p>
      ))}
      <Bouton
        variante="secondaire"
        icone={FolderOpen}
        className="mt-3"
        onClick={() =>
          api.ouvrirDossierDocuments(d.document.type_document).catch((e) => setErreur(versErreur(e).message))
        }
      >
        Ouvrir le dossier des {NOM[d.document.type_document]}s
      </Bouton>
      {erreur && <p className="mt-2 font-semibold text-danger">{erreur}</p>}
    </Alerte>
  );
}

/**
 * Impression d'un reçu ou d'une facture. `enCours` donne l'identifiant en cours
 * (pour n'afficher le sablier que sur le bon bouton).
 */
export function useImpression() {
  const [enCours, setEnCours] = useState<string | null>(null);
  const [document, setDocument] = useState<DocumentEnregistre | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  const lancer = async (cle: string, action: () => Promise<DocumentEnregistre>) => {
    setEnCours(cle);
    setErreur(null);
    setDocument(null);
    try {
      const d = await action();
      setDocument(d);
      return d;
    } catch (e) {
      setErreur(versErreur(e).message);
      return null;
    } finally {
      setEnCours(null);
    }
  };

  return {
    enCours,
    document,
    erreur,
    effacer: () => {
      setDocument(null);
      setErreur(null);
    },
    imprimerRecu: (paiementId: string) => lancer(paiementId, () => api.imprimerRecu(paiementId)),
    imprimerFacture: (reservationId: string) => lancer(`facture-${reservationId}`, () => api.imprimerFacture(reservationId)),
  };
}
