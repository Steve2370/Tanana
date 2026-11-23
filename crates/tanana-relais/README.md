# Relais de Tanana (travail à distance)

Petit serveur qui permet à un administrateur, où qu'il soit, de suivre **et
modifier** son hôtel. La base reste sur l'ordinateur de l'hôtel (il marche sans
Internet) ; le relais ne fait que transmettre des paquets **chiffrés de bout en
bout**. **Il ne peut rien lire** : ni noms, ni montants, ni mots de passe.

- L'ordinateur de l'hôtel y dépose ses changements, et de temps en temps une
  copie complète (pour « Retrouver mon hôtel » sur un autre ordinateur).
- Chaque administrateur a un **coffre** : l'accès à l'hôtel, chiffré avec une clé
  tirée de son mot de passe. Identifiant + mot de passe suffisent, sur n'importe
  quel ordinateur. Le relais garde l'empreinte d'une preuve tirée du mot de
  passe : il refuse un mauvais mot de passe et freine les essais (10 par compte
  et 30 par adresse en 15 minutes), sans jamais connaître le mot de passe.
- Une modification faite à distance passe par le relais jusqu'à l'ordinateur
  de l'hôtel, qui l'exécute et répond. Si l'hôtel ne s'est pas manifesté depuis
  75 secondes, elle est refusée tout de suite (« hôtel injoignable »).
- Un seul ordinateur principal à la fois : quand un nouvel ordinateur reprend
  l'hôtel, l'ancien ne peut plus rien envoyer et l'apprend à son envoi suivant.

## Page web

Le relais sert aussi la page de consultation (`src/web/` du projet, construite
dans l'image Docker) : le propriétaire ouvre l'adresse du relais dans un
navigateur, tape l'identifiant de l'hôtel et la clé secrète. Le déchiffrement
se fait dans le navigateur : le relais ne voit jamais la clé.

## Ce qu'il garde

Par hôtel : un identifiant court (`HK7Q-F29X`), l'empreinte de deux jetons
(envoi pour l'hôtel, lecture pour l'administrateur), un « témoin » chiffré
pour vérifier la clé, les paquets chiffrés dans l'ordre d'arrivée, et l'heure
du dernier contact. Rien d'autre.

## Installer sur un VPS (≈ 5 €/mois)

Prérequis : un VPS Linux avec Docker, et un nom de domaine qui pointe vers lui.

```bash
git clone <dépôt> tanana && cd tanana/crates/tanana-relais/deploiement
nano Caddyfile             # remplacer relais.mondomaine.com
nano docker-compose.yml    # choisir les codes d'invitation
docker compose up -d
curl https://relais.mondomaine.com/sante   # doit répondre : ok
```

Caddy obtient et renouvelle tout seul le certificat HTTPS.

## Codes d'invitation

Pour qu'un hôtel s'inscrive, il faut un code d'invitation (variable
`TANANA_RELAIS_CODES`, plusieurs séparés par des virgules). Tu le donnes à
tes clients au moment de l'installation. Sans code valide, pas d'inscription :
personne d'autre ne peut utiliser ton relais.

## Sauvegarde

Les données sont dans le volume Docker `donnees` (fichier `relais.db`).
Elles sont chiffrées, mais une copie régulière évite de devoir faire
renvoyer tout l'historique par les hôtels en cas de panne du VPS.

## Variables

| Variable | Rôle | Défaut |
|---|---|---|
| `TANANA_RELAIS_ECOUTE` | adresse d'écoute | `127.0.0.1:8080` |
| `TANANA_RELAIS_BASE` | fichier de données | `relais.db` |
| `TANANA_RELAIS_CODES` | codes d'invitation | (aucun : inscriptions fermées) |
| `TANANA_RELAIS_HOTELS_MAX` | nombre maximal d'hôtels | `500` |
| `TANANA_RELAIS_INSCRIPTIONS_LIBRES=oui` | essai local seulement | non |
| `TANANA_RELAIS_PAGES` | dossier de la page web (`npm run build:web`) | `/srv/consultation` dans l'image Docker |

## Essai local

```bash
TANANA_RELAIS_INSCRIPTIONS_LIBRES=oui cargo run -p tanana-relais
```
