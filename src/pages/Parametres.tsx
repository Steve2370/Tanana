import { useEffect, useState, type FormEvent } from "react";
import { save } from "@tauri-apps/plugin-dialog";

import { HardDriveDownload, History } from "lucide-react";

import { Alerte, Bouton, Carte, Champ, ChampMotDePasse, EnTete, useAction, ZoneTexte } from "../components/ui";
import type { Taxe } from "../bindings/Taxe";
import { CarteMisesAJour } from "../components/miseAJour";
import { RemettreCopie } from "../components/restauration";
import { EditeurTaxes } from "../components/taxes";
import { api } from "../lib/api";
import { versErreur } from "../lib/erreurs";
import { aujourdhui, formatMontant } from "../lib/format";
import { SiPermis, useDevise, useSession } from "../lib/session";
import { ReglagesDistance } from "./distance/ReglagesDistance";

export function Parametres() {
  return (
    <>
      <EnTete
        titre="Réglages"
        sousTitre="Les informations de l'hôtel, les taxes, les sauvegardes, la consultation à distance, les mises à jour et la clé de protection des données."
      />
      <div className="grid max-w-3xl gap-6">
        <SiPermis permission="parametres.gerer">
          <ParametresHotel />
        </SiPermis>
        <SiPermis permission="parametres.gerer">
          <TaxesHotel />
        </SiPermis>
        <SiPermis permission="sauvegarde.faire">
          <Sauvegarde />
        </SiPermis>
        <SiPermis permission="parametres.gerer">
          <ReglagesDistance />
        </SiPermis>
        <SiPermis permission="parametres.gerer">
          <CarteMisesAJour />
        </SiPermis>
        <SiPermis permission="parametres.gerer">
          <CleChiffrement />
        </SiPermis>
      </div>
    </>
  );
}

function ParametresHotel() {
  const { parametres, rafraichir } = useSession();
  const [nom, setNom] = useState(parametres?.nom_hotel ?? "");
  const [delai, setDelai] = useState(String(parametres?.delai_inactivite_minutes ?? 15));
  const [adresse, setAdresse] = useState(parametres?.adresse_hotel ?? "");
  const [telephone, setTelephone] = useState(parametres?.telephone_hotel ?? "");
  const [email, setEmail] = useState(parametres?.email_hotel ?? "");
  const [mentions, setMentions] = useState(parametres?.mentions_documents ?? "");
  const [ok, setOk] = useState(false);
  const { lancer, enCours, erreur, setErreur } = useAction(api.modifierParametres);

  // Un message disparaît dès que la personne modifie quelque chose.
  const modifie =
    <T,>(f: (v: T) => void) =>
    (v: T) => {
      f(v);
      setOk(false);
      setErreur(null);
    };

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    if (!parametres) return;
    setOk(false);
    const donnees = {
      ...parametres,
      nom_hotel: nom,
      delai_inactivite_minutes: Number(delai),
      adresse_hotel: adresse.trim() || null,
      telephone_hotel: telephone.trim() || null,
      email_hotel: email.trim() || null,
      mentions_documents: mentions.trim() || null,
    };
    if (await lancer(donnees)) {
      setOk(true);
      await rafraichir();
    }
  };

  return (
    <Carte titre="L'hôtel">
      <form onSubmit={soumettre} noValidate aria-label="Informations de l'hôtel" className="grid gap-4">
        <div className="grid grid-cols-2 gap-4">
          <Champ libelle="Nom de l'hôtel" value={nom} onChange={(e) => modifie(setNom)(e.target.value)} required />
          <Champ
            libelle="Déconnexion automatique après (minutes sans activité)"
            type="number"
            min={1}
            max={240}
            value={delai}
            onChange={(e) => modifie(setDelai)(e.target.value)}
          />
        </div>
        <p className="text-base text-gris">
          Monnaie : {parametres?.devise}. Elle ne se change pas, pour ne pas fausser les montants déjà enregistrés.
        </p>

        <fieldset className="mt-2 grid gap-4 border-t border-trait pt-5">
          <legend className="pr-3 text-lg font-bold">Sur les reçus et factures</legend>
          <p className="-mt-2 text-base text-gris">
            Ces informations sont imprimées en haut et en bas de chaque reçu et de chaque facture. Toutes sont facultatives.
          </p>
          <ZoneTexte
            libelle="Adresse (facultatif)"
            placeholder={"Exemple : 12 rue du Lac\nBonapriso, Douala"}
            value={adresse}
            onChange={(e) => modifie(setAdresse)(e.target.value)}
          />
          <div className="grid grid-cols-2 gap-4">
            <Champ
              libelle="Téléphone (facultatif)"
              inputMode="tel"
              value={telephone}
              onChange={(e) => modifie(setTelephone)(e.target.value)}
            />
            <Champ
              libelle="Courriel (facultatif)"
              type="email"
              autoCapitalize="off"
              spellCheck={false}
              value={email}
              onChange={(e) => modifie(setEmail)(e.target.value)}
            />
          </div>
          <ZoneTexte
            libelle="Mentions en bas de page (facultatif)"
            aide="Numéro de contribuable (NIU), RCCM, conditions… 400 caractères au plus."
            maxLength={400}
            value={mentions}
            onChange={(e) => modifie(setMentions)(e.target.value)}
          />
        </fieldset>

        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        {ok && <Alerte type="succes">C'est enregistré. Les prochains reçus et factures porteront ces informations.</Alerte>}
        <div className="flex justify-end">
          <Bouton type="submit" enCours={enCours} grand>
            Enregistrer
          </Bouton>
        </div>
      </form>
    </Carte>
  );
}

function TaxesHotel() {
  const devise = useDevise();
  const [taxes, setTaxes] = useState<Taxe[] | null>(null);
  const [erreurChargement, setErreurChargement] = useState<string | null>(null);
  const [ok, setOk] = useState(false);
  const [version, setVersion] = useState(0);
  const { lancer, enCours, erreur } = useAction(api.definirTaxesHotel);

  useEffect(() => {
    api.taxesHotel().then(setTaxes, (e) => setErreurChargement(versErreur(e).message));
  }, []);

  const enregistrer = async (nouvelles: Taxe[]) => {
    setOk(false);
    const r = await lancer(nouvelles);
    if (r) {
      setTaxes(r);
      setVersion((v) => v + 1);
      setOk(true);
    }
  };

  return (
    <Carte titre="Taxes">
      <div className="grid gap-4">
        <p className="text-base text-gris">
          TVA, taxe de séjour… Réglez ici les taxes habituelles de l'hôtel. Elles s'appliquent toutes seules aux
          nouvelles réservations. Pour un séjour précis, elles se changent sur la fiche de la réservation, avant la
          facture.
        </p>
        {erreurChargement && <Alerte type="erreur">{erreurChargement}</Alerte>}
        {taxes && (
          <EditeurTaxes
            key={version}
            titre="Taxes de l'hôtel"
            initiales={taxes}
            devise={devise}
            apercu={{
              prix: 20_000 * 10 ** devise.decimales,
              nuits: 2,
              titre: `Exemple : séjour de 2 nuits à ${formatMontant(20_000 * 10 ** devise.decimales, devise)}`,
            }}
            libelleValider="Enregistrer les taxes"
            enCours={enCours}
            erreur={erreur}
            succes={ok ? "Les taxes sont enregistrées. Elles s'appliqueront aux prochaines réservations." : null}
            onModification={() => setOk(false)}
            onValider={(t) => void enregistrer(t)}
          />
        )}
      </div>
    </Carte>
  );
}

function Sauvegarde() {
  const { peut, fermer } = useSession();
  const [remettre, setRemettre] = useState(false);
  const [fichier, setFichier] = useState<string | null>(null);
  const { lancer, enCours, erreur } = useAction(async () => {
    const chemin = await save({
      title: "Où enregistrer la copie de sécurité ?",
      defaultPath: `tanana-sauvegarde-${aujourdhui()}.db`,
      filters: [{ name: "Copie de sécurité Tanana", extensions: ["db"] }],
    });
    if (!chemin) return;
    await api.sauvegarderVers(chemin);
    setFichier(chemin);
  });

  return (
    <Carte titre="Copie de sécurité">
      <p className="mb-4 text-sm text-gris">
        Tanana fait une copie de sécurité automatique chaque jour (les 14 dernières sont gardées sur cet ordinateur).
        Une fois par semaine, faites aussi une copie sur une clé USB : c'est votre seule protection si l'ordinateur
        tombe en panne ou est volé.
      </p>
      {erreur && <Alerte type="erreur">{erreur}</Alerte>}
      {fichier && (
        <Alerte type="succes">
          Copie de sécurité enregistrée ici : <span className="selectionnable font-mono text-base break-all">{fichier}</span>
        </Alerte>
      )}
      <div className="mt-4 flex flex-wrap gap-3">
        <Bouton enCours={enCours} onClick={() => void lancer()} icone={HardDriveDownload} grand>
          Faire une copie maintenant…
        </Bouton>
        {peut("parametres.gerer") && !remettre && (
          <Bouton variante="secondaire" icone={History} onClick={() => setRemettre(true)} grand>
            Remettre une copie…
          </Bouton>
        )}
      </div>
      {remettre && (
        <section aria-label="Remettre une copie de sécurité" className="mt-6 border-t border-trait pt-5">
          <h3 className="mb-1 text-lg font-bold">Remettre une copie de sécurité</h3>
          <p className="mb-4 text-base text-gris">
            Pour revenir à un état précédent de l'hôtel (erreur grave, données abîmées). Choisissez la copie : Tanana
            montre ce qu'elle contient avant de la remettre.
          </p>
          <RemettreCopie
            hotelInstalle
            onAnnuler={() => setRemettre(false)}
            onFaite={(faite) =>
              void fermer(
                [
                  "La copie de sécurité a été remise. Reconnectez-vous pour continuer.",
                  faite.distance_arretee
                    ? "La consultation à distance a été arrêtée : réactivez-la dans Réglages et donnez la nouvelle clé au propriétaire."
                    : "",
                ]
                  .filter(Boolean)
                  .join(" "),
              )
            }
          />
        </section>
      )}
    </Carte>
  );
}

function CleChiffrement() {
  const [mdp, setMdp] = useState("");
  const [cle, setCle] = useState<string | null>(null);
  const { lancer, enCours, erreur } = useAction(api.exporterCleBase);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    const c = await lancer(mdp);
    setMdp("");
    if (c !== undefined) setCle(c ?? "Données non chiffrées (mode développement).");
  };

  return (
    <Carte titre="Clé de protection des données">
      <p className="mb-4 text-sm text-gris">
        Les données de l'hôtel sont protégées (chiffrées) par une clé gardée sur cet ordinateur. Pour utiliser une
        copie de sécurité sur un <strong>autre</strong> ordinateur, il faudra cette clé. Recopiez-la sur papier et
        gardez-la avec le code de secours. Chaque affichage est noté dans l'historique.
      </p>
      {cle ? (
        <>
          <p
            data-testid="cle-base"
            className="selectionnable rounded-xl border-2 border-dashed border-or bg-or-clair p-4 font-mono text-xl leading-relaxed tracking-wide"
          >
            {grouperCle(cle).map((ligne) => (
              <span key={ligne} className="block">
                {ligne}
              </span>
            ))}
          </p>
          <p className="mt-2 text-base text-gris">Recopiez les 16 groupes de 4 caractères : les espaces ne comptent pas.</p>
          <Bouton className="mt-3" variante="secondaire" onClick={() => setCle(null)}>
            Masquer
          </Bouton>
        </>
      ) : (
        <form onSubmit={soumettre} className="flex items-end gap-2">
          <ChampMotDePasse
            libelle="Par sécurité, retapez votre mot de passe"
            value={mdp}
            onChange={(e) => setMdp(e.target.value)}
            className="flex-1"
            required
          />
          <Bouton type="submit" variante="secondaire" enCours={enCours}>
            Afficher la clé
          </Bouton>
        </form>
      )}
      {erreur && <div className="mt-3"><Alerte type="erreur">{erreur}</Alerte></div>}
    </Carte>
  );
}

/** « 3ad442fb… » → deux lignes de 8 groupes de 4, plus faciles à recopier sur papier. */
export function grouperCle(cle: string): string[] {
  if (!/^[0-9a-f]{64}$/i.test(cle)) return [cle];
  const groupes = cle.match(/.{4}/g) ?? [cle];
  if (groupes.length <= 8) return [groupes.join(" ")];
  return [groupes.slice(0, 8).join(" "), groupes.slice(8).join(" ")];
}
