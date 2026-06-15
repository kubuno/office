-- 000004_office_fonts.up.sql

CREATE TABLE IF NOT EXISTS user_fonts (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL,
    name        VARCHAR(255) NOT NULL,
    css_family  VARCHAR(255) NOT NULL,
    source      VARCHAR(20) NOT NULL DEFAULT 'google'
                    CHECK (source IN ('google', 'url')),
    import_url  VARCHAR(2000) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_user_font UNIQUE (user_id, css_family)
);

CREATE INDEX idx_office_uf_user ON user_fonts(user_id);
