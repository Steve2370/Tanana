#!/bin/sh
# Teste sauvegarde.sh du relais avec un faux docker : copie du jour, rotation, pannes.
# Lancer : sh scripts/qa/test-sauvegarde-relais.sh
set -u
S="$(cd "$(dirname "$0")/../.." && pwd)/crates/tanana-relais/deploiement/vps-partage/sauvegarde.sh"
T=$(mktemp -d); export TANANA_SAUVEGARDES=$T/sauv
# Faux docker : vérifie les volumes, puis écrit la copie là où sqlite l'écrirait
cat > $T/docker <<'F'
#!/bin/sh
echo "$@" > "$TANANA_SAUVEGARDES/../args"
case "$*" in *"tanana_donnees:/donnees "*) ;; *) echo "volume absent" >&2; exit 3;; esac
nom=$(echo "$*" | sed -n "s/.*\/sauvegardes\/\(relais-[0-9-]*\.db\).*/\1/p" | head -1)
[ -n "${VIDE:-}" ] && { : > "$TANANA_SAUVEGARDES/$nom"; exit 0; }
[ -n "${PANNE:-}" ] && exit 1
echo "base" > "$TANANA_SAUVEGARDES/$nom"
F
chmod +x $T/docker; export DOCKER=$T/docker
ok=0; ko=0; t(){ if eval "$2"; then ok=$((ok+1)); else ko=$((ko+1)); echo "ÉCHEC : $1"; fi; }
sortie=$(sh $S 2>&1); code=$?
t "réussit" "[ $code -eq 0 ]"
t "écrit la copie du jour" "[ -s $T/sauv/relais-$(date +%Y-%m-%d).db ]"
t "le dit" "echo \"\$sortie\" | grep -q 'copie relais-'"
t "copie à chaud avec .backup puis vérification" "grep -q '.backup' $T/args && grep -q 'integrity_check' $T/args"
# Rotation : une vieille copie part, une récente reste, un autre fichier n'est pas touché
touch -d '20 days ago' $T/sauv/relais-2026-01-01.db $T/sauv/autre.txt; touch -d '3 days ago' $T/sauv/relais-2026-11-16.db
sh $S >/dev/null 2>&1
t "vieille copie supprimée" "[ ! -e $T/sauv/relais-2026-01-01.db ]"
t "copie récente gardée" "[ -e $T/sauv/relais-2026-11-16.db ]"
t "autre fichier jamais touché" "[ -e $T/sauv/autre.txt ]"
# Panne de docker : échec visible, rien de supprimé
touch -d '20 days ago' $T/sauv/relais-2026-02-01.db
PANNE=1 sh $S >/dev/null 2>&1; code=$?
t "panne : code d'erreur" "[ $code -ne 0 ]"
t "panne : rien supprimé" "[ -e $T/sauv/relais-2026-02-01.db ]"
# Copie vide : échec, rien supprimé
rm -f $T/sauv/relais-$(date +%Y-%m-%d).db
VIDE=1 sh $S >/dev/null 2>$T/err; code=$?
t "copie vide : erreur" "[ $code -ne 0 ] && grep -q 'ÉCHEC' $T/err"
t "copie vide : rien supprimé" "[ -e $T/sauv/relais-2026-02-01.db ]"
echo "$ok réussis, $ko échoués"; [ $ko -eq 0 ]
