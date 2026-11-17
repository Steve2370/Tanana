-- Une dépense saisie par erreur ne se supprime pas : on la marque « annulée »,
-- avec la raison et l'auteur. Elle reste visible (barrée) et sort des totaux.
ALTER TABLE depenses ADD COLUMN annulee_le TEXT;
ALTER TABLE depenses ADD COLUMN annulee_par TEXT REFERENCES utilisateurs(id);
ALTER TABLE depenses ADD COLUMN motif_annulation TEXT;

-- Garde-fous : le montant, la date et la catégorie d'une dépense ne changent
-- jamais après coup, et une dépense ne se supprime pas.
CREATE TRIGGER depenses_contenu_fige
BEFORE UPDATE OF id, categorie, montant, description, date, utilisateur_id, cree_le ON depenses
BEGIN SELECT RAISE(ABORT, 'DEPENSE_NON_MODIFIABLE'); END;

CREATE TRIGGER depenses_annulation_definitive
BEFORE UPDATE OF annulee_le, annulee_par, motif_annulation ON depenses
WHEN OLD.annulee_le IS NOT NULL
BEGIN SELECT RAISE(ABORT, 'DEPENSE_NON_MODIFIABLE'); END;

CREATE TRIGGER depenses_pas_de_suppression BEFORE DELETE ON depenses
BEGIN SELECT RAISE(ABORT, 'DEPENSE_NON_MODIFIABLE'); END;
