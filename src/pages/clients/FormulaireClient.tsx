import { useEffect, useRef, useState, type ReactNode } from "react";

import type { Client } from "../../bindings/Client";
import type { DonneesClient } from "../../bindings/DonneesClient";
import { Alerte, Bouton, Champ } from "../../components/ui";

/** Les mêmes champs servent à créer et à modifier un client. */
export function donneesDe(c: Client | null): DonneesClient {
  return {
    nom: c?.nom ?? "",
    prenom: c?.prenom ?? null,
    telephone: c?.telephone ?? null,
    email: c?.email ?? null,
    piece_identite: c?.piece_identite ?? null,
    notes: c?.notes ?? null,
  };
}

/** Champ vide → null, espaces retirés (Rust refait la même chose, par sécurité). */
function nettoyer(d: DonneesClient): DonneesClient {
  const v = (s: string | null) => (s && s.trim() ? s.trim() : null);
  return {
    nom: d.nom.trim(),
    prenom: v(d.prenom),
    telephone: v(d.telephone),
    email: v(d.email),
    piece_identite: v(d.piece_identite),
    notes: v(d.notes),
  };
}

export function FormulaireClient({
  titre,
  initial,
  libelleValider,
  enCours,
  erreur,
  avertissement,
  onValider,
  onFermer,
}: {
  titre: string;
  initial: DonneesClient;
  libelleValider: string;
  enCours: boolean;
  erreur: string | null;
  /** Message affiché au-dessus des boutons (ex. client peut-être déjà enregistré). */
  avertissement?: ReactNode;
  onValider: (d: DonneesClient) => void;
  onFermer: () => void;
}) {
  const [d, setD] = useState<DonneesClient>(initial);
  const [erreurLocale, setErreurLocale] = useState<string | null>(null);
  const champ = (cle: keyof DonneesClient) => ({
    value: d[cle] ?? "",
    onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      setD((avant) => ({ ...avant, [cle]: e.target.value }));
      setErreurLocale(null);
    },
  });

  const messageErreur = erreurLocale ?? erreur;
  const aUnAvertissement = Boolean(avertissement);
  const zoneMessages = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (messageErreur || aUnAvertissement) zoneMessages.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
  }, [messageErreur, aUnAvertissement]);

  const valider = () => {
    if (!d.nom.trim()) return setErreurLocale("Écrivez le nom du client.");
    onValider(nettoyer(d));
  };

  return (
    <form
      noValidate
      aria-label={titre}
      onSubmit={(e) => {
        e.preventDefault();
        valider();
      }}
      className="apparition grid gap-5 rounded-xl border-2 border-marque/30 bg-papier p-5"
    >
      <p className="text-xl font-bold">{titre}</p>
      <div className="grid grid-cols-2 gap-5">
        <Champ libelle="Nom" autoFocus {...champ("nom")} />
        <Champ libelle="Prénom (facultatif)" {...champ("prenom")} />
        <Champ libelle="Téléphone (facultatif)" inputMode="tel" aide="Utile pour retrouver le client." {...champ("telephone")} />
        <Champ libelle="Email (facultatif)" type="email" autoCapitalize="off" spellCheck={false} {...champ("email")} />
        <Champ libelle="Pièce d'identité (facultatif)" aide="Type et numéro, par exemple : CNI 123456789." {...champ("piece_identite")} />
      </div>
      <div>
        <label htmlFor="notes-client" className="mb-1.5 block font-semibold">
          Remarques (facultatif)
        </label>
        <textarea
          id="notes-client"
          rows={2}
          {...champ("notes")}
          placeholder="Exemple : préfère une chambre au rez-de-chaussée"
          className="w-full rounded-xl border-2 border-bordure-champ/60 bg-white px-4 py-2.5 text-lg placeholder:text-gris/70 focus:border-marque focus:ring-4 focus:ring-marque/15 focus:outline-none"
        />
      </div>
      {/* Messages et boutons ensemble : on les fait apparaître à l'écran dès qu'un message s'affiche */}
      <div ref={zoneMessages} className="grid gap-5">
        {avertissement}
        {messageErreur && <Alerte type="erreur">{messageErreur}</Alerte>}
        <div className="flex justify-end gap-3">
          <Bouton variante="secondaire" onClick={onFermer}>
            Fermer sans enregistrer
          </Bouton>
          <Bouton type="submit" enCours={enCours} grand>
            {libelleValider}
          </Bouton>
        </div>
      </div>
    </form>
  );
}
