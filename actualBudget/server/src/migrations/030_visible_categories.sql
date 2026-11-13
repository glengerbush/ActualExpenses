-- Retire category visibility while retaining legacy archive columns.
UPDATE categories SET hidden=0;
CREATE TRIGGER visible_categories_insert AFTER INSERT ON categories
WHEN NEW.hidden<>0 BEGIN
 UPDATE categories SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;
CREATE TRIGGER visible_categories_update AFTER UPDATE OF hidden ON categories
WHEN NEW.hidden<>0 BEGIN
 UPDATE categories SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;
UPDATE category_groups SET hidden=0;
CREATE TRIGGER visible_category_groups_insert AFTER INSERT ON category_groups
WHEN NEW.hidden<>0 BEGIN
 UPDATE category_groups SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;
CREATE TRIGGER visible_category_groups_update AFTER UPDATE OF hidden ON category_groups
WHEN NEW.hidden<>0 BEGIN
 UPDATE category_groups SET hidden=0 WHERE budget_id=NEW.budget_id AND id=NEW.id;
END;
