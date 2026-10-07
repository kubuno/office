-- Collaboration epoch of the entities the web edits through a real-time Yjs room.
--
-- The web names its room after the entity AND this epoch
-- (`office-document:<id>` at epoch 0, `office-document:<id>~<epoch>` after).
-- A REST write that replaces the stored content from outside the room (desktop
-- apps, Android, version restore) bumps the epoch in the same transaction as the
-- content write: the old room — whose state still holds the previous content —
-- is abandoned, and the next web session seeds a fresh room from the stored
-- content. A web save sends the epoch of its room and is refused once it is
-- stale, so a session still holding the old state can never write it back.
ALTER TABLE office.documents     ADD COLUMN IF NOT EXISTS collab_epoch BIGINT NOT NULL DEFAULT 0;
ALTER TABLE office.spreadsheets  ADD COLUMN IF NOT EXISTS collab_epoch BIGINT NOT NULL DEFAULT 0;
ALTER TABLE office.presentations ADD COLUMN IF NOT EXISTS collab_epoch BIGINT NOT NULL DEFAULT 0;
