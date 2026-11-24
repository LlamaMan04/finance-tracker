-- Establishes the profile-local finance schema. Every mutable finance record is scoped to the
-- authenticated profile even though profiles currently use separate encrypted database files.
CREATE TABLE profile_scopes (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (length(id) BETWEEN 1 AND 128)
) STRICT;

-- Records compatibility values separately from the migration ledger so backup and restore code
-- can validate a protected payload before attempting to mutate the active database.
CREATE TABLE ledger_schema_metadata (
    singleton_id INTEGER PRIMARY KEY NOT NULL CHECK (singleton_id = 1),
    compatibility_version INTEGER NOT NULL CHECK (compatibility_version > 0),
    minimum_backup_format_version INTEGER NOT NULL CHECK (minimum_backup_format_version > 0),
    updated_at TEXT NOT NULL
) STRICT;

INSERT INTO ledger_schema_metadata (
    singleton_id,
    compatibility_version,
    minimum_backup_format_version,
    updated_at
) VALUES (1, 1, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

-- Budgets preserve their selected currency and optional planning bounds for period allocations.
CREATE TABLE budgets (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
    currency TEXT NOT NULL CHECK (currency GLOB '[A-Z][A-Z][A-Z]'),
    starts_on TEXT,
    ends_on TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    UNIQUE (profile_id, id),
    CHECK (starts_on IS NULL OR ends_on IS NULL OR starts_on <= ends_on)
) STRICT;

-- Categories remain extensible through hierarchy, kind, deterministic display ordering, and archiving.
CREATE TABLE categories (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    parent_category_id TEXT,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
    category_kind TEXT NOT NULL CHECK (category_kind IN ('income', 'expense')),
    color TEXT CHECK (color IS NULL OR length(color) BETWEEN 1 AND 32),
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    FOREIGN KEY (profile_id, parent_category_id) REFERENCES categories(profile_id, id),
    UNIQUE (profile_id, id),
    CHECK (parent_category_id IS NULL OR parent_category_id <> id)
) STRICT;

-- Accounts store opening balance only; current balances are derived from ledger entries in Rust.
CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
    account_type TEXT NOT NULL CHECK (length(trim(account_type)) BETWEEN 1 AND 64),
    currency TEXT NOT NULL CHECK (currency GLOB '[A-Z][A-Z][A-Z]'),
    opening_minor_units INTEGER NOT NULL,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    UNIQUE (profile_id, id)
) STRICT;

-- Transactions are immutable ledger facts; corrections and transfers link related facts below.
CREATE TABLE transactions (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    budget_id TEXT,
    category_id TEXT,
    occurred_on TEXT NOT NULL,
    entry_type TEXT NOT NULL CHECK (entry_type IN ('income', 'expense', 'transfer', 'correction')),
    minor_units INTEGER NOT NULL,
    currency TEXT NOT NULL CHECK (currency GLOB '[A-Z][A-Z][A-Z]'),
    payee TEXT CHECK (payee IS NULL OR length(payee) <= 200),
    description TEXT CHECK (description IS NULL OR length(description) <= 500),
    notes TEXT CHECK (notes IS NULL OR length(notes) <= 4000),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    FOREIGN KEY (profile_id, account_id) REFERENCES accounts(profile_id, id),
    FOREIGN KEY (profile_id, budget_id) REFERENCES budgets(profile_id, id),
    FOREIGN KEY (profile_id, category_id) REFERENCES categories(profile_id, id),
    UNIQUE (profile_id, id)
) STRICT;

-- Links the two transaction facts of one atomic transfer without classifying either as budget income or expense.
CREATE TABLE transfers (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    source_transaction_id TEXT NOT NULL,
    destination_transaction_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    FOREIGN KEY (profile_id, source_transaction_id) REFERENCES transactions(profile_id, id),
    FOREIGN KEY (profile_id, destination_transaction_id) REFERENCES transactions(profile_id, id),
    UNIQUE (profile_id, id),
    UNIQUE (profile_id, source_transaction_id),
    UNIQUE (profile_id, destination_transaction_id),
    CHECK (source_transaction_id <> destination_transaction_id)
) STRICT;

-- Stores immutable correction relationships so editing history can remain auditable.
CREATE TABLE transaction_corrections (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    original_transaction_id TEXT NOT NULL,
    correction_transaction_id TEXT NOT NULL,
    correction_kind TEXT NOT NULL CHECK (correction_kind IN ('reversal', 'adjustment')),
    reason TEXT CHECK (reason IS NULL OR length(reason) <= 500),
    created_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    FOREIGN KEY (profile_id, original_transaction_id) REFERENCES transactions(profile_id, id),
    FOREIGN KEY (profile_id, correction_transaction_id) REFERENCES transactions(profile_id, id),
    UNIQUE (profile_id, id),
    UNIQUE (profile_id, correction_transaction_id),
    CHECK (original_transaction_id <> correction_transaction_id)
) STRICT;

-- Splits associate exact minor-unit portions of a transaction with budget categories.
CREATE TABLE transaction_allocations (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    category_id TEXT NOT NULL,
    minor_units INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    FOREIGN KEY (profile_id, transaction_id) REFERENCES transactions(profile_id, id),
    FOREIGN KEY (profile_id, category_id) REFERENCES categories(profile_id, id),
    UNIQUE (profile_id, id)
) STRICT;

-- Budget allocations preserve planned minor units per budget/category planning period.
CREATE TABLE budget_allocations (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    budget_id TEXT NOT NULL,
    category_id TEXT NOT NULL,
    period_starts_on TEXT NOT NULL,
    period_ends_on TEXT NOT NULL,
    planned_minor_units INTEGER NOT NULL,
    notes TEXT CHECK (notes IS NULL OR length(notes) <= 1000),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    FOREIGN KEY (profile_id, budget_id) REFERENCES budgets(profile_id, id),
    FOREIGN KEY (profile_id, category_id) REFERENCES categories(profile_id, id),
    UNIQUE (profile_id, id),
    UNIQUE (profile_id, budget_id, category_id, period_starts_on, period_ends_on),
    CHECK (period_starts_on <= period_ends_on)
) STRICT;

-- Supports the profile-scoped, deterministic history filters planned for the transaction workflow.
CREATE INDEX idx_transactions_profile_occurred_on_id
    ON transactions (profile_id, occurred_on DESC, id DESC);
CREATE INDEX idx_transactions_profile_account_occurred_on
    ON transactions (profile_id, account_id, occurred_on DESC, id DESC);
CREATE INDEX idx_transactions_profile_category_occurred_on
    ON transactions (profile_id, category_id, occurred_on DESC, id DESC);
CREATE INDEX idx_transactions_profile_type_occurred_on
    ON transactions (profile_id, entry_type, occurred_on DESC, id DESC);
CREATE INDEX idx_transaction_allocations_profile_transaction
    ON transaction_allocations (profile_id, transaction_id);
CREATE INDEX idx_budget_allocations_profile_budget_period
    ON budget_allocations (profile_id, budget_id, period_starts_on, period_ends_on);
