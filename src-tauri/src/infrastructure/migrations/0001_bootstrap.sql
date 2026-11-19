-- Creates only non-sensitive bootstrap metadata while the at-rest protection design is pending.
CREATE TABLE application_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;
