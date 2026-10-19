-- A deletion batch binds only the applications deleted with a particular group.
-- NULL means the deletion predates this feature; its cascade cannot be inferred
-- safely, so those applications remain separately recoverable.
ALTER TABLE item_icon ADD COLUMN trash_batch TEXT;
ALTER TABLE item_icon_group ADD COLUMN trash_batch TEXT;
CREATE INDEX rust_trash_items ON item_icon(user_id, deleted_at, trash_batch, item_icon_group_id);
CREATE INDEX rust_trash_groups ON item_icon_group(user_id, deleted_at, trash_batch);
