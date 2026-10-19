-- Retain both legacy addresses for recovery before selecting the intranet address.
CREATE TABLE rust_legacy_item_addresses (
 item_id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL,
 previous_url TEXT NOT NULL, previous_lan_url TEXT NOT NULL,
 migrated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO rust_legacy_item_addresses(item_id,user_id,previous_url,previous_lan_url)
 SELECT id,user_id,COALESCE(url,''),COALESCE(lan_url,'') FROM item_icon
 WHERE TRIM(COALESCE(lan_url,'')) <> '';
UPDATE item_icon SET url=TRIM(lan_url) WHERE TRIM(COALESCE(lan_url,'')) <> '';
UPDATE item_icon SET lan_url='' WHERE COALESCE(lan_url,'') <> '';

ALTER TABLE user_config ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE module_config ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;

-- A reviewed import can reject changes made since its preview, including file replacement.
CREATE TABLE rust_navigation_revision (user_id INTEGER PRIMARY KEY, revision INTEGER NOT NULL DEFAULT 0);
INSERT INTO rust_navigation_revision(user_id)
 SELECT user_id FROM item_icon_group UNION SELECT user_id FROM item_icon;
CREATE TRIGGER rust_navigation_item_icon_insert AFTER INSERT ON item_icon
BEGIN
 INSERT INTO rust_navigation_revision(user_id,revision) VALUES(NEW.user_id,1)
 ON CONFLICT(user_id) DO UPDATE SET revision=revision+1;
END;
CREATE TRIGGER rust_navigation_item_icon_update AFTER UPDATE ON item_icon
BEGIN
 INSERT INTO rust_navigation_revision(user_id,revision) VALUES(NEW.user_id,1)
 ON CONFLICT(user_id) DO UPDATE SET revision=revision+1;
END;
CREATE TRIGGER rust_navigation_item_icon_delete AFTER DELETE ON item_icon
BEGIN
 INSERT INTO rust_navigation_revision(user_id,revision) VALUES(OLD.user_id,1)
 ON CONFLICT(user_id) DO UPDATE SET revision=revision+1;
END;
CREATE TRIGGER rust_navigation_item_icon_group_insert AFTER INSERT ON item_icon_group
BEGIN
 INSERT INTO rust_navigation_revision(user_id,revision) VALUES(NEW.user_id,1)
 ON CONFLICT(user_id) DO UPDATE SET revision=revision+1;
END;
CREATE TRIGGER rust_navigation_item_icon_group_update AFTER UPDATE ON item_icon_group
BEGIN
 INSERT INTO rust_navigation_revision(user_id,revision) VALUES(NEW.user_id,1)
 ON CONFLICT(user_id) DO UPDATE SET revision=revision+1;
END;
CREATE TRIGGER rust_navigation_item_icon_group_delete AFTER DELETE ON item_icon_group
BEGIN
 INSERT INTO rust_navigation_revision(user_id,revision) VALUES(OLD.user_id,1)
 ON CONFLICT(user_id) DO UPDATE SET revision=revision+1;
END;

-- Image references need only consult retired paths during ordinary saves.
CREATE INDEX rust_file_retired_paths ON file(src) WHERE deleted_at IS NOT NULL;
