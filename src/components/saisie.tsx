/** Saisie d'une date et d'un nombre, faites pour être faciles à utiliser et identiques sur Mac et Windows. */
import { useId } from "react";
import { Minus, Plus } from "lucide-react";

import { composer, decomposer, formatJourLong, joursDansLeMois, NOMS_DES_MOIS } from "../lib/dates";

const STYLE_SELECT =
  "min-h-12 rounded-xl border-2 border-bordure-champ/60 bg-white px-3 text-lg focus:border-marque focus:outline-none focus:ring-4 focus:ring-marque/15 disabled:bg-sable disabled:text-gris";

/**
 * Date choisie avec trois listes (jour, mois, année), puis réécrite en toutes
 * lettres pour vérification : « vendredi 13 novembre 2026 ».
 */
export function ChampDate({
  libelle,
  valeur,
  onChange,
  desactive = false,
  aide,
}: {
  libelle: string;
  valeur: string;
  onChange: (iso: string) => void;
  desactive?: boolean;
  aide?: string;
}) {
  const id = useId();
  const { annee, mois, jour } = decomposer(valeur);
  const anneeCourante = new Date().getFullYear();
  const annees = [...new Set([anneeCourante - 1, anneeCourante, anneeCourante + 1, anneeCourante + 2, annee])].sort();

  return (
    <fieldset disabled={desactive}>
      <legend className="mb-1.5 font-semibold">{libelle}</legend>
      <div className="flex flex-wrap gap-2">
        <select
          aria-label={`${libelle} : jour`}
          className={`${STYLE_SELECT} w-24`}
          value={jour}
          onChange={(e) => onChange(composer(annee, mois, Number(e.target.value)))}
        >
          {Array.from({ length: joursDansLeMois(annee, mois) }, (_, i) => i + 1).map((j) => (
            <option key={j} value={j}>
              {j}
            </option>
          ))}
        </select>
        <select
          aria-label={`${libelle} : mois`}
          className={`${STYLE_SELECT} w-44`}
          value={mois}
          onChange={(e) => onChange(composer(annee, Number(e.target.value), jour))}
        >
          {NOMS_DES_MOIS.map((nom, i) => (
            <option key={nom} value={i + 1}>
              {nom}
            </option>
          ))}
        </select>
        <select
          aria-label={`${libelle} : année`}
          className={`${STYLE_SELECT} w-32`}
          value={annee}
          onChange={(e) => onChange(composer(Number(e.target.value), mois, jour))}
        >
          {annees.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
      </div>
      <p id={`${id}-lu`} className="mt-1.5 text-base font-semibold text-marque" aria-live="polite">
        {formatJourLong(valeur)}
      </p>
      {aide && <p className="text-base text-gris">{aide}</p>}
    </fieldset>
  );
}

/** Nombre choisi avec deux gros boutons − et +. */
export function Compteur({
  libelle,
  valeur,
  onChange,
  min = 1,
  max = 365,
  unite,
}: {
  libelle: string;
  valeur: number;
  onChange: (n: number) => void;
  min?: number;
  max?: number;
  /** Mot au singulier, accordé automatiquement : « nuit » → « 3 nuits ». */
  unite: string;
}) {
  const bouton =
    "flex size-12 items-center justify-center rounded-xl border-2 border-trait bg-white text-marque hover:border-marque/40 hover:bg-marque-clair disabled:cursor-not-allowed disabled:text-gris/50";
  return (
    <div role="group" aria-label={libelle}>
      <p className="mb-1.5 font-semibold">{libelle}</p>
      <div className="flex items-center gap-3">
        <button type="button" className={bouton} onClick={() => onChange(valeur - 1)} disabled={valeur <= min} aria-label={`Une ${unite} de moins`}>
          <Minus className="size-6" aria-hidden strokeWidth={2.5} />
        </button>
        <output className="min-w-28 text-center text-2xl font-bold" aria-live="polite">
          {valeur} {unite}
          {valeur > 1 ? "s" : ""}
        </output>
        <button type="button" className={bouton} onClick={() => onChange(valeur + 1)} disabled={valeur >= max} aria-label={`Une ${unite} de plus`}>
          <Plus className="size-6" aria-hidden strokeWidth={2.5} />
        </button>
      </div>
    </div>
  );
}
