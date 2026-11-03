/**
 * The turntable (D-050): a drawing tied to real playback. The record turns
 * only while audio plays (and stays still under reduced motion; the music
 * keeps playing), and the tonearm tracks the position in the song. Purely
 * visual: every control lives in ordinary buttons and sliders beside it.
 */

export function Turntable({ playing, progress, label, size = 280 }: { playing: boolean; progress: number; label: string; size?: number }) {
  const p = Number.isFinite(progress) ? Math.min(1, Math.max(0, progress)) : 0;
  // Rest off the record; on the outer groove at 0; near the label at 1.
  const arm = playing || p > 0 ? 14 + p * 20 : 0;
  // A stable colour per track for the label.
  const hue = [...label].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 17);

  return (
    <svg viewBox="0 0 300 260" width={size} height={(size * 260) / 300} className="max-w-full" aria-hidden focusable="false">
      {/* plinth */}
      <rect x="4" y="4" width="292" height="252" rx="14" fill="var(--color-sunken)" stroke="var(--color-line)" />
      <rect x="14" y="14" width="272" height="232" rx="9" fill="none" stroke="var(--color-line)" strokeDasharray="1 5" />
      {/* platter and record */}
      <circle cx="122" cy="130" r="108" fill="var(--color-line)" />
      <g className="turntable-record" data-playing={playing || undefined} style={{ transformOrigin: "122px 130px" }}>
        <circle cx="122" cy="130" r="102" fill="#16151a" />
        {[94, 86, 78, 70, 62, 54].map((r) => (
          <circle key={r} cx="122" cy="130" r={r} fill="none" stroke="#2a2930" strokeWidth="1" />
        ))}
        {/* sheen, so the turning is visible */}
        <path d="M122 30a100 100 0 0 1 70 29" stroke="#3a3942" strokeWidth="5" fill="none" strokeLinecap="round" opacity="0.7" />
        <path d="M122 230a100 100 0 0 1-70-29" stroke="#3a3942" strokeWidth="5" fill="none" strokeLinecap="round" opacity="0.5" />
        <circle cx="122" cy="130" r="34" fill={`hsl(${hue} 42% 52%)`} />
        <circle cx="122" cy="130" r="34" fill="none" stroke="#00000033" />
        <path d="M100 124h44M104 136h36" stroke="#ffffff55" strokeWidth="2" strokeLinecap="round" />
        <circle cx="122" cy="130" r="3.5" fill="var(--color-sunken)" />
      </g>
      {/* tonearm, pivoting at the top right */}
      <circle cx="256" cy="46" r="15" fill="var(--color-panel)" stroke="var(--color-line)" />
      <g
        className="turntable-arm"
        style={{
          transform: `rotate(${arm}deg)`,
          transformOrigin: "256px 46px",
        }}
      >
        <path d="M256 46 L256 168 L232 196" fill="none" stroke="var(--color-muted)" strokeWidth="5" strokeLinecap="round" strokeLinejoin="round" />
        <rect x="222" y="190" width="20" height="13" rx="2" transform="rotate(42 232 196)" fill="var(--color-ink)" />
        <circle cx="256" cy="46" r="6" fill="var(--color-muted)" />
      </g>
      {/* speed indicator */}
      <circle cx="262" cy="222" r="5" fill={playing ? "var(--color-accent)" : "var(--color-line)"} />
    </svg>
  );
}
