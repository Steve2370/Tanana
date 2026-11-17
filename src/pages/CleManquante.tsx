import { useState, type FormEvent } from "react";
import { History, LockOpen } from "lucide-react";

import { EcranCentre } from "../App";
import { RemettreCopie } from "../components/restauration";
import { Alerte, Bouton, Carte, useAction } from "../components/ui";
import { api } from "../lib/api";

/** La base existe mais ce poste n'a pas sa clé (nouvel ordinateur, restauration). */
export function CleManquante({ onOuverte }: { onOuverte: () => void }) {
  const [cle, setCle] = useState("");
  const [restauration, setRestauration] = useState(false);
  const { lancer, enCours, erreur } = useAction(api.fournirCle);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    if ((await lancer(cle)) !== undefined) onOuverte();
  };

  if (restauration) {
    return (
      <EcranCentre large>
        <Carte titre="Remettre une copie de sécurité">
          <p className="mb-5 text-lg">
            Les données actuelles ne sont pas effacées : elles sont mises de côté dans le dossier des copies de sécurité.
          </p>
          <RemettreCopie hotelInstalle={false} onAnnuler={() => setRestauration(false)} onFaite={onOuverte} />
        </Carte>
      </EcranCentre>
    );
  }

  return (
    <EcranCentre large>
      <Carte titre="Clé de protection des données">
        <div className="mb-5 grid gap-2 text-lg text-gris">
          <p>Les données de l'hôtel sont protégées par une clé. Cet ordinateur ne la connaît pas encore.</p>
          <p>
            Vous la trouverez sur le papier remis à l'installation, ou un administrateur peut l'afficher sur l'ancien
            ordinateur dans <strong className="text-encre">Réglages → Clé de protection</strong>.
          </p>
        </div>
        <form onSubmit={soumettre} className="grid gap-4">
          <label className="font-semibold" htmlFor="cle">
            Clé (64 caractères)
          </label>
          <textarea
            id="cle"
            value={cle}
            onChange={(e) => setCle(e.target.value)}
            rows={3}
            spellCheck={false}
            className="rounded-xl border-2 border-bordure-champ/60 bg-white p-4 font-mono text-lg focus:border-marque focus:outline-none focus:ring-4 focus:ring-marque/15"
          />
          {erreur && <Alerte type="erreur">{erreur}</Alerte>}
          <Bouton type="submit" enCours={enCours} disabled={cle.trim().length === 0} icone={LockOpen} grand>
            Ouvrir les données
          </Bouton>
        </form>
        <div className="mt-6 flex flex-wrap items-center justify-between gap-3 border-t border-trait pt-4">
          <p className="min-w-0 flex-1 basis-64 text-base text-gris">Plus de clé, ou des données abîmées ?</p>
          <Bouton variante="secondaire" icone={History} onClick={() => setRestauration(true)}>
            Remettre une copie de sécurité
          </Bouton>
        </div>
      </Carte>
    </EcranCentre>
  );
}
