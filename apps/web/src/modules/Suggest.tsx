import { useCallback, useEffect, useState } from "react";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import { formatMinutes, localDate, type PlanPick } from "../lib/tasks";

/**
 * Paper "JARVIS suggests" hero (Desktop/Mobile · Home): the deterministic
 * planner's top picks for today (GET /tasks/plan, read-only, D-041). "Not now"
 * moves to the next pick; nothing is changed until the task itself is opened.
 */
export function SuggestCard({ online }: { online: boolean }) {
  const [picks, setPicks] = useState<PlanPick[]>();
  const [index, setIndex] = useState(0);
  const [failed, setFailed] = useState(false);

  const load = useCallback(async () => {
    setFailed(false);
    try {
      const plan = await api.planTasks(localDate());
      setPicks(plan.picks.slice(0, 3));
      setIndex(0);
    } catch (e) {
      reportIfUnauthorized(e);
      setFailed(true);
    }
  }, []);

  useEffect(() => {
    if (online) void load();
  }, [online, load]);

  const pick = picks?.[index];
  const reasons = pick?.factors.map((f) => f.reason).slice(0, 2).join(" · ");

  return (
    <section aria-label="JARVIS suggests" className="bg-tidal flex min-h-48 flex-col rounded-lg p-6 md:p-7">
      <header className="flex items-center justify-between font-mono text-[11px] tracking-[0.14em] uppercase opacity-90">
        <span>JARVIS suggests</span>
        {picks && picks.length > 1 && pick && (
          <span>
            {index + 1} of {picks.length}
          </span>
        )}
      </header>

      {!online ? (
        <p className="mt-4 text-xl leading-snug">Suggestions need a connection. Your tasks still work offline.</p>
      ) : failed ? (
        <div className="mt-4">
          <p className="text-xl leading-snug">JARVIS couldn't plan your day just now.</p>
          <div className="mt-5">
            <HeroButton onClick={() => void load()}>Try again</HeroButton>
          </div>
        </div>
      ) : !picks ? (
        <p className="mt-4 text-xl leading-snug opacity-80" aria-busy="true">
          Looking at your day…
        </p>
      ) : !pick ? (
        <div className="mt-4">
          <p className="text-xl leading-snug">{picks.length === 0 ? "Nothing open. Nice." : "That's everything for now."}</p>
          <div className="mt-5 flex flex-wrap gap-2">
            <HeroButton href="#/ask" solid>
              Ask JARVIS
            </HeroButton>
            {picks.length > 0 && <HeroButton onClick={() => setIndex(0)}>Start over</HeroButton>}
          </div>
        </div>
      ) : (
        <div className="mt-4 flex flex-1 flex-col">
          <p className="text-xl leading-snug md:text-[22px]">
            Start with “{pick.title}”{pick.estimated ? `, about ${formatMinutes(pick.minutes)}` : ""}.
          </p>
          {reasons && <p className="mt-2 text-sm opacity-85">{reasons}</p>}
          <div className="mt-auto flex flex-wrap gap-2 pt-5">
            <HeroButton href={`#/tasks?task=${encodeURIComponent(pick.task_id)}`} solid>
              Open task
            </HeroButton>
            <HeroButton onClick={() => setIndex((i) => i + 1)}>Not now</HeroButton>
          </div>
        </div>
      )}
    </section>
  );
}

/** White pill on the tidal gradient (Paper "Review plan" / "Not now"). */
function HeroButton({ children, href, onClick, solid }: { children: string; href?: string; onClick?: () => void; solid?: boolean }) {
  const cls = `inline-flex min-h-9 items-center rounded-full border px-4 text-[13px] font-medium transition ${
    solid ? "border-white bg-white text-[#3b3fa8] hover:bg-white/90" : "border-white/60 text-white hover:bg-white/10"
  }`;
  return href ? (
    <a href={href} className={cls}>
      {children}
    </a>
  ) : (
    <button type="button" onClick={onClick} className={`${cls} mt-0`}>
      {children}
    </button>
  );
}
