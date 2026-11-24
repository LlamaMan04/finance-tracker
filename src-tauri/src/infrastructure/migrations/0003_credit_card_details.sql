-- Keeps optional credit-card terms beside the generic account while transactions remain the sole
-- source of the card balance. Amounts are in the account's currency and integer minor units.
CREATE TABLE credit_card_details (
    profile_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    credit_limit_minor_units INTEGER CHECK (
        credit_limit_minor_units IS NULL OR credit_limit_minor_units >= 0
    ),
    statement_closing_day INTEGER CHECK (
        statement_closing_day IS NULL OR statement_closing_day BETWEEN 1 AND 31
    ),
    payment_due_day INTEGER CHECK (
        payment_due_day IS NULL OR payment_due_day BETWEEN 1 AND 31
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (profile_id, account_id),
    FOREIGN KEY (profile_id) REFERENCES profile_scopes(id),
    FOREIGN KEY (profile_id, account_id) REFERENCES accounts(profile_id, id)
) STRICT;

-- Prevents a details row from being attached to a generic cash, checking, or future account type.
CREATE TRIGGER validate_credit_card_details_account_type
BEFORE INSERT ON credit_card_details
FOR EACH ROW
WHEN (SELECT account_type FROM accounts WHERE profile_id = NEW.profile_id AND id = NEW.account_id)
    <> 'credit_card'
BEGIN
    SELECT RAISE(ABORT, 'credit card details require a credit_card account');
END;

-- Preserves that invariant if a later account-edit workflow changes an account's type.
CREATE TRIGGER prevent_credit_card_account_type_change
BEFORE UPDATE OF account_type ON accounts
FOR EACH ROW
WHEN OLD.account_type = 'credit_card'
    AND NEW.account_type <> 'credit_card'
    AND EXISTS (
        SELECT 1 FROM credit_card_details
        WHERE profile_id = OLD.profile_id AND account_id = OLD.id
    )
BEGIN
    SELECT RAISE(ABORT, 'remove credit card details before changing account type');
END;

-- Supports the card-details lookup rendered with an account summary without duplicating balances.
CREATE INDEX idx_credit_card_details_profile_account
    ON credit_card_details (profile_id, account_id);
