/**
 * Categories shared by tasks and reminders (DECISIONS D-049): a name, a
 * colour, and an optional icon. Independent of projects; an item can carry
 * several. Deleting one removes only its assignments.
 */

export interface Category {
  id: string;
  name: string;
  /** `#rrggbb`. */
  color: string;
  icon: string | null;
  created_at: number;
  updated_at: number;
}

export interface NewCategory {
  name: string;
  color?: string;
  icon?: string | null;
}

export interface CategoryPatch {
  name?: string;
  color?: string;
  /** `null` clears it. */
  icon?: string | null;
}

export interface Assignment {
  task_id: string;
  category_id: string;
}

/** `GET /api/v1/categories`. */
export interface CategoryListing {
  categories: Category[];
  tasks: Assignment[];
}

/** A category as cached: with the ids of the tasks that carry it. */
export interface CategoryItem extends Category {
  task_ids: string[];
}

export const MAX_NAME = 40;
export const MAX_PER_ITEM = 12;

/** Colours offered when creating a category (any `#rrggbb` is accepted). */
export const PALETTE: { color: string; label: string }[] = [
  { color: "#3b3fa8", label: "Indigo" },
  { color: "#7a4fc4", label: "Violet" },
  { color: "#2f7fc1", label: "Blue" },
  { color: "#2c8a7a", label: "Teal" },
  { color: "#2c7445", label: "Green" },
  { color: "#b7791f", label: "Amber" },
  { color: "#c2552d", label: "Orange" },
  { color: "#b23a32", label: "Red" },
  { color: "#b4407e", label: "Pink" },
  { color: "#5f6275", label: "Slate" },
];

export function fromListing(l: CategoryListing): CategoryItem[] {
  const byCategory = new Map<string, string[]>();
  for (const a of l.tasks) byCategory.set(a.category_id, [...(byCategory.get(a.category_id) ?? []), a.task_id]);
  return sortCategories(l.categories.map((c) => ({ ...c, task_ids: byCategory.get(c.id) ?? [] })));
}

export function sortCategories<T extends Category>(items: T[]): T[] {
  return [...items].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

/** The task's categories, by name. */
export function taskCategories(items: CategoryItem[] | undefined, taskId: string): CategoryItem[] {
  return (items ?? []).filter((c) => c.task_ids.includes(taskId));
}

/** The category with this name, ignoring case and surrounding spaces. */
export function byName<T extends Category>(items: T[] | undefined, name: string): T | undefined {
  const want = name.trim().toLowerCase();
  return (items ?? []).find((c) => c.name.toLowerCase() === want);
}

/** Client-side mirror of the server's checks, so errors show before saving. */
export function nameProblem(items: Category[] | undefined, name: string, self?: string): string | undefined {
  const n = name.trim();
  if (!n) return "Enter a name.";
  if (n.length > MAX_NAME) return `At most ${MAX_NAME} characters.`;
  const same = byName(items, n);
  if (same && same.id !== self) return "A category with that name exists.";
  return undefined;
}

/** Replace the task's set across all categories. */
export function withTaskCategories(items: CategoryItem[], taskId: string, ids: string[]): CategoryItem[] {
  return items.map((c) => {
    const has = c.task_ids.includes(taskId);
    const want = ids.includes(c.id);
    if (has === want) return c;
    return { ...c, task_ids: want ? [...c.task_ids, taskId] : c.task_ids.filter((t) => t !== taskId) };
  });
}

/** Category filter values: "" = all, "none" = uncategorised, otherwise an id. */
export const NO_CATEGORY = "none";

export function matchesCategory(filter: string, ids: string[]): boolean {
  if (!filter) return true;
  if (filter === NO_CATEGORY) return ids.length === 0;
  return ids.includes(filter);
}

/**
 * Group items by category for display. An item with several categories
 * appears under each; uncategorised items come last.
 */
export function groupByCategory<T>(
  items: T[],
  categories: Category[] | undefined,
  idsOf: (item: T) => string[],
): { category?: Category; items: T[] }[] {
  const groups = sortCategories(categories ?? []).map((category) => ({
    category: category as Category | undefined,
    items: items.filter((i) => idsOf(i).includes(category.id)),
  }));
  const known = new Set((categories ?? []).map((c) => c.id));
  const rest = items.filter((i) => !idsOf(i).some((id) => known.has(id)));
  return [...groups.filter((g) => g.items.length > 0), ...(rest.length ? [{ category: undefined, items: rest }] : [])];
}
