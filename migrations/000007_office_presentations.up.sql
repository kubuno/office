-- 000007_office_presentations.up.sql
-- search_path = office, public (set at connection level)

CREATE TABLE presentations (
    id               UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id         UUID        NOT NULL,
    title            VARCHAR(500) NOT NULL DEFAULT 'Présentation sans titre',
    theme            JSONB       NOT NULL DEFAULT '{"name":"Défaut","primaryColor":"#1a73e8","bgColor":"#ffffff","fontFamily":"Google Sans, Arial, sans-serif","accentColor":"#ea4335","textColor":"#202124"}',
    aspect_ratio     VARCHAR(10) NOT NULL DEFAULT '16:9',
    slide_width      INT         NOT NULL DEFAULT 960,
    slide_height     INT         NOT NULL DEFAULT 540,
    slide_count      INT         NOT NULL DEFAULT 0,
    is_starred       BOOL        NOT NULL DEFAULT FALSE,
    is_trashed       BOOL        NOT NULL DEFAULT FALSE,
    trashed_at       TIMESTAMPTZ,
    last_edited_by   UUID,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_presentations_owner   ON presentations(owner_id);
CREATE INDEX idx_presentations_starred ON presentations(owner_id, is_starred) WHERE is_starred = TRUE AND is_trashed = FALSE;
CREATE INDEX idx_presentations_updated ON presentations(owner_id, updated_at DESC);

CREATE TABLE slides (
    id               UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    presentation_id  UUID        NOT NULL REFERENCES presentations(id) ON DELETE CASCADE,
    position         INT         NOT NULL DEFAULT 0,
    background       JSONB       NOT NULL DEFAULT '{"type":"color","color":"#ffffff"}',
    notes            TEXT        NOT NULL DEFAULT '',
    elements         JSONB       NOT NULL DEFAULT '[]',
    transition       JSONB       NOT NULL DEFAULT '{"type":"none","duration":0.3}',
    thumbnail_path   TEXT,
    thumbnail_dirty  BOOL        NOT NULL DEFAULT TRUE,
    is_hidden        BOOL        NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_slides_presentation ON slides(presentation_id, position);

CREATE TABLE presentation_shares (
    id               UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    presentation_id  UUID        NOT NULL REFERENCES presentations(id) ON DELETE CASCADE,
    created_by       UUID        NOT NULL,
    token            VARCHAR(64) UNIQUE NOT NULL DEFAULT encode(uuid_send(gen_random_uuid()) || uuid_send(gen_random_uuid()), 'hex'),
    permission       VARCHAR(10) NOT NULL DEFAULT 'read'
                         CHECK (permission IN ('read', 'edit')),
    is_active        BOOL        NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pres_shares_token ON presentation_shares(token) WHERE is_active = TRUE;
CREATE INDEX idx_pres_shares_pres  ON presentation_shares(presentation_id);

-- Trigger: maintain slide_count on presentations
CREATE OR REPLACE FUNCTION office_update_slide_count()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE presentations
           SET slide_count = (
               SELECT COUNT(*) FROM slides
               WHERE presentation_id = NEW.presentation_id AND is_hidden = FALSE
           )
         WHERE id = NEW.presentation_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE presentations
           SET slide_count = (
               SELECT COUNT(*) FROM slides
               WHERE presentation_id = OLD.presentation_id AND is_hidden = FALSE
           )
         WHERE id = OLD.presentation_id;
    ELSIF TG_OP = 'UPDATE' AND (OLD.is_hidden IS DISTINCT FROM NEW.is_hidden OR OLD.presentation_id IS DISTINCT FROM NEW.presentation_id) THEN
        UPDATE presentations
           SET slide_count = (
               SELECT COUNT(*) FROM slides
               WHERE presentation_id = NEW.presentation_id AND is_hidden = FALSE
           )
         WHERE id = NEW.presentation_id;
        IF OLD.presentation_id IS DISTINCT FROM NEW.presentation_id THEN
            UPDATE presentations
               SET slide_count = (
                   SELECT COUNT(*) FROM slides
                   WHERE presentation_id = OLD.presentation_id AND is_hidden = FALSE
               )
             WHERE id = OLD.presentation_id;
        END IF;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER slides_slide_count
    AFTER INSERT OR DELETE OR UPDATE OF is_hidden, presentation_id
    ON slides
    FOR EACH ROW EXECUTE FUNCTION office_update_slide_count();

-- Trigger: updated_at on presentations
CREATE TRIGGER presentations_updated_at
    BEFORE UPDATE ON presentations
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();

-- Trigger: updated_at on slides
CREATE TRIGGER slides_updated_at
    BEFORE UPDATE ON slides
    FOR EACH ROW EXECUTE FUNCTION office_set_updated_at();
