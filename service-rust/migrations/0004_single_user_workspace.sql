-- Account rows and their passwords remain intact for legacy data ownership.
-- Identity now belongs to one persistent local workspace; sessions are unused.
DROP TABLE IF EXISTS sessions;
