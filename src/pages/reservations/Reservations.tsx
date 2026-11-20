import { useEffect, useRef, useState, type ReactNode } from "react";

import { aujourdhui } from "../../lib/format";
import { FicheReservation } from "./FicheReservation";
import { messageCreation, NouvelleReservation } from "./NouvelleReservation";
import { Tableau } from "./Tableau";

type Vue =
  | { type: "tableau" }
  | { type: "nouvelle"; chambreId?: string; arrivee?: string }
  | { type: "fiche"; id: string; message?: string };

/** Écran Réservations : le tableau, puis une réservation à la fois (pas de fenêtres empilées). */
export function Reservations() {
  const [vue, setVue] = useState<Vue>({ type: "tableau" });
  // Gardé ici pour retrouver le tableau au même endroit en revenant d'une fiche.
  const [debut, setDebut] = useState(aujourdhui());
  const retour = () => setVue({ type: "tableau" });

  // En changeant de vue, on repart toujours du haut de l'écran.
  const haut = useRef<HTMLDivElement>(null);
  useEffect(() => {
    haut.current?.closest("main")?.scrollTo?.({ top: 0 });
  }, [vue]);

  let contenu: ReactNode;
  if (vue.type === "nouvelle") {
    contenu = (
      <NouvelleReservation
        chambreInitiale={vue.chambreId}
        arriveeInitiale={vue.arrivee}
        onRetour={retour}
        onCree={(r, chambres) => setVue({ type: "fiche", id: r.id, message: messageCreation(r, chambres) })}
      />
    );
  } else if (vue.type === "fiche") {
    contenu = (
      <FicheReservation
        key={vue.id}
        id={vue.id}
        messageInitial={vue.message}
        onRetour={retour}
        onOuvrir={(id) => setVue({ type: "fiche", id })}
      />
    );
  } else {
    contenu = (
      <Tableau
        debut={debut}
        onDebut={setDebut}
        onNouvelle={(chambreId, arrivee) => setVue({ type: "nouvelle", chambreId, arrivee })}
        onOuvrir={(id) => setVue({ type: "fiche", id })}
      />
    );
  }
  return (
    <>
      <div ref={haut} />
      {contenu}
    </>
  );
}
