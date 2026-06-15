-- ── Sous-module Script (automatisation / scripting) ──────────────────────────

CREATE SCHEMA IF NOT EXISTS office_script;

CREATE OR REPLACE FUNCTION office_script.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

-- ── Scripts ───────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_script.scripts (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id            UUID NOT NULL,
    name                VARCHAR(255) NOT NULL DEFAULT 'Nouveau script',
    description         TEXT,
    source_code         TEXT NOT NULL DEFAULT '// Kubuno Script\n',
    compiled_code       TEXT,
    compile_error       TEXT,
    timeout_secs        INTEGER NOT NULL DEFAULT 30,
    memory_limit_mb     INTEGER NOT NULL DEFAULT 64,
    run_count           INTEGER NOT NULL DEFAULT 0,
    last_run_at         TIMESTAMPTZ,
    last_run_status     VARCHAR(15),
    is_trashed          BOOLEAN NOT NULL DEFAULT FALSE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oscript_scripts_owner ON office_script.scripts(owner_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_oscript_scripts_trashed ON office_script.scripts(owner_id, is_trashed);

CREATE TRIGGER scripts_updated_at
    BEFORE UPDATE ON office_script.scripts
    FOR EACH ROW EXECUTE FUNCTION office_script.set_updated_at();

-- ── Déclencheurs ──────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_script.triggers (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    script_id           UUID NOT NULL REFERENCES office_script.scripts(id) ON DELETE CASCADE,
    owner_id            UUID NOT NULL,
    name                VARCHAR(255) NOT NULL DEFAULT 'Déclencheur',
    trigger_type        VARCHAR(10) NOT NULL CHECK (trigger_type IN ('cron', 'event', 'webhook')),
    cron_expression     VARCHAR(100),
    event_name          VARCHAR(100),
    event_module        VARCHAR(50),
    event_filter        JSONB NOT NULL DEFAULT '{}',
    webhook_token       VARCHAR(64) UNIQUE DEFAULT md5(random()::text || clock_timestamp()::text),
    input_vars          JSONB NOT NULL DEFAULT '{}',
    is_active           BOOLEAN NOT NULL DEFAULT TRUE,
    last_fired_at       TIMESTAMPTZ,
    fire_count          INTEGER NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oscript_triggers_script ON office_script.triggers(script_id);
CREATE INDEX IF NOT EXISTS idx_oscript_triggers_owner  ON office_script.triggers(owner_id);
CREATE INDEX IF NOT EXISTS idx_oscript_triggers_active ON office_script.triggers(is_active) WHERE is_active = TRUE;

-- ── Exécutions ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS office_script.runs (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    script_id       UUID NOT NULL REFERENCES office_script.scripts(id) ON DELETE CASCADE,
    owner_id        UUID NOT NULL,
    trigger_id      UUID REFERENCES office_script.triggers(id) ON DELETE SET NULL,
    run_source      VARCHAR(10) NOT NULL DEFAULT 'manual',
    status          VARCHAR(15) NOT NULL DEFAULT 'running',
    duration_ms     INTEGER,
    memory_used_kb  INTEGER,
    console_output  JSONB NOT NULL DEFAULT '[]',
    return_value    JSONB,
    error_message   TEXT,
    error_stack     TEXT,
    trigger_data    JSONB,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_oscript_runs_script ON office_script.runs(script_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_oscript_runs_owner  ON office_script.runs(owner_id, started_at DESC);

-- ── Macros (boutons attachés à des documents) ─────────────────────────────────

CREATE TABLE IF NOT EXISTS office_script.macros (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    script_id       UUID NOT NULL REFERENCES office_script.scripts(id) ON DELETE CASCADE,
    owner_id        UUID NOT NULL,
    document_type   VARCHAR(20),
    document_id     UUID,
    button_label    VARCHAR(100) NOT NULL DEFAULT 'Exécuter',
    button_icon     VARCHAR(50) NOT NULL DEFAULT '⚡',
    position        INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oscript_macros_owner    ON office_script.macros(owner_id);
CREATE INDEX IF NOT EXISTS idx_oscript_macros_document ON office_script.macros(document_type, document_id);
CREATE INDEX IF NOT EXISTS idx_oscript_macros_script   ON office_script.macros(script_id);
