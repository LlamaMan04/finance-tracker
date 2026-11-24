import { useEffect, useState } from "react";
import type {
  AccountDto,
  BudgetAllocationDto,
  BudgetDto,
  CategoryDto,
  TransactionDto,
  TransactionHistoryInput,
  TransferDto,
} from "../api/contracts";
import { FinanceCommandClient } from "../api/commandClient";
import { ErrorNotice } from "./ErrorNotice";

/** Reuses the command adapter without retaining protected values outside React state. */
const commandClient = new FinanceCommandClient();

/** Defines the protected workspace controls supplied by the authenticated application shell. */
interface FinanceWorkspaceProps {
  profileId: string;
  isWorking: boolean;
  onLock: () => void;
  onSwitchProfile: () => void;
}

/** Holds one UI-only row until Rust atomically records the transaction split. */
interface AllocationRow {
  id: string;
  categoryId: string;
  minorUnits: string;
}

/** Renders the first protected finance workflows: budgets, categories, and accounts. */
export function FinanceWorkspace({
  profileId,
  isWorking,
  onLock,
  onSwitchProfile,
}: FinanceWorkspaceProps) {
  /** Holds budget DTOs returned by Rust for the active profile only. */
  const [budgets, setBudgets] = useState<BudgetDto[]>([]);
  /** Holds category DTOs returned by Rust for the active profile only. */
  const [categories, setCategories] = useState<CategoryDto[]>([]);
  /** Holds account DTOs returned by Rust for the active profile only. */
  const [accounts, setAccounts] = useState<AccountDto[]>([]);
  /** Holds Rust-authoritative planned budget category amounts for the active profile only. */
  const [budgetAllocations, setBudgetAllocations] = useState<BudgetAllocationDto[]>([]);
  /** Tracks the selected ledger kind so the category choices remain applicable. */
  const [transactionKind, setTransactionKind] = useState<"income" | "expense">("expense");
  /** Chooses whether the entry uses one category or exact category portions. */
  const [isSplitTransaction, setIsSplitTransaction] = useState(false);
  /** Holds unsubmitted split rows and is cleared immediately after a successful record. */
  const [allocationRows, setAllocationRows] = useState<AllocationRow[]>([]);
  /** Shows the Rust-normalized result of the current entry without implementing history yet. */
  const [lastRecordedTransaction, setLastRecordedTransaction] = useState<TransactionDto | null>(
    null,
  );
  /** Shows the safe normalized transfer result without retaining an independent ledger copy. */
  const [lastRecordedTransfer, setLastRecordedTransfer] = useState<TransferDto | null>(null);
  /** Holds the current Rust-authoritative history page rather than a client-maintained ledger. */
  const [historyTransactions, setHistoryTransactions] = useState<TransactionDto[]>([]);
  /** Holds the total matching history records returned with the current page. */
  const [historyTotalCount, setHistoryTotalCount] = useState(0);
  /** Retains the applied query so pagination does not discard active filters. */
  const [historyRequest, setHistoryRequest] = useState<TransactionHistoryInput>({
    startsOn: null,
    endsOn: null,
    accountId: null,
    categoryId: null,
    entryType: null,
    search: null,
    page: 0,
    pageSize: 20,
  });
  /** Holds a safe UI error and never raw backend diagnostics. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  /** Prevents duplicate finance submissions while a command is running. */
  const [isSaving, setIsSaving] = useState(false);

  /** Loads all reference data together after the profile becomes authenticated. */
  async function refresh() {
    const [nextBudgets, nextCategories, nextAccounts, nextBudgetAllocations, nextHistory] = await Promise.all([
      commandClient.listBudgets(),
      commandClient.listCategories(),
      commandClient.listAccounts(),
      commandClient.listBudgetAllocations(),
      commandClient.listTransactions(historyRequest),
    ]);
    setBudgets(nextBudgets);
    setCategories(nextCategories);
    setAccounts(nextAccounts);
    setBudgetAllocations(nextBudgetAllocations);
    setHistoryTransactions(nextHistory.transactions);
    setHistoryTotalCount(nextHistory.totalCount);
  }

  /** Fetches an explicitly selected filter page while preserving Rust as the ledger source of truth. */
  async function loadHistory(request: TransactionHistoryInput) {
    const nextHistory = await commandClient.listTransactions(request);
    setHistoryRequest(request);
    setHistoryTransactions(nextHistory.transactions);
    setHistoryTotalCount(nextHistory.totalCount);
  }

  useEffect(() => {
    void refresh().catch(() => setErrorMessage("The operation could not be completed."));
  }, []);

  /** Runs one mutation and reloads the Rust-authoritative workspace state. */
  async function mutate(operation: () => Promise<unknown>) {
    setIsSaving(true);
    setErrorMessage(null);
    try {
      await operation();
      await refresh();
    } catch {
      setErrorMessage("The supplied value is invalid or the operation could not be completed.");
    } finally {
      setIsSaving(false);
    }
  }

  /** Creates a budget from the short form without retaining its values after submission. */
  function submitBudget(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    /** Retains the form element beyond the submit event lifetime. */
    const form = event.currentTarget;
    /** Captures fields synchronously before the async command begins. */
    const values = new FormData(form);
    void mutate(async () => {
      await commandClient.createBudget({
        name: String(values.get("budgetName") ?? ""),
        currency: String(values.get("budgetCurrency") ?? ""),
      });
      form.reset();
    });
  }

  /** Creates an income or expense category from the short form. */
  function submitCategory(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    /** Retains the form element beyond the submit event lifetime. */
    const form = event.currentTarget;
    /** Captures fields synchronously before the async command begins. */
    const values = new FormData(form);
    void mutate(async () => {
      await commandClient.createCategory({
        name: String(values.get("categoryName") ?? ""),
        categoryKind: String(values.get("categoryKind")) as "income" | "expense",
        color: String(values.get("categoryColor") ?? "") || null,
        sortOrder: Number(values.get("categorySort") ?? 0),
      });
      form.reset();
    });
  }

  /** Creates an account while preserving the opening amount as an exact decimal string. */
  function submitAccount(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    /** Retains the form element beyond the submit event lifetime. */
    const form = event.currentTarget;
    /** Captures fields synchronously before the async command begins. */
    const values = new FormData(form);
    void mutate(async () => {
      await commandClient.createAccount({
        name: String(values.get("accountName") ?? ""),
        accountType: String(values.get("accountType")) as AccountDto["accountType"],
        currency: String(values.get("accountCurrency") ?? ""),
        openingMinorUnits: String(values.get("openingMinorUnits") ?? ""),
      });
      form.reset();
    });
  }

  /** Creates or replaces the one plan for a budget, category, and inclusive period. */
  function submitBudgetAllocation(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    /** Retains the form element beyond the submit event lifetime. */
    const form = event.currentTarget;
    /** Captures fields synchronously before the async command begins. */
    const values = new FormData(form);
    void mutate(async () => {
      await commandClient.upsertBudgetAllocation({
        budgetId: String(values.get("allocationBudgetId") ?? ""),
        categoryId: String(values.get("allocationCategoryId") ?? ""),
        periodStartsOn: String(values.get("allocationPeriodStartsOn") ?? ""),
        periodEndsOn: String(values.get("allocationPeriodEndsOn") ?? ""),
        plannedMinorUnits: String(values.get("allocationPlannedMinorUnits") ?? ""),
        notes: String(values.get("allocationNotes") ?? "") || null,
      });
      form.reset();
    });
  }

  /** Records one signed income or expense fact and displays only Rust's normalized response. */
  function submitTransaction(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    /** Retains the form element beyond the submit event lifetime. */
    const form = event.currentTarget;
    /** Captures all form values synchronously before the async command begins. */
    const values = new FormData(form);
    void mutate(async () => {
      const transaction = await commandClient.createTransaction({
        accountId: String(values.get("transactionAccountId") ?? ""),
        budgetId: String(values.get("transactionBudgetId") ?? "") || null,
        categoryId: String(values.get("transactionCategoryId") ?? "") || null,
        entryType: transactionKind,
        minorUnits: String(values.get("transactionMinorUnits") ?? ""),
        currency: String(values.get("transactionCurrency") ?? ""),
        occurredOn: String(values.get("transactionOccurredOn") ?? ""),
        payee: String(values.get("transactionPayee") ?? "") || null,
        description: String(values.get("transactionDescription") ?? "") || null,
        notes: String(values.get("transactionNotes") ?? "") || null,
        allocations: isSplitTransaction
          ? allocationRows.map(({ categoryId, minorUnits }) => ({ categoryId, minorUnits }))
          : [],
      });
      setLastRecordedTransaction(transaction);
      form.reset();
      setTransactionKind("expense");
      setIsSplitTransaction(false);
      setAllocationRows([]);
    });
  }

  /** Records one reviewed transfer after confirming its immutable paired ledger effect. */
  function submitTransfer(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    /** Retains the form element beyond the submit event lifetime. */
    const form = event.currentTarget;
    /** Captures fields synchronously before the async command begins. */
    const values = new FormData(form);
    if (!window.confirm("Record this transfer? It creates immutable entries in both accounts.")) {
      return;
    }
    void mutate(async () => {
      const transfer = await commandClient.createTransfer({
        sourceAccountId: String(values.get("transferSourceAccountId") ?? ""),
        destinationAccountId: String(values.get("transferDestinationAccountId") ?? ""),
        minorUnits: String(values.get("transferMinorUnits") ?? ""),
        currency: String(values.get("transferCurrency") ?? ""),
        occurredOn: String(values.get("transferOccurredOn") ?? ""),
      });
      setLastRecordedTransfer(transfer);
      form.reset();
    });
  }

  /** Applies optional history filters without duplicating filtering or pagination rules in React. */
  function submitHistoryFilters(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    /** Captures filter values synchronously before protected history is queried. */
    const values = new FormData(event.currentTarget);
    const request: TransactionHistoryInput = {
      startsOn: String(values.get("historyStartsOn") ?? "") || null,
      endsOn: String(values.get("historyEndsOn") ?? "") || null,
      accountId: String(values.get("historyAccountId") ?? "") || null,
      categoryId: String(values.get("historyCategoryId") ?? "") || null,
      entryType:
        (String(values.get("historyEntryType") ?? "") as TransactionHistoryInput["entryType"]) ||
        null,
      search: String(values.get("historySearch") ?? "") || null,
      page: 0,
      pageSize: historyRequest.pageSize,
    };
    setErrorMessage(null);
    void loadHistory(request).catch(() => setErrorMessage("The operation could not be completed."));
  }

  /** Creates a compensating ledger fact only after the user confirms the permanent audit action. */
  function confirmReverse(transaction: TransactionDto) {
    if (!window.confirm("Reverse this entry? This adds an immutable equal-and-opposite correction.")) {
      return;
    }
    void mutate(() => commandClient.reverseTransaction(transaction.id));
  }

  /** Adds an empty split row without calculating or validating financial values in the UI. */
  function addAllocationRow() {
    setAllocationRows((rows) => [
      ...rows,
      { id: crypto.randomUUID(), categoryId: "", minorUnits: "" },
    ]);
  }

  /** Updates one transient split field while Rust remains the authoritative validator. */
  function updateAllocationRow(
    id: string,
    field: "categoryId" | "minorUnits",
    value: string,
  ) {
    setAllocationRows((rows) =>
      rows.map((row) => (row.id === id ? { ...row, [field]: value } : row)),
    );
  }

  /** Removes a not-yet-recorded split row without altering protected ledger history. */
  function removeAllocationRow(id: string) {
    setAllocationRows((rows) => rows.filter((row) => row.id !== id));
  }

  /** Uses concise native prompts for the initial edit workflow while commands retain validation. */
  function editBudget(budget: BudgetDto) {
    const name = window.prompt("Budget name", budget.name);
    if (name !== null) void mutate(() => commandClient.updateBudget({ id: budget.id, name }));
  }
  /** Edits every mutable category field with deliberate individual prompts. */
  function editCategory(category: CategoryDto) {
    const name = window.prompt("Category name", category.name);
    const categoryKind = window.prompt("Category kind: income or expense", category.categoryKind);
    const color = window.prompt("Color label (optional)", category.color ?? "");
    const sortOrder = window.prompt("Sort order", String(category.sortOrder));
    if (name !== null && categoryKind !== null && color !== null && sortOrder !== null)
      void mutate(() =>
        commandClient.updateCategory({
          id: category.id,
          name,
          categoryKind: categoryKind as "income" | "expense",
          color: color || null,
          sortOrder: Number(sortOrder),
        }),
      );
  }
  /** Edits mutable account display fields without permitting an opening-balance rewrite. */
  function editAccount(account: AccountDto) {
    const name = window.prompt("Account name", account.name);
    const accountType = window.prompt("Account type", account.accountType);
    if (name !== null && accountType !== null)
      void mutate(() =>
        commandClient.updateAccount({
          id: account.id,
          name,
          accountType: accountType as AccountDto["accountType"],
        }),
      );
  }
  /** Confirms archival because it changes which records are available for new finance work. */
  function confirmArchive(label: string, archive: () => Promise<void>) {
    if (window.confirm(`Archive ${label}? Historical records will be retained.`))
      void mutate(archive);
  }

  const disabled = isWorking || isSaving;
  return (
    <section aria-labelledby="workspace-heading" className="finance-workspace">
      <header className="workspace-header">
        <div>
          <p className="eyebrow">Protected finance workspace</p>
          <h1 id="workspace-heading">{profileId}</h1>
        </div>
        <div className="workspace-actions">
          <button disabled={disabled} onClick={onLock} type="button">
            Lock profile
          </button>
          <button
            className="secondary-button"
            disabled={disabled}
            onClick={onSwitchProfile}
            type="button"
          >
            Switch profile
          </button>
        </div>
      </header>
      <ErrorNotice message={errorMessage} />
      <section aria-labelledby="budgets-heading" className="finance-section">
        <h2 id="budgets-heading">Budgets</h2>
        <form onSubmit={submitBudget}>
          <label htmlFor="budget-name">Name</label>
          <input id="budget-name" name="budgetName" required />
          <label htmlFor="budget-currency">Currency</label>
          <input
            defaultValue="USD"
            id="budget-currency"
            maxLength={3}
            name="budgetCurrency"
            required
          />
          <button disabled={disabled}>Create budget</button>
        </form>
        <RecordList
          records={budgets}
          describe={(budget) =>
            `${budget.name} · ${budget.currency}${budget.isArchived ? " · archived" : ""}`
          }
          onArchive={(budget) =>
            confirmArchive(budget.name, () => commandClient.archiveBudget(budget.id))
          }
          onEdit={editBudget}
        />
      </section>
      <section aria-labelledby="categories-heading" className="finance-section">
        <h2 id="categories-heading">Categories</h2>
        <form onSubmit={submitCategory}>
          <label htmlFor="category-name">Name</label>
          <input id="category-name" name="categoryName" required />
          <label htmlFor="category-kind">Kind</label>
          <select defaultValue="expense" id="category-kind" name="categoryKind">
            <option value="expense">Expense</option>
            <option value="income">Income</option>
          </select>
          <label htmlFor="category-color">Color label (optional)</label>
          <input id="category-color" maxLength={32} name="categoryColor" />
          <label htmlFor="category-sort">Sort order</label>
          <input defaultValue="0" id="category-sort" name="categorySort" type="number" />
          <button disabled={disabled}>Create category</button>
        </form>
        <RecordList
          records={categories}
          describe={(category) =>
            `${category.name} · ${category.categoryKind} · order ${category.sortOrder}${category.isArchived ? " · archived" : ""}`
          }
          onArchive={(category) =>
            confirmArchive(category.name, () => commandClient.archiveCategory(category.id))
          }
          onEdit={editCategory}
        />
      </section>
      <section aria-labelledby="budget-allocations-heading" className="finance-section">
        <h2 id="budget-allocations-heading">Budget period plans</h2>
        <p>
          Plan income with positive minor units and expenses with negative minor units. Submitting
          the same budget, category, and period replaces its existing plan.
        </p>
        <form onSubmit={submitBudgetAllocation}>
          <label htmlFor="allocation-budget">Budget</label>
          <select id="allocation-budget" name="allocationBudgetId" required>
            <option value="">Choose a budget</option>
            {budgets
              .filter((budget) => !budget.isArchived)
              .map((budget) => (
                <option key={budget.id} value={budget.id}>
                  {budget.name} ({budget.currency})
                </option>
              ))}
          </select>
          <label htmlFor="allocation-category">Category</label>
          <select id="allocation-category" name="allocationCategoryId" required>
            <option value="">Choose a category</option>
            {categories
              .filter((category) => !category.isArchived)
              .map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name} ({category.categoryKind})
                </option>
              ))}
          </select>
          <label htmlFor="allocation-period-start">Period starts</label>
          <input
            id="allocation-period-start"
            name="allocationPeriodStartsOn"
            required
            type="date"
          />
          <label htmlFor="allocation-period-end">Period ends</label>
          <input
            id="allocation-period-end"
            name="allocationPeriodEndsOn"
            required
            type="date"
          />
          <label htmlFor="allocation-planned-minor-units">Planned amount (minor units)</label>
          <input
            id="allocation-planned-minor-units"
            inputMode="numeric"
            name="allocationPlannedMinorUnits"
            required
          />
          <label htmlFor="allocation-notes">Notes (optional)</label>
          <textarea id="allocation-notes" maxLength={1000} name="allocationNotes" />
          <button disabled={disabled}>Save period plan</button>
        </form>
        <BudgetAllocationList
          allocations={budgetAllocations}
          budgets={budgets}
          categories={categories}
          disabled={disabled}
          onRemove={(allocation) => {
            if (
              window.confirm(
                "Remove this budget period plan? It does not remove transactions or reference data.",
              )
            ) {
              void mutate(() => commandClient.removeBudgetAllocation(allocation.id));
            }
          }}
        />
      </section>
      <section aria-labelledby="accounts-heading" className="finance-section">
        <h2 id="accounts-heading">Accounts</h2>
        <form onSubmit={submitAccount}>
          <label htmlFor="account-name">Name</label>
          <input id="account-name" name="accountName" required />
          <label htmlFor="account-type">Type</label>
          <select defaultValue="checking" id="account-type" name="accountType">
            <option value="cash">Cash</option>
            <option value="checking">Checking</option>
            <option value="savings">Savings</option>
            <option value="credit_card">Credit card</option>
            <option value="other">Other</option>
          </select>
          <label htmlFor="account-currency">Currency</label>
          <input
            defaultValue="USD"
            id="account-currency"
            maxLength={3}
            name="accountCurrency"
            required
          />
          <label htmlFor="opening-minor-units">Opening balance (minor units)</label>
          <input
            defaultValue="0"
            id="opening-minor-units"
            inputMode="numeric"
            name="openingMinorUnits"
            required
          />
          <button disabled={disabled}>Create account</button>
        </form>
        <RecordList
          records={accounts}
          describe={(account) =>
            `${account.name} · ${account.accountType} · ${account.currency} · opening ${account.openingMinorUnits}${account.isArchived ? " · archived" : ""}`
          }
          onArchive={(account) =>
            confirmArchive(account.name, () => commandClient.archiveAccount(account.id))
          }
          onEdit={editAccount}
        />
      </section>
      <section aria-labelledby="transactions-heading" className="finance-section">
        <h2 id="transactions-heading">Record income or expense</h2>
        <p>Use positive minor units for income and negative minor units for expenses.</p>
        <form onSubmit={submitTransaction}>
          <label htmlFor="transaction-account">Account</label>
          <select id="transaction-account" name="transactionAccountId" required>
            <option value="">Choose an account</option>
            {accounts
              .filter((account) => !account.isArchived)
              .map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name} ({account.currency})
                </option>
              ))}
          </select>
          <label htmlFor="transaction-kind">Type</label>
          <select
            id="transaction-kind"
            name="transactionKind"
            onChange={(event) => {
              setTransactionKind(event.currentTarget.value as "income" | "expense");
              setAllocationRows((rows) => rows.map((row) => ({ ...row, categoryId: "" })));
            }}
            value={transactionKind}
          >
            <option value="expense">Expense</option>
            <option value="income">Income</option>
          </select>
          <label>
            <input
              checked={isSplitTransaction}
              onChange={(event) => {
                setIsSplitTransaction(event.currentTarget.checked);
                if (event.currentTarget.checked && allocationRows.length === 0) addAllocationRow();
              }}
              type="checkbox"
            />{" "}
            Split this transaction across categories
          </label>
          {!isSplitTransaction && (
            <>
              <label htmlFor="transaction-category">Category (optional)</label>
              <select id="transaction-category" name="transactionCategoryId">
                <option value="">No category</option>
                {categories
                  .filter(
                    (category) =>
                      !category.isArchived && category.categoryKind === transactionKind,
                  )
                  .map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
              </select>
            </>
          )}
          {isSplitTransaction && (
            <fieldset>
              <legend>Category splits</legend>
              <p>Each amount must use the transaction sign, and all splits must equal its amount.</p>
              {allocationRows.map((row, index) => (
                <div key={row.id}>
                  <label htmlFor={`split-category-${row.id}`}>Split {index + 1} category</label>
                  <select
                    id={`split-category-${row.id}`}
                    onChange={(event) => updateAllocationRow(row.id, "categoryId", event.currentTarget.value)}
                    required
                    value={row.categoryId}
                  >
                    <option value="">Choose a category</option>
                    {categories
                      .filter(
                        (category) =>
                          !category.isArchived && category.categoryKind === transactionKind,
                      )
                      .map((category) => (
                        <option key={category.id} value={category.id}>
                          {category.name}
                        </option>
                      ))}
                  </select>
                  <label htmlFor={`split-amount-${row.id}`}>Split {index + 1} amount (minor units)</label>
                  <input
                    id={`split-amount-${row.id}`}
                    inputMode="numeric"
                    onChange={(event) => updateAllocationRow(row.id, "minorUnits", event.currentTarget.value)}
                    required
                    value={row.minorUnits}
                  />
                  <button disabled={disabled} onClick={() => removeAllocationRow(row.id)} type="button">
                    Remove split {index + 1}
                  </button>
                </div>
              ))}
              <button disabled={disabled} onClick={addAllocationRow} type="button">
                Add category split
              </button>
            </fieldset>
          )}
          <label htmlFor="transaction-budget">Budget (optional)</label>
          <select id="transaction-budget" name="transactionBudgetId">
            <option value="">No budget</option>
            {budgets
              .filter((budget) => !budget.isArchived)
              .map((budget) => (
                <option key={budget.id} value={budget.id}>
                  {budget.name} ({budget.currency})
                </option>
              ))}
          </select>
          <label htmlFor="transaction-minor-units">Amount (minor units)</label>
          <input
            id="transaction-minor-units"
            inputMode="numeric"
            name="transactionMinorUnits"
            required
          />
          <label htmlFor="transaction-currency">Currency</label>
          <input
            defaultValue="USD"
            id="transaction-currency"
            maxLength={3}
            name="transactionCurrency"
            required
          />
          <label htmlFor="transaction-occurred-on">Date</label>
          <input id="transaction-occurred-on" name="transactionOccurredOn" required type="date" />
          <label htmlFor="transaction-payee">Payee (optional)</label>
          <input id="transaction-payee" maxLength={200} name="transactionPayee" />
          <label htmlFor="transaction-description">Description (optional)</label>
          <input id="transaction-description" maxLength={500} name="transactionDescription" />
          <label htmlFor="transaction-notes">Notes (optional)</label>
          <textarea id="transaction-notes" maxLength={4000} name="transactionNotes" />
          <button disabled={disabled}>Record transaction</button>
        </form>
        {lastRecordedTransaction !== null && (
          <p aria-live="polite">
            Recorded {lastRecordedTransaction.entryType} of {lastRecordedTransaction.minorUnits}{" "}
            {lastRecordedTransaction.currency} on {lastRecordedTransaction.occurredOn}
            {lastRecordedTransaction.allocations.length > 0
              ? ` across ${lastRecordedTransaction.allocations.length} categories`
              : ""}
            .
          </p>
        )}
      </section>
      <section aria-labelledby="transfers-heading" className="finance-section">
        <h2 id="transfers-heading">Transfer between accounts</h2>
        <p>Enter a positive amount. Transfers do not count as income, expenses, or budget activity.</p>
        <form onSubmit={submitTransfer}>
          <label htmlFor="transfer-source-account">From account</label>
          <select id="transfer-source-account" name="transferSourceAccountId" required>
            <option value="">Choose the source account</option>
            {accounts
              .filter((account) => !account.isArchived)
              .map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name} ({account.currency})
                </option>
              ))}
          </select>
          <label htmlFor="transfer-destination-account">To account</label>
          <select id="transfer-destination-account" name="transferDestinationAccountId" required>
            <option value="">Choose the destination account</option>
            {accounts
              .filter((account) => !account.isArchived)
              .map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name} ({account.currency})
                </option>
              ))}
          </select>
          <label htmlFor="transfer-minor-units">Amount (minor units)</label>
          <input id="transfer-minor-units" inputMode="numeric" name="transferMinorUnits" required />
          <label htmlFor="transfer-currency">Currency</label>
          <input
            defaultValue="USD"
            id="transfer-currency"
            maxLength={3}
            name="transferCurrency"
            required
          />
          <label htmlFor="transfer-occurred-on">Date</label>
          <input id="transfer-occurred-on" name="transferOccurredOn" required type="date" />
          <button disabled={disabled}>Record transfer</button>
        </form>
        {lastRecordedTransfer !== null && (
          <p aria-live="polite">
            Transferred {lastRecordedTransfer.destinationTransaction.minorUnits} {lastRecordedTransfer.destinationTransaction.currency} from one account to another on {lastRecordedTransfer.destinationTransaction.occurredOn}.
          </p>
        )}
      </section>
      <section aria-labelledby="history-heading" className="finance-section">
        <h2 id="history-heading">Transaction history</h2>
        <form onSubmit={submitHistoryFilters}>
          <label htmlFor="history-starts-on">From date</label>
          <input id="history-starts-on" name="historyStartsOn" type="date" />
          <label htmlFor="history-ends-on">To date</label>
          <input id="history-ends-on" name="historyEndsOn" type="date" />
          <label htmlFor="history-account">Account</label>
          <select id="history-account" name="historyAccountId">
            <option value="">All accounts</option>
            {accounts.map((account) => <option key={account.id} value={account.id}>{account.name}</option>)}
          </select>
          <label htmlFor="history-category">Category</label>
          <select id="history-category" name="historyCategoryId">
            <option value="">All categories</option>
            {categories.map((category) => <option key={category.id} value={category.id}>{category.name}</option>)}
          </select>
          <label htmlFor="history-entry-type">Type</label>
          <select id="history-entry-type" name="historyEntryType">
            <option value="">All types</option>
            <option value="income">Income</option>
            <option value="expense">Expense</option>
            <option value="transfer">Transfer</option>
            <option value="correction">Correction</option>
          </select>
          <label htmlFor="history-search">Search payee, description, or notes</label>
          <input id="history-search" maxLength={200} name="historySearch" />
          <button disabled={disabled}>Apply history filters</button>
        </form>
        <p aria-live="polite">{historyTotalCount} matching transaction{historyTotalCount === 1 ? "" : "s"}.</p>
        <TransactionHistoryList
          accounts={accounts}
          categories={categories}
          disabled={disabled}
          onReverse={confirmReverse}
          transactions={historyTransactions}
        />
        <div className="record-actions">
          <button
            className="secondary-button"
            disabled={disabled || historyRequest.page === 0}
            onClick={() => void loadHistory({ ...historyRequest, page: historyRequest.page - 1 })}
            type="button"
          >Previous page</button>
          <button
            className="secondary-button"
            disabled={disabled || (historyRequest.page + 1) * historyRequest.pageSize >= historyTotalCount}
            onClick={() => void loadHistory({ ...historyRequest, page: historyRequest.page + 1 })}
            type="button"
          >Next page</button>
        </div>
      </section>
    </section>
  );
}

/** Renders history with explicit text labels and only permits reversible original ledger facts. */
function TransactionHistoryList({
  accounts, categories, disabled, onReverse, transactions,
}: {
  accounts: AccountDto[];
  categories: CategoryDto[];
  disabled: boolean;
  onReverse: (transaction: TransactionDto) => void;
  transactions: TransactionDto[];
}) {
  if (transactions.length === 0) return <p>No transactions match these filters.</p>;
  return <ul className="record-list">
    {transactions.map((transaction) => {
      /** Resolves a readable label without recalculating protected financial data. */
      const account = accounts.find((candidate) => candidate.id === transaction.accountId);
      /** Retains useful category context for direct-category transactions. */
      const category = categories.find((candidate) => candidate.id === transaction.categoryId);
      return <li key={transaction.id}>
        <span>
          {transaction.occurredOn} · {transaction.entryType} · {transaction.minorUnits} {transaction.currency} · {account?.name ?? "Archived or unknown account"}
          {category ? ` · ${category.name}` : ""}{transaction.payee ? ` · ${transaction.payee}` : ""}
          {transaction.allocations.length > 0 ? ` · ${transaction.allocations.length} split categories` : ""}
        </span>
        {(transaction.entryType === "income" || transaction.entryType === "expense") && (
          <span className="record-actions"><button className="danger-button" disabled={disabled} onClick={() => onReverse(transaction)} type="button">Reverse</button></span>
        )}
      </li>;
    })}
  </ul>;
}

/** Renders each planned amount with stable text labels rather than a financial calculation. */
function BudgetAllocationList({
  allocations,
  budgets,
  categories,
  disabled,
  onRemove,
}: {
  allocations: BudgetAllocationDto[];
  budgets: BudgetDto[];
  categories: CategoryDto[];
  disabled: boolean;
  onRemove: (allocation: BudgetAllocationDto) => void;
}) {
  if (allocations.length === 0) return <p>No period plans yet.</p>;
  return (
    <ul className="record-list">
      {allocations.map((allocation) => {
        /** Preserves understandable historical labels when a reference record is now archived. */
        const budget = budgets.find((candidate) => candidate.id === allocation.budgetId);
        /** Looks up the category label without moving financial rules into the UI. */
        const category = categories.find((candidate) => candidate.id === allocation.categoryId);
        return (
          <li key={allocation.id}>
            <span>
              {budget?.name ?? "Unknown budget"} · {category?.name ?? "Unknown category"} ·{" "}
              {allocation.periodStartsOn} to {allocation.periodEndsOn} ·{" "}
              {allocation.plannedMinorUnits}
              {allocation.notes ? ` · ${allocation.notes}` : ""}
            </span>
            <span className="record-actions">
              <button
                className="danger-button"
                disabled={disabled}
                onClick={() => onRemove(allocation)}
                type="button"
              >
                Remove plan
              </button>
            </span>
          </li>
        );
      })}
    </ul>
  );
}

/** Defines the common stable fields needed to render archive and edit controls. */
interface ArchivableRecord {
  id: string;
  isArchived: boolean;
}
/** Renders an accessible text list for each protected reference-data collection. */
function RecordList<T extends ArchivableRecord>({
  records,
  describe,
  onEdit,
  onArchive,
}: {
  records: T[];
  describe: (record: T) => string;
  onEdit: (record: T) => void;
  onArchive: (record: T) => void;
}) {
  if (records.length === 0) return <p>No records yet.</p>;
  return (
    <ul className="record-list">
      {records.map((record) => (
        <li key={record.id}>
          <span>{describe(record)}</span>
          {!record.isArchived && (
            <span className="record-actions">
              <button className="secondary-button" onClick={() => onEdit(record)} type="button">
                Edit
              </button>
              <button className="danger-button" onClick={() => onArchive(record)} type="button">
                Archive
              </button>
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}
