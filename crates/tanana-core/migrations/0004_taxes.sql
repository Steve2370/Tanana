-- Taxes (TVA, taxe de séjour…), réglables par chaque hôtel dans sa devise.
--
-- taxes_hotel : les taxes habituelles, réglées par l'administrateur.
--   Elles sont copiées sur chaque nouvelle réservation.
-- taxes_reservation : les taxes d'un séjour précis, modifiables avant la facture.
--
-- calcul : 'pourcentage' (valeur en centièmes : 1925 = 19,25 %),
--          'par_nuit' ou 'par_sejour' (valeur dans la plus petite unité de la devise).
-- comprise : 1 = déjà dans le prix de la chambre ; 0 = s'ajoute au prix.
CREATE TABLE taxes_hotel (
    ordre    INTEGER PRIMARY KEY,
    libelle  TEXT NOT NULL CHECK (length(trim(libelle)) > 0),
    calcul   TEXT NOT NULL CHECK (calcul IN ('pourcentage', 'par_nuit', 'par_sejour')),
    valeur   INTEGER NOT NULL CHECK (valeur > 0),
    comprise INTEGER NOT NULL CHECK (comprise IN (0, 1)),
    CHECK (calcul <> 'pourcentage' OR valeur <= 10000)
) STRICT;

CREATE TABLE taxes_reservation (
    reservation_id TEXT NOT NULL REFERENCES reservations(id),
    ordre          INTEGER NOT NULL,
    libelle        TEXT NOT NULL CHECK (length(trim(libelle)) > 0),
    calcul         TEXT NOT NULL CHECK (calcul IN ('pourcentage', 'par_nuit', 'par_sejour')),
    valeur         INTEGER NOT NULL CHECK (valeur > 0),
    comprise       INTEGER NOT NULL CHECK (comprise IN (0, 1)),
    CHECK (calcul <> 'pourcentage' OR valeur <= 10000),
    PRIMARY KEY (reservation_id, ordre)
) STRICT;
