import type { SVGProps } from 'react';

export type IconName =
  | 'board'
  | 'calendar'
  | 'capture'
  | 'folder'
  | 'plus'
  | 'refresh'
  | 'check'
  | 'chevron'
  | 'move'
  | 'trash'
  | 'command'
  | 'close'
  | 'alert'
  | 'arrow'
  | 'target'
  | 'half'
  | 'panel-right'
  | 'panel-right-collapsed'
  | 'panel-bottom'
  | 'panel-bottom-collapsed'
  | 'minus'
  | 'settings';

const paths: Record<IconName, React.ReactNode> = {
  board: (
    <>
      <rect x="3" y="4" width="7" height="16" rx="1" />
      <rect x="14" y="4" width="7" height="10" rx="1" />
    </>
  ),
  calendar: (
    <>
      <rect x="3" y="5" width="18" height="16" rx="2" />
      <path d="M8 3v4M16 3v4M3 10h18M8 14h.01M12 14h.01M16 14h.01M8 18h.01M12 18h.01" />
    </>
  ),
  capture: (
    <>
      <path d="M5 4h14v16H5zM8 8h8M8 12h8M8 16h5" />
      <path d="M3 7V2h5M21 17v5h-5" />
    </>
  ),
  folder: <path d="M3 6h7l2 2h9v11H3zM3 10h18" />,
  plus: <path d="M12 5v14M5 12h14" />,
  refresh: <path d="M20 7v5h-5M4 17v-5h5M6.1 9a7 7 0 0 1 11.6-2L20 12M4 12l2.3 5a7 7 0 0 0 11.6-2" />,
  check: <path d="m5 12 4 4L19 6" />,
  chevron: <path d="m8 10 4 4 4-4" />,
  move: <path d="M12 3v18M3 12h18M12 3l-3 3M12 3l3 3M21 12l-3-3M21 12l-3 3M12 21l-3-3M12 21l3-3M3 12l3-3M3 12l3 3" />,
  trash: <path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" />,
  command: <path d="M9 7V5a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v14a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z" />,
  close: <path d="m6 6 12 12M18 6 6 18" />,
  alert: (
    <>
      <path d="M12 3 2.8 20h18.4z" />
      <path d="M12 9v5M12 17h.01" />
    </>
  ),
  arrow: <path d="M5 12h14M14 7l5 5-5 5" />,
  target: (
    <>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="4.5" />
      <path d="M12 3v3M12 18v3M3 12h3M18 12h3" />
    </>
  ),
  // Half-filled disc: the ◐ partial-progress token drawn in the icon stroke.
  half: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 3.5v17A8.5 8.5 0 0 0 12 3.5z" fill="currentColor" stroke="none" />
    </>
  ),
  'panel-right': (
    <>
      <rect x="3" y="4" width="18" height="16" rx="1.5" />
      <path d="M15 4v16" />
    </>
  ),
  'panel-right-collapsed': (
    <>
      <rect x="3" y="4" width="18" height="16" rx="1.5" />
      <path d="M15 4v16" />
      <path d="M17 12h2M8 12h4M10 9.5 12 12l-2 2.5" />
    </>
  ),
  'panel-bottom': (
    <>
      <rect x="3" y="4" width="18" height="16" rx="1.5" />
      <path d="M3 15h18" />
    </>
  ),
  'panel-bottom-collapsed': (
    <>
      <rect x="3" y="4" width="18" height="16" rx="1.5" />
      <path d="M3 15h18" />
      <path d="M12 7v4M9.5 9l2.5 2.5L14.5 9" />
    </>
  ),
  minus: <path d="M6 12h12" />,
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" />
    </>
  ),
};

export function Icon({ name, ...props }: { name: IconName } & SVGProps<SVGSVGElement>) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
    >
      {paths[name]}
    </svg>
  );
}
