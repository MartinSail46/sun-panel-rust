CREATE INDEX rust_icon_owner_order ON item_icon(user_id, deleted_at, sort, created_at, id);
CREATE INDEX rust_group_owner_order ON item_icon_group(user_id, deleted_at, sort, created_at, id);
DROP INDEX IF EXISTS rust_group_owner_sort;
-- The Go token was a raw persistent credential. Rust sessions use independent hashes.
UPDATE user SET token = '' WHERE COALESCE(token, '') <> '';
