/**
 * Le curseur toujours prêt dans les champs : quand un écran ou un formulaire
 * s'ouvre, le premier champ de saisie reçoit le curseur, sans avoir à cliquer.
 *
 * Règles, pour ne jamais gêner :
 *  - on ne prend jamais la main à quelqu'un qui est déjà dans un champ ;
 *  - un formulaire qui apparaît (« Nouveau client », une fenêtre de saisie…)
 *    reçoit le curseur si la personne n'est pas en train de taper ailleurs ;
 *  - à l'ouverture d'un écran, seulement si le champ est visible sans faire
 *    défiler la page (pas de saut vers le bas de l'écran).
 */

/** Champs où l'on tape du texte (pas les cases à cocher, listes, dates à boutons…). */
const CHAMPS =
  'input:not([type="hidden"]):not([type="checkbox"]):not([type="radio"]):not([type="file"]):not([type="button"]):not([type="submit"]):not([type="range"]):not([type="color"]), textarea';

/** Ce qui « ouvre » une saisie quand il apparaît. */
const CONTENEURS = 'form, [role="dialog"], dialog, [role="group"][aria-label]';

/** L'élément sert-il à saisir ? (texte, liste déroulante, zone éditable) */
export function estSaisie(el: Element | null): boolean {
  if (!el || !(el instanceof HTMLElement)) return false;
  if (el.isContentEditable) return true;
  if (el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) return true;
  return el instanceof HTMLInputElement && el.matches(CHAMPS);
}

function cache(el: HTMLElement): boolean {
  for (let n: HTMLElement | null = el; n && n !== document.body; n = n.parentElement) {
    if (n.hidden || n.getAttribute("aria-hidden") === "true" || n.hasAttribute("inert")) return true;
    const style = getComputedStyle(n);
    if (style.display === "none" || style.visibility === "hidden") return true;
  }
  return false;
}

/** Visible dans la fenêtre sans défiler. (Taille nulle : environnement de test, on accepte.) */
function dansLaFenetre(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return true;
  return r.bottom > 0 && r.top < window.innerHeight && r.right > 0 && r.left < window.innerWidth;
}

/** Premier champ de saisie utilisable dans `racine`. */
export function premierChamp(racine: ParentNode, fenetreSeulement = false): HTMLElement | null {
  for (const el of racine.querySelectorAll<HTMLElement>(CHAMPS)) {
    const champ = el as HTMLInputElement | HTMLTextAreaElement;
    if (champ.disabled || champ.readOnly || cache(el)) continue;
    if (fenetreSeulement && !dansLaFenetre(el)) continue;
    // À l'ouverture d'un écran, pas de curseur dans un filtre de dates : on cherche un vrai champ à remplir
    if (fenetreSeulement && /^(date|datetime-local|time|month|week)$/.test((el as HTMLInputElement).type ?? "")) continue;
    return el;
  }
  return null;
}

/** Met le curseur dans le premier champ de `racine`, si personne n'est déjà dans un champ. */
export function placerCurseur(racine: ParentNode, fenetreSeulement = false): boolean {
  if (estSaisie(document.activeElement)) return false;
  const champ = premierChamp(racine, fenetreSeulement);
  if (!champ) return false;
  champ.focus({ preventScroll: fenetreSeulement });
  return document.activeElement === champ;
}

/** Écran qui vient de s'ouvrir : on attend un peu que ses champs arrivent (données chargées). */
let ecranAttendu: { racine: HTMLElement; jusqua: number } | null = null;

/** À appeler quand un écran s'ouvre : son premier champ visible recevra le curseur. */
export function curseurPourEcran(racine: HTMLElement, delaiMs = 2000) {
  ecranAttendu = { racine, jusqua: Date.now() + delaiMs };
  window.setTimeout(() => {
    if (ecranAttendu?.racine === racine && placerCurseur(racine, true)) ecranAttendu = null;
  }, 0);
}

let installe = false;

/**
 * Surveille l'écran : chaque formulaire qui apparaît reçoit le curseur dans son
 * premier champ, et l'écran qui vient de s'ouvrir aussi quand ses champs arrivent.
 * À appeler une fois au démarrage.
 */
export function installerCurseurAuto(): () => void {
  if (installe || typeof MutationObserver === "undefined") return () => {};
  installe = true;
  let enAttente = false;
  const ajouts: Element[] = [];

  const traiter = () => {
    enAttente = false;
    const nouveaux = ajouts.splice(0).filter((n) => n.isConnected);
    if (estSaisie(document.activeElement)) {
      ecranAttendu = null; // la personne est déjà dans un champ
      return;
    }
    // Un formulaire ou une fenêtre de saisie vient d'apparaître : le plus récent gagne
    for (let i = nouveaux.length - 1; i >= 0; i--) {
      const n = nouveaux[i]!;
      const conteneur = n.matches(CONTENEURS) ? n : n.querySelector(CONTENEURS);
      if (conteneur && placerCurseur(conteneur)) {
        ecranAttendu = null;
        return;
      }
    }
    // L'écran qui vient de s'ouvrir a peut-être reçu ses champs
    if (ecranAttendu) {
      if (Date.now() > ecranAttendu.jusqua || !ecranAttendu.racine.isConnected) ecranAttendu = null;
      else if (placerCurseur(ecranAttendu.racine, true)) ecranAttendu = null;
    }
  };

  const observateur = new MutationObserver((mutations) => {
    for (const m of mutations) for (const n of m.addedNodes) if (n instanceof Element) ajouts.push(n);
    if (ajouts.length && !enAttente) {
      enAttente = true;
      // Après React (et après le focus que l'écran a pu choisir lui-même avec autoFocus)
      window.setTimeout(traiter, 0);
    }
  });
  observateur.observe(document.body, { childList: true, subtree: true });
  return () => {
    observateur.disconnect();
    installe = false;
    ecranAttendu = null;
  };
}
