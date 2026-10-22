import { formatTime } from "../lib/format";
import { Button } from "./ui";

/** Shown when a save was rejected because another device changed the document. */
export function ConflictBanner({ at, onTheirs, onMine }: { at: number; onTheirs: () => void; onMine: () => void }) {
  const small = "min-h-7 px-2 text-[10px]";
  return (
    <div role="alert" className="mb-3 rounded-sm border border-warn bg-sunken/60 p-3 text-sm">
      <p>This was changed on another device ({formatTime(at)}).</p>
      <div className="mt-2 flex flex-wrap gap-2">
        <Button className={small} onClick={onTheirs}>
          use theirs
        </Button>
        <Button className={small} variant="primary" onClick={onMine}>
          keep mine
        </Button>
      </div>
    </div>
  );
}
