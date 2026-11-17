-- Consultation à distance.
--
-- sync_boite : « boîte d'envoi » des changements, remplie par des triggers
--   (créés seulement quand la consultation à distance est activée). Chaque
--   ligne est un changement d'une table : écrire une ligne complète, ou
--   l'effacer. Les lignes sont retirées une fois reçues par le relais.
--   Les secrets (mots de passe, PIN, code de récupération, sessions) n'y
--   entrent jamais.
-- sync_config : liaison avec le relais (une seule ligne). Jamais envoyée.
CREATE TABLE sync_boite (
    seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    nom_table TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('ecrire', 'effacer')),
    cle       TEXT NOT NULL,
    donnees   TEXT
) STRICT;

CREATE TABLE sync_config (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    relais                TEXT NOT NULL,
    hotel_id              TEXT NOT NULL,
    jeton_envoi           TEXT NOT NULL,
    secret                TEXT NOT NULL,
    active_le             TEXT NOT NULL,
    dernier_envoi_le      TEXT,
    derniere_tentative_le TEXT,
    derniere_erreur       TEXT
) STRICT;
