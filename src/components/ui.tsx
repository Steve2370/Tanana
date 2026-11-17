/**
 * Composants de base. Pensés pour des personnes qui voient ou manipulent moins
 * bien : grandes zones cliquables (au moins 48 px), texte jamais sous 16 px,
 * libellés toujours écrits (pas d'icône seule), contrastes élevés.
 */
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ComponentProps,
  type InputHTMLAttributes,
  type TextareaHTMLAttributes,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { CircleAlert, CircleCheck, Eye, EyeOff, Info, TriangleAlert, type LucideIcon } from "lucide-react";

import { versErreur } from "../lib/erreurs";

// ─────────────────────────────── Boutons ───────────────────────────────

type Variante = "primaire" | "secondaire" | "danger" | "discret";

const STYLES_BOUTON: Record<Variante, string> = {
  primaire: "bg-marque text-white shadow-sm hover:bg-marque-fonce disabled:bg-marque/45",
  secondaire: "bg-white text-encre border-2 border-trait hover:border-marque/40 hover:bg-marque-clair/50 disabled:text-gris/60",
  danger: "bg-danger text-white shadow-sm hover:bg-danger/90 disabled:bg-danger/45",
  discret: "text-marque underline-offset-4 hover:bg-marque-clair hover:underline disabled:text-gris/60",
};

export function Bouton({
  variante = "primaire",
  enCours = false,
  icone: Icone,
  grand = false,
  className = "",
  children,
  disabled,
  ...props
}: ComponentProps<"button"> & {
  variante?: Variante;
  enCours?: boolean;
  icone?: LucideIcon;
  grand?: boolean;
}) {
  return (
    <button
      type="button"
      {...props}
      disabled={disabled || enCours}
      aria-busy={enCours || undefined}
      className={`inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-xl font-semibold transition-colors duration-150 disabled:cursor-not-allowed ${
        grand ? "min-h-14 px-6 text-lg" : "min-h-12 px-5 text-base"
      } ${STYLES_BOUTON[variante]} ${className}`}
    >
      {enCours ? (
        <span className="size-5 animate-spin rounded-full border-[3px] border-current border-t-transparent" aria-hidden />
      ) : (
        Icone && <Icone className="size-5 shrink-0" aria-hidden strokeWidth={2.25} />
      )}
      {children}
    </button>
  );
}

// ──────────────────────────────── Champs ────────────────────────────────

const STYLE_CHAMP =
  "w-full min-h-12 rounded-xl border-2 border-bordure-champ/60 bg-white px-4 py-2.5 text-lg text-encre placeholder:text-gris/70 transition-colors focus:border-marque focus:outline-none focus:ring-4 focus:ring-marque/15 aria-invalid:border-danger";

export function Champ({
  libelle,
  aide,
  erreur,
  className = "",
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { libelle: string; aide?: ReactNode; erreur?: string | null }) {
  const id = useId();
  return (
    <div className={className}>
      <label htmlFor={id} className="mb-1.5 block font-semibold">
        {libelle}
      </label>
      <input
        id={id}
        aria-invalid={erreur ? true : undefined}
        aria-describedby={aide || erreur ? `${id}-aide` : undefined}
        {...props}
        className={STYLE_CHAMP}
      />
      {(erreur || aide) && (
        <p id={`${id}-aide`} className={`mt-1.5 text-base ${erreur ? "font-medium text-danger" : "text-gris"}`}>
          {erreur ?? aide}
        </p>
      )}
    </div>
  );
}

/** Texte sur plusieurs lignes (adresse, remarques…). */
export function ZoneTexte({
  libelle,
  aide,
  className = "",
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { libelle: string; aide?: ReactNode }) {
  const id = useId();
  return (
    <div className={className}>
      <label htmlFor={id} className="mb-1.5 block font-semibold">
        {libelle}
      </label>
      <textarea id={id} rows={2} aria-describedby={aide ? `${id}-aide` : undefined} {...props} className={STYLE_CHAMP} />
      {aide && (
        <p id={`${id}-aide`} className="mt-1.5 text-base text-gris">
          {aide}
        </p>
      )}
    </div>
  );
}

/** Détecte la touche Verr. Maj. : cause n° 1 des « mot de passe incorrect ». */
function useVerrMaj() {
  const [actif, setActif] = useState(false);
  const surTouche = (e: React.KeyboardEvent) => {
    if (typeof e.getModifierState === "function") setActif(e.getModifierState("CapsLock"));
  };
  return { actif, surTouche };
}

/**
 * Mot de passe avec bouton « Afficher » et alerte Verr. Maj.
 * Le texte affiché reste local à l'écran : rien n'est enregistré.
 */
export function ChampMotDePasse({
  libelle,
  aide,
  className = "",
  ...props
}: Omit<InputHTMLAttributes<HTMLInputElement>, "type"> & { libelle: string; aide?: ReactNode }) {
  const id = useId();
  const [visible, setVisible] = useState(false);
  const verrMaj = useVerrMaj();
  return (
    <div className={className}>
      <label htmlFor={id} className="mb-1.5 block font-semibold">
        {libelle}
      </label>
      <div className="relative">
        <input
          id={id}
          type={visible ? "text" : "password"}
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          aria-describedby={`${id}-aide`}
          onKeyDown={verrMaj.surTouche}
          onKeyUp={verrMaj.surTouche}
          {...props}
          className={`${STYLE_CHAMP} pr-32`}
        />
        <button
          type="button"
          onClick={() => setVisible((v) => !v)}
          aria-controls={id}
          aria-pressed={visible}
          className="absolute inset-y-1.5 right-1.5 inline-flex items-center gap-1.5 rounded-lg px-3 text-base font-medium text-marque hover:bg-marque-clair"
        >
          {visible ? <EyeOff className="size-5" aria-hidden /> : <Eye className="size-5" aria-hidden />}
          {visible ? "Masquer" : "Afficher"}
        </button>
      </div>
      <div id={`${id}-aide`}>
        {verrMaj.actif && (
          <p role="status" className="mt-1.5 flex items-center gap-1.5 text-base font-semibold text-alerte">
            <TriangleAlert className="size-5" aria-hidden /> La touche Majuscule (Verr. Maj.) est activée.
          </p>
        )}
        {aide && <div className="mt-1.5 text-base text-gris">{aide}</div>}
      </div>
    </div>
  );
}

/** Règles du mot de passe cochées au fur et à mesure de la saisie. */
export const REGLES_MOT_DE_PASSE: { libelle: string; test: (m: string) => boolean }[] = [
  { libelle: "Au moins 8 caractères", test: (m) => [...m].length >= 8 },
  { libelle: "Au moins une lettre", test: (m) => /\p{L}/u.test(m) },
  { libelle: "Au moins un chiffre", test: (m) => /\d/.test(m) },
];

export const motDePasseValide = (m: string) => REGLES_MOT_DE_PASSE.every((r) => r.test(m));

export function ReglesMotDePasse({ motDePasse }: { motDePasse: string }) {
  return (
    <ul aria-label="Règles du mot de passe" className="grid gap-0.5">
      {REGLES_MOT_DE_PASSE.map((r) => {
        const ok = r.test(motDePasse);
        return (
          <li key={r.libelle} className={`flex items-center gap-2 ${ok ? "text-succes" : "text-gris"}`}>
            {ok ? <CircleCheck className="size-5" aria-hidden /> : <span className="mx-[3px] size-3.5 rounded-full border-2 border-current" aria-hidden />}
            <span>
              {r.libelle}
              <span className="sr-only">{ok ? " : respecté" : " : pas encore"}</span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}

export function Selection({
  libelle,
  valeur,
  onChange,
  options,
  aide,
  className = "",
}: {
  libelle: string;
  valeur: string;
  onChange: (v: string) => void;
  options: { valeur: string; libelle: string }[];
  aide?: ReactNode;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={className}>
      <label htmlFor={id} className="mb-1.5 block font-semibold">
        {libelle}
      </label>
      <select id={id} value={valeur} onChange={(e) => onChange(e.target.value)} className={STYLE_CHAMP}>
        {options.map((o) => (
          <option key={o.valeur} value={o.valeur}>
            {o.libelle}
          </option>
        ))}
      </select>
      {aide && <p className="mt-1.5 text-base text-gris">{aide}</p>}
    </div>
  );
}

// ─────────────────────────────── Messages ───────────────────────────────

const ALERTES = {
  info: { style: "border-marque/25 bg-marque-clair text-marque-fonce", icone: Info },
  succes: { style: "border-succes/30 bg-succes-clair text-succes", icone: CircleCheck },
  attention: { style: "border-or/60 bg-or-clair text-alerte", icone: TriangleAlert },
  erreur: { style: "border-danger/30 bg-danger-clair text-danger", icone: CircleAlert },
} as const;

/** Message toujours accompagné d'une icône ET d'un texte ; il ne disparaît pas tout seul. */
export function Alerte({
  type = "info",
  titre,
  children,
}: {
  type?: keyof typeof ALERTES;
  titre?: string;
  children?: ReactNode;
}) {
  const { style, icone: Icone } = ALERTES[type];
  return (
    <div
      role={type === "erreur" ? "alert" : "status"}
      className={`apparition flex gap-3 rounded-xl border-2 px-4 py-3 text-base ${style}`}
    >
      <Icone className="mt-0.5 size-6 shrink-0" aria-hidden />
      <div>
        {titre && <p className="font-bold">{titre}</p>}
        {children}
      </div>
    </div>
  );
}

export function Carte({
  titre,
  actions,
  children,
  className = "",
}: {
  titre?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`rounded-2xl border border-trait bg-white shadow-sm ${className}`}>
      {(titre || actions) && (
        <header className="flex items-center justify-between gap-4 border-b border-trait px-6 py-4">
          <h2 className="text-xl font-bold">{titre}</h2>
          {actions}
        </header>
      )}
      <div className="p-6">{children}</div>
    </section>
  );
}

export function EnTete({ titre, sousTitre, actions }: { titre: string; sousTitre?: string; actions?: ReactNode }) {
  return (
    <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0 flex-1 basis-96">
        <h1 className="text-3xl font-bold tracking-tight text-marque-fonce">{titre}</h1>
        {sousTitre && <p className="mt-1.5 max-w-3xl text-lg text-gris">{sousTitre}</p>}
      </div>
      {actions}
    </div>
  );
}

export function Pastille({
  ton = "neutre",
  children,
}: {
  ton?: "neutre" | "succes" | "alerte" | "danger";
  children: ReactNode;
}) {
  const styles = {
    neutre: "bg-sable text-gris",
    succes: "bg-succes-clair text-succes",
    alerte: "bg-or-clair text-alerte",
    danger: "bg-danger-clair text-danger",
  }[ton];
  return <span className={`inline-flex items-center rounded-full px-3 py-0.5 text-sm font-bold ${styles}`}>{children}</span>;
}

// ──────────────────────────── Confirmation ────────────────────────────

/**
 * Fenêtre « Êtes-vous sûr ? » pour les actions importantes. Le bouton
 * « Annuler » a le focus par défaut : appuyer sur Entrée par réflexe ne
 * déclenche jamais l'action. Échap ferme la fenêtre.
 */
export function Dialogue({
  titre,
  children,
  libelleConfirmer,
  libelleAnnuler = "Annuler",
  danger = false,
  enCours = false,
  onConfirmer,
  onAnnuler,
  focusSur = "annuler",
  onEchap,
}: {
  titre: string;
  children: ReactNode;
  libelleConfirmer: string;
  libelleAnnuler?: string;
  danger?: boolean;
  enCours?: boolean;
  onConfirmer: () => void;
  onAnnuler: () => void;
  /** Bouton qui reçoit le focus (et donc la touche Entrée) à l'ouverture. */
  focusSur?: "annuler" | "confirmer";
  /** Action de la touche Échap ; par défaut, comme « Annuler ». */
  onEchap?: () => void;
}) {
  const idTitre = useId();
  const annuler = useRef<HTMLButtonElement>(null);
  const confirmer = useRef<HTMLButtonElement>(null);
  const boite = useRef<HTMLDivElement>(null);
  const echap = onEchap ?? onAnnuler;
  // Élément qui avait le focus avant l'ouverture (lu au premier rendu, avant
  // que la fenêtre ne prenne le focus) : il le retrouve à la fermeture, pour
  // que la personne au clavier ne se retrouve pas en haut de la page.
  const [avant] = useState(() => (document.activeElement instanceof HTMLElement ? document.activeElement : null));

  useEffect(() => {
    (focusSur === "confirmer" ? confirmer : annuler).current?.focus();
  }, [focusSur]);

  useEffect(
    () => () => {
      if (avant && avant !== document.body && avant.isConnected) avant.focus();
    },
    [avant],
  );

  useEffect(() => {
    const surTouche = (e: KeyboardEvent) => {
      if (e.key === "Escape") echap();
      // Garde le focus dans la fenêtre
      if (e.key === "Tab" && boite.current) {
        const elements = boite.current.querySelectorAll<HTMLElement>("button:not([disabled])");
        const premier = elements[0];
        const dernier = elements[elements.length - 1];
        if (e.shiftKey && document.activeElement === premier) {
          e.preventDefault();
          dernier?.focus();
        } else if (!e.shiftKey && document.activeElement === dernier) {
          e.preventDefault();
          premier?.focus();
        }
      }
    };
    document.addEventListener("keydown", surTouche);
    return () => document.removeEventListener("keydown", surTouche);
  }, [echap]);

  // Rendue directement dans <body> : couvre toujours toute la fenêtre,
  // quel que soit l'endroit de la page qui l'ouvre.
  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-encre/50 p-6">
      <div
        ref={boite}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={idTitre}
        className="apparition w-full max-w-lg rounded-2xl bg-white p-7 shadow-2xl"
      >
        <h2 id={idTitre} className="mb-3 text-2xl font-bold">
          {titre}
        </h2>
        <div className="text-lg text-gris">{children}</div>
        <div className="mt-7 flex justify-end gap-3">
          <Bouton ref={annuler} variante="secondaire" onClick={onAnnuler} grand>
            {libelleAnnuler}
          </Bouton>
          <Bouton ref={confirmer} variante={danger ? "danger" : "primaire"} onClick={onConfirmer} enCours={enCours} grand>
            {libelleConfirmer}
          </Bouton>
        </div>
      </div>
    </div>,
    document.body,
  );
}

export interface DemandeConfirmation {
  titre: string;
  message: ReactNode;
  libelleConfirmer: string;
  danger?: boolean;
}

/** `const { confirmer, fenetre } = useConfirmation()` puis `if (await confirmer({...}))`. */
export function useConfirmation() {
  const [demande, setDemande] = useState<(DemandeConfirmation & { repondre: (ok: boolean) => void }) | null>(null);

  const confirmer = useCallback(
    (d: DemandeConfirmation) => new Promise<boolean>((repondre) => setDemande({ ...d, repondre })),
    [],
  );

  const fermer = useCallback(
    (ok: boolean) => {
      demande?.repondre(ok);
      setDemande(null);
    },
    [demande],
  );

  const fenetre = demande ? (
    <Dialogue
      titre={demande.titre}
      libelleConfirmer={demande.libelleConfirmer}
      danger={demande.danger}
      onConfirmer={() => fermer(true)}
      onAnnuler={() => fermer(false)}
    >
      {demande.message}
    </Dialogue>
  ) : null;

  return { confirmer, fenetre };
}

// ───────────────────────────── Clavier PIN ─────────────────────────────

/** Pavé numérique à l'écran : grandes touches, plus simple qu'un petit champ. */
export function ClavierPin({
  valeur,
  onChange,
  longueurMax = 6,
}: {
  valeur: string;
  onChange: (v: string) => void;
  longueurMax?: number;
}) {
  const ajouter = (c: string) => valeur.length < longueurMax && onChange(valeur + c);
  const touche =
    "min-h-16 rounded-xl border-2 border-trait bg-white text-2xl font-bold text-encre transition-colors hover:border-marque/40 hover:bg-marque-clair active:bg-marque-clair";
  return (
    <div>
      <div className="mb-4 flex justify-center gap-3" aria-live="polite" aria-label={`${valeur.length} chiffres saisis`}>
        {Array.from({ length: Math.max(4, valeur.length) }, (_, i) => (
          <span
            key={i}
            className={`size-5 rounded-full border-2 ${i < valeur.length ? "border-marque bg-marque" : "border-bordure-champ/60"}`}
            aria-hidden
          />
        ))}
      </div>
      <div className="grid grid-cols-3 gap-3" role="group" aria-label="Clavier du code PIN">
        {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((c) => (
          <button key={c} type="button" className={touche} onClick={() => ajouter(c)}>
            {c}
          </button>
        ))}
        <button type="button" className={`${touche} text-lg`} onClick={() => onChange("")} disabled={!valeur}>
          Effacer
        </button>
        <button type="button" className={touche} onClick={() => ajouter("0")}>
          0
        </button>
        <button type="button" className={`${touche} text-lg`} onClick={() => onChange(valeur.slice(0, -1))} disabled={!valeur}>
          ← Retour
        </button>
      </div>
    </div>
  );
}

/**
 * Dans un champ de texte, la touche Entrée valide (comme dans un formulaire classique).
 * Comme pour un bouton désactivé, rien ne se passe pendant un envoi (un bouton du
 * groupe est « en cours ») ni quand la touche reste enfoncée : une personne qui
 * appuie plusieurs fois, faute de voir l'écran changer, n'enregistre qu'une fois.
 */
export function entreePourValider(valider: () => void) {
  return (e: React.KeyboardEvent) => {
    const cible = e.target;
    if (e.key === "Enter" && cible instanceof HTMLInputElement && cible.type !== "radio" && cible.type !== "checkbox") {
      e.preventDefault();
      if (e.repeat || e.currentTarget.querySelector('[aria-busy="true"]')) return;
      valider();
    }
  };
}

// ─────────────────────────────── Actions ───────────────────────────────

/** Lance une action asynchrone en suivant « en cours » et le message d'erreur. */
export function useAction<A extends unknown[], R>(f: (...args: A) => Promise<R>) {
  const [enCours, setEnCours] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const lancer = useCallback(
    async (...args: A): Promise<R | undefined> => {
      setEnCours(true);
      setErreur(null);
      try {
        return await f(...args);
      } catch (e) {
        setErreur(versErreur(e).message);
        return undefined;
      } finally {
        setEnCours(false);
      }
    },
    [f],
  );
  return { lancer, enCours, erreur, setErreur };
}
