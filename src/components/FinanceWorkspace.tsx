import { useEffect, useState } from "react";
import type {
  AccountDto,
  BudgetAllocationDto,
  BudgetDto,
  CategoryDto,
  TransactionDto,
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
  /** Shows the Rust-normalized result of the current entry without implementing history yet. */
  const [lastRecordedTransaction, setLastRecordedTransaction] = useState<TransactionDto | null>(
    null,
  );
  /** Holds a safe UI error and never raw backend diagnostics. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  /** Prevents duplicate finance submissions while a command is running. */
  const [isSaving, setIsSaving] = useState(false);

  /** Loads all reference data together after the profile becomes authenticated. */
  async function refresh() {
    const [nextBudgets, nextCategories, nextAccounts, nextBudgetAllocations] = await Promise.all([
      commandClient.listBudgets(),
      commandClient.listCategories(),
      commandClient.listAccounts(),
      commandClient.listBudgetAllocations(),
    ]);
    setBudgets(nextBudgets);
    setCategories(nextCategories);
    setAccounts(nextAccounts);
    setBudgetAllocations(nextBudgetAllocations);
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
      });
      setLastRecordedTransaction(transaction);
      form.reset();
      setTransactionKind("expense");
    });
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
            onChange={(event) =>
              setTransactionKind(event.currentTarget.value as "income" | "expense")
            }
            value={transactionKind}
          >
            <option value="expense">Expense</option>
            <option value="income">Income</option>
          </select>
          <label htmlFor="transaction-category">Category (optional)</label>
          <select id="transaction-category" name="transactionCategoryId">
            <option value="">No category</option>
            {categories
              .filter(
                (category) => !category.isArchived && category.categoryKind === transactionKind,
              )
              .map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name}
                </option>
              ))}
          </select>
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
            {lastRecordedTransaction.currency} on {lastRecordedTransaction.occurredOn}.
          </p>
        )}
      </section>
    </section>
  );
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
