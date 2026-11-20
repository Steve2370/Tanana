import { useEffect, useRef, useState, type FormEvent } from "react";
import { Plus, Trash2 } from "lucide-react";

import type { DetailTaxes } from "../bindings/DetailTaxes";
import type { Taxe } from "../bindings/Taxe";
import { api } from "../lib/api";
import { formatMontant, type Devise } from "../lib/format";
import { CALCULS, LIBELLE_CALCUL, libelleTaxe, lireTaxes, nouvelleCle, versSaisie, type TaxeSaisie } from "../lib/taxes";
import { Alerte, Bouton, Champ } from "./ui";

/** Liste lisible des taxes d'un séjour, avec leurs montants. */
/**
 * Taxes de plusieurs chambres additionnées ligne par ligne (même taxe = même ligne),
 * comme sur la facture d'un groupe (calcul de Rust : cumuler_taxes).
 */
export function cumulerTaxes(details: DetailTaxes[]): DetailTaxes {
  const total: DetailTaxes = { prix: 0, nuits: 0, lignes: [], total_comprises: 0, total_ajoutees: 0, hors_taxes: 0, total_a_payer: 0 };
  const meme = (a: Taxe, b: Taxe) => a.libelle === b.libelle && a.calcul === b.calcul && a.valeur === b.valeur && a.comprise === b.comprise;
  for (const d of details) {
    total.prix += d.prix;
    total.nuits += d.nuits;
    total.total_comprises += d.total_comprises;
    total.total_ajoutees += d.total_ajoutees;
    total.hors_taxes += d.hors_taxes;
    total.total_a_payer += d.total_a_payer;
    for (const l of d.lignes) {
      const deja = total.lignes.find((x) => meme(x.taxe, l.taxe));
      if (deja) deja.montant += l.montant;
      else total.lignes.push({ taxe: l.taxe, montant: l.montant });
    }
  }
  return total;
}

export function ResumeTaxes({ detail, devise }: { detail: DetailTaxes; devise: Devise }) {
  const m = (v: number) => formatMontant(v, devise);
  return (
    <ul aria-label="Taxes du séjour" className="grid gap-1.5">
      {detail.lignes.map((l, i) => (
        <li key={i} className="flex flex-wrap items-baseline justify-between gap-x-4">
          <span>
            <span className="font-semibold">{libelleTaxe(l.taxe, devise)}</span>
            <span className="text-gris"> · {l.taxe.comprise ? "comprise dans le prix" : "ajoutée au prix"}</span>
          </span>
          <span className={l.taxe.comprise ? "text-gris" : "font-semibold"}>
            {l.taxe.comprise ? "" : "+ "}
            {m(l.montant)}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** Aperçu du calcul (par Rust : une seule règle de calcul dans toute l'application). */
function Apercu({
  prix,
  nuits,
  titre,
  taxes,
  devise,
}: {
  prix: number;
  nuits: number;
  titre?: string;
  taxes: Taxe[];
  devise: Devise;
}) {
  const [detail, setDetail] = useState<DetailTaxes | null>(null);
  const cle = JSON.stringify(taxes);
  useEffect(() => {
    let annule = false;
    api.calculerTaxes(prix, nuits, JSON.parse(cle) as Taxe[]).then(
      (d) => !annule && setDetail(d),
      () => !annule && setDetail(null),
    );
    return () => {
      annule = true;
    };
  }, [prix, nuits, cle]);
  if (!detail) return null;
  const m = (v: number) => formatMontant(v, devise);
  return (
    <div role="group" aria-label="Résultat du calcul" className="grid gap-1 rounded-xl bg-sable px-4 py-3">
      {titre && <p className="mb-1 font-semibold">{titre}</p>}
      <p className="flex justify-between gap-4">
        <span>Prix du séjour</span>
        <span>{m(detail.prix)}</span>
      </p>
      {detail.lignes.map((l, i) => (
        <p key={i} className="flex justify-between gap-4 text-gris">
          <span>
            {libelleTaxe(l.taxe, devise)} · {l.taxe.comprise ? "déjà dans le prix" : "en plus"}
          </span>
          <span>
            {l.taxe.comprise ? "" : "+ "}
            {m(l.montant)}
          </span>
        </p>
      ))}
      <p className="mt-1 flex justify-between gap-4 text-lg font-bold">
        <span>Total à payer</span>
        <span>{m(detail.total_a_payer)}</span>
      </p>
    </div>
  );
}

/**
 * Éditeur de taxes : nom, façon de calculer (%, par nuit, pour le séjour),
 * montant, et « comprise dans le prix » ou « ajoutée au prix ».
 */
export function EditeurTaxes({
  titre,
  initiales,
  devise,
  apercu,
  libelleValider,
  enCours,
  erreur,
  succes,
  onValider,
  onModification,
  onFermer,
}: {
  titre: string;
  initiales: Taxe[];
  devise: Devise;
  /** Séjour sur lequel montrer le calcul en direct. */
  apercu?: { prix: number; nuits: number; titre?: string };
  libelleValider: string;
  enCours: boolean;
  erreur?: string | null;
  /** Message de réussite, affiché à côté du bouton. */
  succes?: string | null;
  onValider: (taxes: Taxe[]) => void;
  /** Appelé à chaque changement (pour effacer un ancien message de réussite). */
  onModification?: () => void;
  onFermer?: () => void;
}) {
  const [lignes, setLignes] = useState<TaxeSaisie[]>(() => initiales.map((t) => versSaisie(t, devise)));
  const [erreurLocale, setErreurLocale] = useState<string | null>(null);

  const modifie = () => {
    setErreurLocale(null);
    onModification?.();
  };
  const changer = (cle: number, modif: Partial<TaxeSaisie>) => {
    setLignes((l) => l.map((x) => (x.cle === cle ? { ...x, ...modif } : x)));
    modifie();
  };
  const ajouter = () => {
    setLignes((l) => [...l, { cle: nouvelleCle(), libelle: "", calcul: "pourcentage", saisie: "", comprise: false }]);
    modifie();
  };
  const retirer = (cle: number) => {
    setLignes((l) => l.filter((x) => x.cle !== cle));
    modifie();
  };

  const lu = lireTaxes(lignes, devise);
  const soumettre = (e: FormEvent) => {
    e.preventDefault();
    if ("erreur" in lu) return setErreurLocale(lu.erreur);
    onValider(lu.taxes);
  };
  const message = erreurLocale ?? erreur;
  // Messages et bouton ensemble, ramenés à l'écran quand un message apparaît.
  const zoneMessages = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (message || succes) zoneMessages.current?.scrollIntoView?.({ block: "nearest", behavior: "smooth" });
  }, [message, succes]);

  return (
    <form noValidate aria-label={titre} onSubmit={soumettre} className="grid gap-5">
      {lignes.length === 0 && (
        <p className="text-lg text-gris">Aucune taxe : le prix de la chambre est le prix final.</p>
      )}
      {lignes.map((l, i) => (
        <fieldset key={l.cle} className="grid gap-4 rounded-xl border-2 border-trait p-4">
          <legend className="px-2 text-lg font-bold">Taxe {i + 1}</legend>
          <div className="grid grid-cols-2 gap-4">
            <Champ
              libelle="Nom de la taxe"
              placeholder="TVA, taxe de séjour…"
              value={l.libelle}
              onChange={(e) => changer(l.cle, { libelle: e.target.value })}
            />
            <Champ
              libelle={l.calcul === "pourcentage" ? "Pourcentage (%)" : `Montant (${devise.code})`}
              inputMode="decimal"
              placeholder={l.calcul === "pourcentage" ? "Exemple : 18 ou 19,25" : "Exemple : 2000"}
              value={l.saisie}
              onChange={(e) => changer(l.cle, { saisie: e.target.value })}
            />
          </div>
          <Choix
            libelle="Comment la calculer"
            nom={`calcul-${l.cle}`}
            options={CALCULS.map((c) => ({ valeur: c, libelle: LIBELLE_CALCUL[c] }))}
            valeur={l.calcul}
            onChange={(calcul) => changer(l.cle, { calcul })}
          />
          <Choix
            libelle="Par rapport au prix de la chambre"
            nom={`comprise-${l.cle}`}
            options={[
              { valeur: "ajoutee", libelle: "Ajoutée au prix (le client paie en plus)" },
              { valeur: "comprise", libelle: "Déjà comprise dans le prix" },
            ]}
            valeur={l.comprise ? "comprise" : "ajoutee"}
            onChange={(v) => changer(l.cle, { comprise: v === "comprise" })}
          />
          <div>
            <Bouton variante="secondaire" icone={Trash2} onClick={() => retirer(l.cle)}>
              Retirer cette taxe
            </Bouton>
          </div>
        </fieldset>
      ))}
      <div>
        <Bouton variante="secondaire" icone={Plus} onClick={ajouter}>
          Ajouter une taxe
        </Bouton>
      </div>

      {apercu && "taxes" in lu && <Apercu {...apercu} taxes={lu.taxes} devise={devise} />}
      <div ref={zoneMessages} className="grid gap-4">
        {message && <Alerte type="erreur">{message}</Alerte>}
        {succes && !message && <Alerte type="succes">{succes}</Alerte>}
        <div className="flex flex-wrap justify-end gap-3">
          {onFermer && (
            <Bouton variante="secondaire" onClick={onFermer}>
              Fermer sans enregistrer
            </Bouton>
          )}
          <Bouton type="submit" enCours={enCours} grand>
            {libelleValider}
          </Bouton>
        </div>
      </div>
    </form>
  );
}

function Choix<T extends string>({
  libelle,
  nom,
  options,
  valeur,
  onChange,
}: {
  libelle: string;
  nom: string;
  options: { valeur: T; libelle: string }[];
  valeur: T;
  onChange: (v: T) => void;
}) {
  return (
    <div>
      <p className="mb-1.5 font-semibold">{libelle}</p>
      <div role="radiogroup" aria-label={libelle} className="flex flex-wrap gap-2">
        {options.map((o) => (
          <label
            key={o.valeur}
            className="flex min-h-12 cursor-pointer items-center gap-2 rounded-xl border-2 border-trait px-4 font-semibold has-[:checked]:border-marque has-[:checked]:bg-marque-clair hover:border-marque/40"
          >
            <input
              type="radio"
              name={nom}
              className="size-5 accent-marque"
              checked={valeur === o.valeur}
              onChange={() => onChange(o.valeur)}
            />
            {o.libelle}
          </label>
        ))}
      </div>
    </div>
  );
}
