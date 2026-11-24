import { useEffect, useState } from "react";
import type { AccountDto, BudgetDto, CategoryDto } from "../api/contracts";
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
  /** Holds a safe UI error and never raw backend diagnostics. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  /** Prevents duplicate finance submissions while a command is running. */
  const [isSaving, setIsSaving] = useState(false);

  /** Loads all reference data together after the profile becomes authenticated. */
  async function refresh() {
    const [nextBudgets, nextCategories, nextAccounts] = await Promise.all([
      commandClient.listBudgets(),
      commandClient.listCategories(),
      commandClient.listAccounts(),
    ]);
    setBudgets(nextBudgets);
    setCategories(nextCategories);
    setAccounts(nextAccounts);
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
    </section>
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
