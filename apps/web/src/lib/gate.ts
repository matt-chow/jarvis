/**
 * Orders async loads against local changes: a load may commit only if
 * nothing newer happened since it started (another load, or a local edit).
 * Without it, a slow response can overwrite an offline edit or a newer
 * response (D-039).
 */
export interface Gate {
  /** Start a load; pass the token to `current` when it resolves. */
  begin(): number;
  /** Record a local change; loads already in flight become stale. */
  bump(): void;
  current(token: number): boolean;
}

export function createGate(): Gate {
  let generation = 0;
  return {
    begin: () => ++generation,
    bump: () => void ++generation,
    current: (token) => token === generation,
  };
}
