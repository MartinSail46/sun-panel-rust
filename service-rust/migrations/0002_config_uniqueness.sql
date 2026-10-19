-- Preserve duplicate legacy configuration rows before selecting the current value.
CREATE TABLE rust_legacy_user_config_duplicates AS
 SELECT * FROM user_config WHERE rowid NOT IN (SELECT MAX(rowid) FROM user_config GROUP BY user_id);
DELETE FROM user_config WHERE rowid NOT IN (SELECT MAX(rowid) FROM user_config GROUP BY user_id);
CREATE UNIQUE INDEX rust_user_config_owner ON user_config(user_id);

CREATE TABLE rust_legacy_module_config_duplicates AS SELECT * FROM module_config WHERE id IN (
 SELECT id FROM (SELECT id, ROW_NUMBER() OVER (PARTITION BY user_id, name ORDER BY (deleted_at IS NULL) DESC, id DESC) AS n FROM module_config) WHERE n > 1
);
DELETE FROM module_config WHERE id IN (SELECT id FROM rust_legacy_module_config_duplicates);
CREATE UNIQUE INDEX rust_module_config_key ON module_config(user_id, name);

CREATE TABLE rust_legacy_system_setting_duplicates AS
 SELECT * FROM system_setting WHERE id NOT IN (SELECT MAX(id) FROM system_setting GROUP BY config_name);
DELETE FROM system_setting WHERE id NOT IN (SELECT MAX(id) FROM system_setting GROUP BY config_name);
CREATE UNIQUE INDEX rust_system_setting_key ON system_setting(config_name);
