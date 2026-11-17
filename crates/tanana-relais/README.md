# Relais de consultation à distance

Petit serveur qui permet à un administrateur, où qu'il soit, de consulter
son hôtel. L'hôtel y dépose ses changements **chiffrés de bout en bout** ;
l'administrateur les télécharge et les déchiffre avec sa clé secrète.
**Le relais ne peut rien lire** : ni noms, ni montants, ni nom d'hôtel.

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
