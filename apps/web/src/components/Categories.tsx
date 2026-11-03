import { useState, type FormEvent } from "react";
import type { CategoriesState } from "../hooks/useCategories";
import { MAX_NAME, MAX_PER_ITEM, PALETTE, nameProblem, type Category } from "../lib/categories";
import { Button, inputClass } from "./ui";

// Shared task and reminder categories (D-049), restyled in the D-050 pass:
// built from the existing chips, pills, and inputs.

const small = "min-h-7 px-2 text-[10px]";

function Dot({ color }: { color: string }) {
  return <span aria-hidden className="inline-block size-2 shrink-0 rounded-full" style={{ backgroundColor: color }} />;
}

/** Compact, read-only category labels for a row's meta line. */
export function CategoryChips({ categories }: { categories: Category[] }) {
  if (categories.length === 0) return null;
  return (
    <>
      {categories.map((c) => (
        <span key={c.id} className="inline-flex items-center gap-1">
          <Dot color={c.color} />
          {c.icon && <span aria-hidden>{c.icon}</span>}
          {c.name}
        </span>
      ))}
    </>
  );
}

/** Toggle chips for an item's categories, with inline create. */
export function CategoryPicker(props: {
  state: CategoriesState;
  selected: string[];
  disabled: boolean;
  onChange: (ids: string[]) => void;
}) {
  const { state, selected, disabled, onChange } = props;
  const [name, setName] = useState("");
  const problem = name.trim() ? nameProblem(state.categories, name) : undefined;
  const full = selected.length >= MAX_PER_ITEM;

  const toggle = (id: string) => onChange(selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id]);
  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || problem || full) return;
    const created = await state.create({ name: name.trim(), color: PALETTE[(state.categories?.length ?? 0) % PALETTE.length].color });
    if (created) {
      setName("");
      onChange([...selected, created.id]);
    }
  };

  return (
    <div>
      <p className="label mb-1">categories</p>
      {state.categories === undefined ? (
        <p className="text-sm text-muted">{state.loading ? "Loading categories…" : "Categories couldn't be loaded."}</p>
      ) : (
        <div role="group" aria-label="Categories" className="mb-2 flex flex-wrap gap-1.5">
          {state.categories.length === 0 && <span className="text-sm text-muted">No categories yet. Add one below.</span>}
          {state.categories.map((c) => {
            const on = selected.includes(c.id);
            return (
              <button
                key={c.id}
                type="button"
                aria-pressed={on}
                disabled={disabled || (!on && full)}
                onClick={() => toggle(c.id)}
                className={`inline-flex min-h-7 items-center gap-1.5 rounded-full border px-2.5 text-[12px] disabled:opacity-50 ${
                  on ? "border-accent bg-sunken text-ink" : "border-line text-muted hover:text-ink"
                }`}
              >
                <Dot color={c.color} />
                {c.icon && <span aria-hidden>{c.icon}</span>}
                {c.name}
              </button>
            );
          })}
        </div>
      )}
      <form onSubmit={(e) => void add(e)} className="flex gap-2">
        <input
          value={name}
          disabled={disabled || state.categories === undefined}
          maxLength={MAX_NAME}
          onChange={(e) => setName(e.target.value)}
          placeholder="New category…"
          aria-label="New category"
          className={`${inputClass} min-w-0 flex-1`}
        />
        <Button type="submit" className={small} disabled={disabled || !name.trim() || !!problem || full}>
          Add
        </Button>
      </form>
      {problem && <p className="mt-1 text-xs text-bad">{problem}</p>}
      {full && <p className="mt-1 text-xs text-muted">At most {MAX_PER_ITEM} categories per item.</p>}
      {state.error && (
        <p role="alert" className="mt-1 text-xs text-bad">
          {state.error}
        </p>
      )}
    </div>
  );
}

/** Create, rename, recolour, and delete categories. */
export function CategoryManager({ state, disabled }: { state: CategoriesState; disabled: boolean }) {
  const [name, setName] = useState("");
  const [color, setColor] = useState(PALETTE[0].color);
  const problem = name.trim() ? nameProblem(state.categories, name) : undefined;

  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || problem) return;
    if (await state.create({ name: name.trim(), color })) setName("");
  };

  return (
    <div className="flex flex-col gap-3">
      {state.loading && !state.categories ? (
        <p className="text-sm text-muted">Loading categories…</p>
      ) : !state.categories ? (
        <div className="text-sm">
          <p className="text-muted">Categories couldn't be loaded.</p>
          <Button className={`${small} mt-2`} onClick={state.reload}>
            Try again
          </Button>
        </div>
      ) : state.categories.length === 0 ? (
        <p className="text-sm text-muted">No categories yet. They work across tasks and reminders, separately from projects.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {state.categories.map((c) => (
            <CategoryEditRow key={`${c.id}:${c.updated_at}`} category={c} count={c.task_ids.length} state={state} disabled={disabled} />
          ))}
        </ul>
      )}
      <form onSubmit={(e) => void add(e)} className="flex flex-wrap items-center gap-2 border-t border-line pt-3">
        <ColorSelect value={color} disabled={disabled} onChange={setColor} label="New category colour" />
        <input
          value={name}
          disabled={disabled}
          maxLength={MAX_NAME}
          onChange={(e) => setName(e.target.value)}
          placeholder="New category…"
          aria-label="New category name"
          className={`${inputClass} min-w-0 flex-1`}
        />
        <Button type="submit" className={small} disabled={disabled || !name.trim() || !!problem}>
          Add
        </Button>
      </form>
      {problem && <p className="text-xs text-bad">{problem}</p>}
      {state.error && (
        <p role="alert" className="text-sm text-bad">
          {state.error}
        </p>
      )}
    </div>
  );
}

function ColorSelect({ value, disabled, onChange, label }: { value: string; disabled: boolean; onChange: (c: string) => void; label: string }) {
  const known = PALETTE.some((p) => p.color === value);
  return (
    <span className="flex items-center gap-1.5">
      <Dot color={value} />
      <select value={value} disabled={disabled} aria-label={label} onChange={(e) => onChange(e.target.value)} className={`${inputClass} min-h-9 text-[13px]`}>
        {!known && <option value={value}>{value}</option>}
        {PALETTE.map((p) => (
          <option key={p.color} value={p.color}>
            {p.label}
          </option>
        ))}
      </select>
    </span>
  );
}

function CategoryEditRow({ category, count, state, disabled }: { category: Category; count: number; state: CategoriesState; disabled: boolean }) {
  const [name, setName] = useState(category.name);
  const [icon, setIcon] = useState(category.icon ?? "");
  const [confirm, setConfirm] = useState(false);
  const problem = nameProblem(state.categories, name, category.id);

  const saveName = () => {
    if (problem || name.trim() === category.name) return setName(category.name);
    void state.update(category.id, { name: name.trim() });
  };
  const saveIcon = () => {
    if (icon.trim() !== (category.icon ?? "")) void state.update(category.id, { icon: icon.trim() || null });
  };

  return (
    <li className="flex flex-wrap items-center gap-2 border-b border-line pb-2 last:border-b-0">
      <input
        value={name}
        disabled={disabled}
        maxLength={MAX_NAME}
        onChange={(e) => setName(e.target.value)}
        onBlur={saveName}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
        aria-label={`Name of ${category.name}`}
        className={`${inputClass} w-full`}
      />
      <ColorSelect value={category.color} disabled={disabled} onChange={(color) => void state.update(category.id, { color })} label={`Colour of ${category.name}`} />
      <input
        value={icon}
        disabled={disabled}
        maxLength={8}
        onChange={(e) => setIcon(e.target.value)}
        onBlur={saveIcon}
        placeholder="icon"
        aria-label={`Icon of ${category.name}`}
        className={`${inputClass} w-16 text-center`}
      />
      <span className="flex-1 font-mono text-[11px] text-muted">
        {count} {count === 1 ? "task" : "tasks"}
      </span>
      {confirm ? (
        <span className="flex gap-1.5">
          <Button className={small} onClick={() => setConfirm(false)}>
            Keep
          </Button>
          <Button variant="danger" className={small} onClick={() => void state.remove(category.id)} aria-label={`Confirm delete ${category.name}`}>
            Confirm delete
          </Button>
        </span>
      ) : (
        <Button variant="danger" className={small} disabled={disabled} onClick={() => setConfirm(true)} aria-label={`Delete ${category.name}`}>
          Delete
        </Button>
      )}
    </li>
  );
}
