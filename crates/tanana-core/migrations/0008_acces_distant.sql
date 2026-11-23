-- Accès à distance par identifiant et mot de passe, ordinateur principal,
-- modifications à distance. Tables locales : jamais envoyées au relais.

-- Coffres d'accès des administrateurs (identifiant de l'hôtel + clé secrète,
-- chiffrés avec une clé tirée de leur mot de passe), à déposer sur le relais.
-- Les clés tirées du mot de passe sont préparées à la connexion (seul moment
-- où le mot de passe est connu) ; le coffre est fermé quand le lien avec le
-- relais existe. Elles ne donnent accès à rien que cette base ne contienne déjà.
CREATE TABLE sync_acces (
    utilisateur_id TEXT PRIMARY KEY REFERENCES utilisateurs(id),
    recherche      TEXT NOT NULL,
    sel            TEXT NOT NULL,
    verif          TEXT NOT NULL,
    cle_coffre     TEXT NOT NULL,
    -- Coffre fermé (vide tant que le lien avec le relais n'existe pas)
    nonce          TEXT NOT NULL DEFAULT '',
    chiffre        TEXT NOT NULL DEFAULT '',
    -- Empreinte du mot de passe au moment du dépôt : s'il change sans que la
    -- personne se reconnecte (réinitialisé par un autre admin), le coffre est retiré.
    mdp_hash       TEXT NOT NULL,
    a_envoyer      INTEGER NOT NULL DEFAULT 1 CHECK (a_envoyer IN (0, 1))
) STRICT;

-- Coffres à retirer du relais (compte désactivé, plus administrateur…).
CREATE TABLE sync_acces_retraits (
    recherche TEXT PRIMARY KEY
) STRICT;

-- Modifications à distance déjà exécutées (protection contre le rejeu).
CREATE TABLE sync_commandes_vues (
    id      TEXT PRIMARY KEY,
    vue_le  TEXT NOT NULL
) STRICT;

-- Activation automatique en attente (pas encore d'Internet à la création de l'hôtel).
CREATE TABLE sync_activation_auto (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    relais          TEXT NOT NULL,
    code_invitation TEXT
) STRICT;

ALTER TABLE sync_config ADD COLUMN dernier_numero INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sync_config ADD COLUMN instantane_numero INTEGER;
ALTER TABLE sync_config ADD COLUMN instantane_le TEXT;
ALTER TABLE sync_config ADD COLUMN remplace_le TEXT;
