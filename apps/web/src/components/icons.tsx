import type { SVGProps } from "react";

// Directory's icon set (D-050): one authored family on a 24px grid, 1.6px
// round strokes, no fills except small dots. Shapes borrow from printed
// planners and desk objects (ring binders, tabs, a record) without becoming
// pictures. Icons are decorative by default: the label next to them names
// the action, so each renders aria-hidden.

const PATHS = {
  home: "M4 11.5 12 5l8 6.5M6.5 10v9h11v-9M10 19v-5h4v5",
  ask: "M5 12h1.5M8.5 8.5v7M12 6v12M15.5 8.5v7M18.5 12H20",
  search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13ZM15.5 15.5 20 20",
  inbox: "M4 13.5 6.5 5h11l2.5 8.5M4 13.5V19h16v-5.5M4 13.5h4.5l1 2h5l1-2H20",
  tasks: "M5 5h14v14H5zM8.5 12l2.3 2.3L15.5 9.6",
  reminders: "M7 16.5V11a5 5 0 0 1 10 0v5.5l1.5 1.5h-13zM10.5 20.5h3",
  projects: "M4 7.5h6l1.5 2H20V19H4zM4 7.5V5h5",
  calendar: "M4.5 6.5h15V19h-15zM4.5 10.5h15M8.5 4v4M15.5 4v4M8 14h2M11 14h2M14 14h2M8 16.5h2",
  habits: "M12 20a8 8 0 1 0-8-8M4 12l-1.5-2M4 12l2-1.5M9 12.5l2 2 4-4.5",
  notes: "M6 4h8.5L18 7.5V20H6zM14 4v4h4M9 12h6M9 15.5h6",
  journal: "M6.5 4H18v16H6.5A1.5 1.5 0 0 1 5 18.5v-13A1.5 1.5 0 0 1 6.5 4ZM9 4v16M3.5 8h3M3.5 12h3M3.5 16h3",
  library: "M5 5h3v14H5zM10 5h3v14h-3zM15 6.2l2.8-.8 3.2 13.2-2.8.8z",
  music: "M12 20.5a8.5 8.5 0 1 0 0-17 8.5 8.5 0 0 0 0 17ZM12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM12 6.5A5.5 5.5 0 0 0 6.5 12",
  files: "M4 6.5h5.5l1.5 2H20V19H4zM4 6.5V5h4",
  jarvis: "M12 4.5v3M12 16.5v3M4.5 12h3M16.5 12h3M7 7l1.8 1.8M15.2 15.2 17 17M17 7l-1.8 1.8M8.8 15.2 7 17",
  modules: "M5 5h5.5v5.5H5zM13.5 5H19v5.5h-5.5zM5 13.5h5.5V19H5zM13.5 13.5H19V19h-5.5z",
  settings: "M5 7h9M18 7h1M5 12h3M12 12h7M5 17h9M18 17h1M14 5v4M8 10v4M14 15v4",
  budget: "M4.5 7.5h13a2 2 0 0 1 2 2V18a1.5 1.5 0 0 1-1.5 1.5H6A1.5 1.5 0 0 1 4.5 18zM4.5 7.5 15 4.5l1 3M15 13.5h4.5",
  plus: "M12 5v14M5 12h14",
  close: "M6 6l12 12M18 6 6 18",
  check: "M5 12.5 9.5 17 19 7",
  chevronLeft: "M14.5 6 8.5 12l6 6",
  chevronRight: "M9.5 6l6 6-6 6",
  chevronDown: "M6 9.5l6 6 6-6",
  arrowUp: "M12 19V5M6 11l6-6 6 6",
  arrowDown: "M12 5v14M6 13l6 6 6-6",
  play: "M8 5.5v13l10.5-6.5z",
  pause: "M8 5.5v13M16 5.5v13",
  prev: "M7 6v12M18 6l-8 6 8 6z",
  next: "M17 6v12M6 6l8 6-8 6z",
  volume: "M5 9.5h3l4.5-4v13L8 14.5H5zM16 9a4 4 0 0 1 0 6M18.5 6.5a7.5 7.5 0 0 1 0 11",
  mute: "M5 9.5h3l4.5-4v13L8 14.5H5zM16 9.5l5 5M21 9.5l-5 5",
  sync: "M19 8.5A7.5 7.5 0 0 0 5.5 9M5 15.5A7.5 7.5 0 0 0 18.5 15M19 4.5v4h-4M5 19.5v-4h4",
  offline: "M3.5 3.5l17 17M8.5 8.6A7 7 0 0 0 6.6 13H6a3.5 3.5 0 0 0 0 7h11.5M12.6 6.1A6.5 6.5 0 0 1 18.4 11.2a4.2 4.2 0 0 1 2.2 6.3",
  cloud: "M7 19h10.5a4 4 0 0 0 .5-8A6 6 0 0 0 6.5 9.6 4.7 4.7 0 0 0 7 19Z",
  keyboard: "M3.5 6.5h17v11h-17zM7 10h1M10.5 10h1M14 10h1M17 10h.5M7 13.5h1M17 13.5h.5M10 14h4.5",
  help: "M12 20.5a8.5 8.5 0 1 0 0-17 8.5 8.5 0 0 0 0 17ZM9.7 9.5a2.4 2.4 0 1 1 3.4 2.2c-.7.3-1.1.9-1.1 1.6v.7M12 16.6v.4",
  panel: "M4 5h16v14H4zM14.5 5v14",
  split: "M4 5h16v14H4zM12 5v14",
  grip: "M9 6.5h.01M15 6.5h.01M9 12h.01M15 12h.01M9 17.5h.01M15 17.5h.01",
  eyeOff: "M4 4l16 16M9.9 6.2A9.6 9.6 0 0 1 12 6c5 0 8.5 6 8.5 6a14.8 14.8 0 0 1-2.6 3.3M6.4 7.9A15 15 0 0 0 3.5 12s3.5 6 8.5 6a8.4 8.4 0 0 0 3.6-.8M10 10.2a2.6 2.6 0 0 0 3.7 3.7",
  resize: "M14 5h5v5M10 19H5v-5M19 5l-6 6M5 19l6-6",
  sound: "M5 9.5h3l4.5-4v13L8 14.5H5zM16 9a4 4 0 0 1 0 6",
  edit: "M5 19h4L19.5 8.5l-4-4L5 15zM13.5 6.5l4 4",
  trash: "M5 7h14M9.5 7V5h5v2M7 7l1 12.5h8L17 7",
  download: "M12 4.5v10.5M7.5 10.5 12 15l4.5-4.5M5 19.5h14",
  upload: "M12 15V4.5M7.5 9 12 4.5 16.5 9M5 19.5h14",
  repeat: "M5 11V9.5A2.5 2.5 0 0 1 7.5 7H18M15 4l3 3-3 3M19 13v1.5a2.5 2.5 0 0 1-2.5 2.5H6M9 20l-3-3 3-3",
  transfer: "M5 8.5h13M14.5 5 18 8.5 14.5 12M19 15.5H6M9.5 12 6 15.5 9.5 19",
  spark: "M12 4l1.6 4.8L18.5 10.5l-4.9 1.7L12 17l-1.6-4.8L5.5 10.5l4.9-1.7z",
  sun: "M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8ZM12 3v1.5M12 19.5V21M3 12h1.5M19.5 12H21M5.6 5.6l1.1 1.1M17.3 17.3l1.1 1.1M5.6 18.4l1.1-1.1M17.3 6.7l1.1-1.1",
  more: "M6 12h.01M12 12h.01M18 12h.01",
  clock: "M12 20.5a8.5 8.5 0 1 0 0-17 8.5 8.5 0 0 0 0 17ZM12 7.5V12l3 2",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18, className = "", ...rest }: { name: IconName; size?: number } & Omit<SVGProps<SVGSVGElement>, "name">) {
  return (
    <svg
      aria-hidden
      focusable="false"
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
      {...rest}
    >
      <path d={PATHS[name]} />
    </svg>
  );
}

export const ICON_NAMES = Object.keys(PATHS) as IconName[];
