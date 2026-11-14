-- Notes retired. Columns remain only for decoding legacy archives and older companion payloads.
DROP TRIGGER IF EXISTS note_identity_categories_insert;
DROP TRIGGER IF EXISTS note_identity_categories_update;
DROP TRIGGER IF EXISTS note_identity_category_groups_insert;
DROP TRIGGER IF EXISTS note_identity_category_groups_update;
UPDATE categories SET notes='' WHERE notes<>'';
CREATE TRIGGER retired_categories_notes_insert AFTER INSERT ON categories
WHEN NEW.notes<>'' BEGIN UPDATE categories SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
CREATE TRIGGER retired_categories_notes_update AFTER UPDATE OF notes ON categories
WHEN NEW.notes<>'' BEGIN UPDATE categories SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
UPDATE category_groups SET notes='' WHERE notes<>'';
CREATE TRIGGER retired_category_groups_notes_insert AFTER INSERT ON category_groups
WHEN NEW.notes<>'' BEGIN UPDATE category_groups SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
CREATE TRIGGER retired_category_groups_notes_update AFTER UPDATE OF notes ON category_groups
WHEN NEW.notes<>'' BEGIN UPDATE category_groups SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
UPDATE transactions SET notes='' WHERE notes<>'';
CREATE TRIGGER retired_transactions_notes_insert AFTER INSERT ON transactions
WHEN NEW.notes<>'' BEGIN UPDATE transactions SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
CREATE TRIGGER retired_transactions_notes_update AFTER UPDATE OF notes ON transactions
WHEN NEW.notes<>'' BEGIN UPDATE transactions SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
UPDATE budget_months SET notes='' WHERE notes<>'';
CREATE TRIGGER retired_budget_months_notes_insert AFTER INSERT ON budget_months
WHEN NEW.notes<>'' BEGIN UPDATE budget_months SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
CREATE TRIGGER retired_budget_months_notes_update AFTER UPDATE OF notes ON budget_months
WHEN NEW.notes<>'' BEGIN UPDATE budget_months SET notes='' WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
UPDATE notes SET note='',tombstone=1 WHERE id=budget_id OR (id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' AND substr(id,8,4) BETWEEN '0001' AND '9999' AND substr(id,13,2) BETWEEN '01' AND '12') OR EXISTS(SELECT 1 FROM categories c WHERE c.budget_id=notes.budget_id AND c.id=notes.id) OR EXISTS(SELECT 1 FROM category_groups g WHERE g.budget_id=notes.budget_id AND g.id=notes.id);
CREATE TRIGGER retired_domain_notes_insert AFTER INSERT ON notes WHEN (NEW.note<>'' OR NEW.tombstone=0) AND (NEW.id=NEW.budget_id OR (NEW.id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' AND substr(NEW.id,8,4) BETWEEN '0001' AND '9999' AND substr(NEW.id,13,2) BETWEEN '01' AND '12') OR EXISTS(SELECT 1 FROM categories c WHERE c.budget_id=NEW.budget_id AND c.id=NEW.id) OR EXISTS(SELECT 1 FROM category_groups g WHERE g.budget_id=NEW.budget_id AND g.id=NEW.id)) BEGIN UPDATE notes SET note='',tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
CREATE TRIGGER retired_domain_notes_update AFTER UPDATE ON notes WHEN (NEW.note<>'' OR NEW.tombstone=0) AND (NEW.id=NEW.budget_id OR (NEW.id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' AND substr(NEW.id,8,4) BETWEEN '0001' AND '9999' AND substr(NEW.id,13,2) BETWEEN '01' AND '12') OR EXISTS(SELECT 1 FROM categories c WHERE c.budget_id=NEW.budget_id AND c.id=NEW.id) OR EXISTS(SELECT 1 FROM category_groups g WHERE g.budget_id=NEW.budget_id AND g.id=NEW.id)) BEGIN UPDATE notes SET note='',tombstone=1 WHERE budget_id=NEW.budget_id AND id=NEW.id; END;
