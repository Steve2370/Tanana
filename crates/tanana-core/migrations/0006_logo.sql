-- Logo de l'hôtel.
--
-- Chaque image enregistrée est gardée telle quelle : un reçu ou une facture
-- déjà émis se réimprime avec le logo qu'il avait. Le paramètre `logo_hotel`
-- désigne le logo en usage (vide : aucun logo). L'image est en base64 (texte),
-- pour pouvoir partir avec la consultation à distance.
CREATE TABLE logos (
    empreinte TEXT PRIMARY KEY,
    format    TEXT NOT NULL CHECK (format IN ('png', 'jpeg')),
    donnees   TEXT NOT NULL,
    largeur   INTEGER NOT NULL CHECK (largeur > 0),
    hauteur   INTEGER NOT NULL CHECK (hauteur > 0),
    ajoute_le TEXT NOT NULL
) STRICT;

CREATE TRIGGER logos_pas_de_modification BEFORE UPDATE ON logos
BEGIN SELECT RAISE(ABORT, 'LOGO_NON_MODIFIABLE'); END;
CREATE TRIGGER logos_pas_de_suppression BEFORE DELETE ON logos
BEGIN SELECT RAISE(ABORT, 'LOGO_NON_MODIFIABLE'); END;
