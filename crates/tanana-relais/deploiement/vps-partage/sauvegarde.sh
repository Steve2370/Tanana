#!/bin/sh
# Copie de sécurité du relais Tanana, à lancer chaque nuit par cron :
#
#   crontab -e
#   30 3 * * * /home/ubuntu/tanana/crates/tanana-relais/deploiement/vps-partage/sauvegarde.sh
#
# La base est copiée « à chaud » avec la commande .backup de SQLite (copie
# cohérente même pendant un envoi). Les données sont chiffrées par les hôtels :
# la copie ne contient rien de lisible sans leurs clés. On garde 14 nuits.
set -eu

VOLUME="${TANANA_VOLUME:-tanana_donnees}"
DOSSIER="${TANANA_SAUVEGARDES:-$HOME/sauvegardes-tanana}"
GARDER_JOURS="${TANANA_GARDER_JOURS:-14}"
DOCKER="${DOCKER:-docker}"

mkdir -p "$DOSSIER"
nom="relais-$(date +%Y-%m-%d).db"

"$DOCKER" run --rm \
  -v "$VOLUME:/donnees" \
  -v "$DOSSIER:/sauvegardes" \
  alpine:3 sh -c "apk add --no-cache --quiet sqlite >/dev/null && sqlite3 /donnees/relais.db \".backup '/sauvegardes/$nom'\" && sqlite3 '/sauvegardes/$nom' 'PRAGMA integrity_check;' | grep -qx ok"

# Une copie vide ou absente est une erreur : on le dit, et on ne supprime rien.
if [ ! -s "$DOSSIER/$nom" ]; then
  echo "$(date '+%Y-%m-%d %H:%M') ÉCHEC : la copie $nom est vide ou absente" >&2
  exit 1
fi

# Les copies de plus de 14 jours partent (seulement les nôtres).
find "$DOSSIER" -maxdepth 1 -name 'relais-*.db' -type f -mtime +"$GARDER_JOURS" -delete

echo "$(date '+%Y-%m-%d %H:%M') copie $nom faite ($(du -h "$DOSSIER/$nom" | cut -f1))"
