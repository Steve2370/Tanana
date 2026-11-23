import { useEffect, useState } from "react";
import { Search, UserPlus, UserRound } from "lucide-react";

import type { Client } from "../../bindings/Client";
import { Alerte, Bouton, Champ, entreePourValider, useAction } from "../../components/ui";
import { api } from "../../lib/api";
import { emailPlausible } from "../../lib/format";
import { useSession } from "../../lib/session";

export function nomComplet(c: Client): string {
  return [c.nom, c.prenom].filter(Boolean).join(" ");
}

/** Retrouver un client déjà venu, ou en créer un nouveau sans quitter la réservation. */
export function ChoixClient({ client, onChange }: { client: Client | null; onChange: (c: Client | null) => void }) {
  const { peut } = useSession();
  const [recherche, setRecherche] = useState("");
  const [resultats, setResultats] = useState<Client[] | null>(null);
  const [creation, setCreation] = useState(false);

  useEffect(() => {
    const q = recherche.trim();
    if (q.length < 2) {
      setResultats(null);
      return;
    }
    let annule = false;
    api.rechercherClients(q).then(
      (r) => !annule && setResultats(r),
      () => !annule && setResultats([]),
    );
    return () => {
      annule = true;
    };
  }, [recherche]);

  if (client) {
    return (
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border-2 border-marque bg-marque-clair px-4 py-3">
        <span className="flex items-center gap-3">
          <UserRound className="size-7 text-marque" aria-hidden />
          <span>
            <span className="block text-lg font-bold">{nomComplet(client)}</span>
            {client.telephone && <span className="block text-gris">Téléphone : {client.telephone}</span>}
            {client.email && <span className="block text-gris">Email : {client.email}</span>}
          </span>
        </span>
        <Bouton variante="secondaire" onClick={() => onChange(null)}>
          Changer de client
        </Bouton>
      </div>
    );
  }

  if (creation) {
    return (
      <NouveauClient
        nomPropose={recherche.trim()}
        onCree={(c) => {
          setCreation(false);
          onChange(c);
        }}
        onAnnuler={() => setCreation(false)}
      />
    );
  }

  return (
    <div className="grid gap-3">
      <div className="relative">
        <Champ
          libelle="Chercher un client déjà venu"
          value={recherche}
          onChange={(e) => setRecherche(e.target.value)}
          placeholder="Nom ou numéro de téléphone"
          aide="Tapez au moins 2 lettres."
          spellCheck={false}
        />
        <Search className="pointer-events-none absolute top-11 right-4 size-6 text-gris" aria-hidden />
      </div>

      {resultats && resultats.length > 0 && (
        <ul className="grid gap-2" aria-label="Clients trouvés">
          {resultats.slice(0, 8).map((c) => (
            <li key={c.id}>
              <button
                type="button"
                onClick={() => onChange(c)}
                className="flex min-h-12 w-full items-center justify-between gap-3 rounded-xl border-2 border-trait px-4 py-1.5 text-left hover:border-marque/40 hover:bg-marque-clair/50"
              >
                <span className="text-lg font-semibold">{nomComplet(c)}</span>
                <span className="text-gris">{c.telephone ?? ""}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {resultats && resultats.length > 8 && <p className="text-gris">Il y a d'autres résultats : précisez le nom.</p>}
      {resultats?.length === 0 && <Alerte type="info">Aucun client trouvé pour « {recherche.trim()} ».</Alerte>}

      {peut("client.gerer") && (
        <div>
          <Bouton variante="secondaire" icone={UserPlus} onClick={() => setCreation(true)}>
            C'est un nouveau client
          </Bouton>
        </div>
      )}
    </div>
  );
}

function NouveauClient({
  nomPropose,
  onCree,
  onAnnuler,
}: {
  nomPropose: string;
  onCree: (c: Client) => void;
  onAnnuler: () => void;
}) {
  // Si la recherche était un numéro, on le propose comme téléphone plutôt que comme nom.
  const estNumero = /^[\d\s+().-]+$/.test(nomPropose);
  const [nom, setNom] = useState(estNumero ? "" : nomPropose);
  const [prenom, setPrenom] = useState("");
  const [telephone, setTelephone] = useState(estNumero ? nomPropose : "");
  const [email, setEmail] = useState("");
  const { lancer, enCours, erreur, setErreur } = useAction(api.creerClient);

  const enregistrer = async () => {
    if (!nom.trim()) return setErreur("Écrivez le nom du client.");
    const mail = email.trim();
    if (mail && !emailPlausible(mail)) return setErreur("L'email n'est pas valide. Écrivez-le comme ceci : nom@exemple.com");
    const c = await lancer({
      nom,
      prenom: prenom || null,
      telephone: telephone || null,
      email: mail || null,
      piece_identite: null,
      notes: null,
    });
    if (c) onCree(c);
  };

  return (
    <div className="grid gap-4 rounded-xl border-2 border-trait p-4" role="group" aria-label="Nouveau client" onKeyDown={entreePourValider(() => void enregistrer())}>
      <p className="text-lg font-bold">Nouveau client</p>
      <div className="grid grid-cols-2 gap-4">
        <Champ libelle="Nom" value={nom} onChange={(e) => setNom(e.target.value)} autoFocus />
        <Champ libelle="Prénom" value={prenom} onChange={(e) => setPrenom(e.target.value)} aide="Facultatif" />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <Champ libelle="Téléphone" value={telephone} onChange={(e) => setTelephone(e.target.value)} inputMode="tel" aide="Facultatif, mais utile pour le retrouver." />
        <Champ
          libelle="Email"
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          autoCapitalize="off"
          spellCheck={false}
          aide="Facultatif."
        />
      </div>
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      <div className="flex justify-end gap-3">
        <Bouton variante="secondaire" onClick={onAnnuler}>
          Annuler
        </Bouton>
        <Bouton enCours={enCours} onClick={() => void enregistrer()}>
          Enregistrer ce client
        </Bouton>
      </div>
    </div>
  );
}
