# Relais sur un VPS partagé

Pour un VPS où un autre projet (ici Kadran) tient déjà les ports 80/443 avec
Caddy. Tanana garde son propre dossier ; le Caddy existant lui transmet
seulement les visiteurs de `relais.<domaine>`.

## Une seule fois, côté Caddy existant

1. Dans le Caddyfile du projet qui tient les ports, ajouter **à la fin** :
   ```
   import /etc/caddy/sites/*.caddy
   ```
2. Dans son `docker-compose.yml`, au service Caddy, ajouter le volume :
   ```yaml
   - /home/ubuntu/sites-caddy:/etc/caddy/sites:ro
   ```
3. `mkdir -p ~/sites-caddy`, reconstruire et relancer ce service.

Chaque projet dépose ensuite son fichier dans `~/sites-caddy/` : on ne retouche
plus le projet qui tient les ports.

## Tanana

```bash
git clone git@github.com:<compte>/Tanana.git ~/tanana
cd ~/tanana/crates/tanana-relais/deploiement/vps-partage
cp reglages.exemple .env && nano .env          # codes d'invitation
# Site du relais pour le Caddy existant (le fichier suivi par git reste intact)
sed "s/relais.exemple.com/relais.mondomaine.com/" relais.caddy > ~/sites-caddy/tanana.caddy
docker compose up -d --build
docker exec <conteneur caddy> caddy reload --config /etc/caddy/Caddyfile
curl https://relais.<domaine>/sante        # doit répondre : ok
```

Mettre à jour : `cd ~/tanana && git pull && cd crates/tanana-relais/deploiement/vps-partage && docker compose up -d --build`.

## Sauvegarde chaque nuit

Les données sont dans le volume Docker `tanana_donnees`. `sauvegarde.sh` en fait
une copie cohérente (commande `.backup` de SQLite), la vérifie, et garde les
14 dernières nuits dans `~/sauvegardes-tanana/`. Une seule fois :

```bash
chmod +x ~/tanana/crates/tanana-relais/deploiement/vps-partage/sauvegarde.sh
~/tanana/crates/tanana-relais/deploiement/vps-partage/sauvegarde.sh   # essai : « copie relais-AAAA-MM-JJ.db faite »
(crontab -l 2>/dev/null; echo "30 3 * * * $HOME/tanana/crates/tanana-relais/deploiement/vps-partage/sauvegarde.sh >> $HOME/sauvegardes-tanana/journal.txt 2>&1") | crontab -
```

Les copies sont chiffrées par les hôtels : sans leurs clés, elles sont illisibles.
Pour revenir à une copie : `docker compose stop`, remplacer `relais.db` dans le
volume par la copie, puis `docker compose start`.
