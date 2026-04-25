-- Fix users.created_at to use timezone-aware timestamp for consistency.
ALTER TABLE users
    ALTER COLUMN created_at TYPE TIMESTAMPTZ
    USING created_at AT TIME ZONE 'UTC';

-- Enforce valid rule_type values at the DB level so direct inserts can't bypass
-- application-level validation.
ALTER TABLE flag_rules
    ADD CONSTRAINT flag_rules_rule_type_check
    CHECK (rule_type IN ('user_id', 'user_email', 'email_domain'));

-- Index to support analytics queries filtering evaluations by user.
CREATE INDEX IF NOT EXISTS idx_flag_evaluations_user_identifier
    ON flag_evaluations (user_identifier);

-- Index to support analytics queries filtering evaluations by flag.
CREATE INDEX IF NOT EXISTS idx_flag_evaluations_flag_id
    ON flag_evaluations (flag_id);
