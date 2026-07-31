-- 0013_unknown_barcodes_ghost_columns.sql
-- Adds columns referenced by ghost_list and ghost_resolve queries.
-- Without these columns, ghost_list returns a SQL "no such column" error
-- whenever the ghost barcodes panel is expanded in the admin UI.
ALTER TABLE unknown_barcodes ADD COLUMN brand TEXT;
ALTER TABLE unknown_barcodes ADD COLUMN category TEXT;
ALTER TABLE unknown_barcodes ADD COLUMN image_url TEXT;
ALTER TABLE unknown_barcodes ADD COLUMN raw_json TEXT;
