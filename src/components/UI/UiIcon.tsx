import { useId, type ReactNode, type SVGProps } from "react";

export type UiIconName =
  | "search"
  | "settings"
  | "palette"
  | "languages"
  | "arrow-left"
  | "tag"
  | "group"
  | "info"
  | "copy"
  | "reset"
  | "window-minimize"
  | "window-maximize"
  | "window-restore"
  | "fullscreen"
  | "close"
  | "heart"
  | "check-circle"
  | "check-square"
  | "bulk-actions"
  | "grip-vertical"
  | "folder"
  | "trash";

type IconFillMode = "none" | "current";

interface IconDefinition {
  viewBox: string;
  content: ReactNode;
  defaultFillMode?: IconFillMode;
}

export interface UiIconProps extends Omit<SVGProps<SVGSVGElement>, "children" | "name"> {
  name: UiIconName;
  filled?: boolean;
  title?: string;
}

const BASE_ICON_CLASSES =
  "h-5 w-5 stroke-current [stroke-linecap:round] [stroke-linejoin:round] [stroke-width:1.8]";

const ICONS = {
  search: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <circle cx="11" cy="11" r="6" />
        <path d="M16 16L21 21" />
      </>
    )
  },
  settings: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <path d="M9.45 3.15 10 5a7.4 7.4 0 0 1 4 0l.55-1.85a1 1 0 0 1 1.32-.66l1.8.67a1 1 0 0 1 .6 1.32L17.6 6a8 8 0 0 1 2 3.45l1.9.22a1 1 0 0 1 .88 1v1.9a1 1 0 0 1-.88 1l-1.9.22A8 8 0 0 1 17.6 17l.67 1.52a1 1 0 0 1-.6 1.32l-1.8.67a1 1 0 0 1-1.32-.66L14 18.95a7.4 7.4 0 0 1-4 0l-.55 1.85a1 1 0 0 1-1.32.66l-1.8-.67a1 1 0 0 1-.6-1.32L6.4 17a8 8 0 0 1-2-3.45l-1.9-.22a1 1 0 0 1-.88-1v-1.9a1 1 0 0 1 .88-1L4.4 9.2A8 8 0 0 1 6.4 5.75l-.67-1.52a1 1 0 0 1 .6-1.32l1.8-.67a1 1 0 0 1 1.32.66Z" />
        <circle cx="12" cy="12" r="2.9" />
      </>
    )
  },
  palette: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <path d="M12 3a9 9 0 1 0 0 18h1.4a2.1 2.1 0 0 0 1.5-3.6 1.4 1.4 0 0 1 1-2.4H18a3 3 0 0 0 3-3 9 9 0 0 0-9-9Z" />
        <circle cx="7" cy="11" r="1" fill="currentColor" stroke="none" />
        <circle cx="10" cy="7" r="1" fill="currentColor" stroke="none" />
        <circle cx="15" cy="7.5" r="1" fill="currentColor" stroke="none" />
        <circle cx="17.5" cy="11" r="1" fill="currentColor" stroke="none" />
      </>
    )
  },
  languages: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <path d="M3 5h12M9 2v3M12 5c-1 5-4 8-9 11M5 8c1.5 3 4 5.5 7 7" />
        <path d="m13 21 4.5-11L22 21M15 17h5" />
      </>
    )
  },
  "arrow-left": {
    viewBox: "0 0 24 24",
    content: (
      <>
        <line x1="19" y1="12" x2="5" y2="12" />
        <path d="M11 18 5 12l6-6" />
      </>
    )
  },
  tag: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <path d="M3.5 4.5v6.35a2 2 0 0 0 .59 1.42l7.64 7.64a2 2 0 0 0 2.83 0l5.35-5.35a2 2 0 0 0 0-2.83L12.27 4.09a2 2 0 0 0-1.42-.59H4.5a1 1 0 0 0-1 1Z" />
        <circle cx="8" cy="8" r="1.15" />
      </>
    )
  },
  group: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <line x1="7.4" y1="16.6" x2="12.1" y2="7.7" />
        <line x1="12.1" y1="7.7" x2="17.1" y2="12.1" />
        <line x1="7.9" y1="16.1" x2="16.4" y2="12.8" />
        <circle cx="6.5" cy="17.5" r="2.15" fill="currentColor" stroke="none" />
        <circle cx="12.2" cy="6.9" r="2.15" fill="currentColor" stroke="none" />
        <circle cx="17.5" cy="12.6" r="2.15" fill="currentColor" stroke="none" />
      </>
    )
  },
  info: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <circle cx="12" cy="12" r="9" />
        <line x1="12" y1="10" x2="12" y2="16" />
        <circle cx="12" cy="7.5" r="1" />
      </>
    )
  },
  copy: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <rect x="9" y="9" width="10" height="10" rx="2" ry="2" />
        <path d="M15 9V7a2 2 0 0 0-2-2H7a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2" />
      </>
    )
  },
  reset: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <path d="M20 12a8 8 0 1 1-2.3-5.7" />
        <path d="M20 4v5h-5" />
      </>
    )
  },
  "window-minimize": {
    viewBox: "0 0 24 24",
    content: <path d="M5 12h14" />
  },
  "window-maximize": {
    viewBox: "0 0 24 24",
    content: <rect x="5" y="5" width="14" height="14" rx="1" />
  },
  "window-restore": {
    viewBox: "0 0 24 24",
    content: <><path d="M9 8V4h11v11h-4" /><rect x="4" y="9" width="11" height="11" rx="1" /></>
  },
  fullscreen: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <path d="M8 3H3v5" />
        <path d="M16 3h5v5" />
        <path d="M21 16v5h-5" />
        <path d="M3 16v5h5" />
      </>
    )
  },
  close: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <line x1="7" y1="7" x2="17" y2="17" />
        <line x1="17" y1="7" x2="7" y2="17" />
      </>
    )
  },
  heart: {
    viewBox: "0 0 24 24",
    content: <path d="M12 20s-7-4.35-7-10a4 4 0 0 1 7-2.5A4 4 0 0 1 19 10c0 5.65-7 10-7 10Z" />
  },
  "check-circle": {
    viewBox: "0 0 24 24",
    content: (
      <>
        <circle cx="12" cy="12" r="9" />
        <path d="m8 12 2.5 2.5L16 9" />
      </>
    )
  },
  "check-square": {
    viewBox: "0 0 24 24",
    content: (
      <>
        <rect x="4" y="4" width="16" height="16" rx="3" ry="3" />
        <path d="m8.5 12 2.4 2.4L15.8 9.5" />
      </>
    )
  },
  "bulk-actions": {
    viewBox: "0 0 24 24",
    content: (
      <>
        <rect x="3.5" y="5" width="7" height="6.5" rx="1.5" ry="1.5" />
        <rect x="13.5" y="5" width="7" height="6.5" rx="1.5" ry="1.5" />
        <rect x="3.5" y="13" width="7" height="6.5" rx="1.5" ry="1.5" />
        <path d="m14.8 16.2 2.2 2.2 3.5-4.1" />
      </>
    )
  },
  "grip-vertical": {
    viewBox: "0 0 24 24",
    content: (
      <>
        <circle cx="9" cy="5" r="1.25" fill="currentColor" stroke="none" />
        <circle cx="15" cy="5" r="1.25" fill="currentColor" stroke="none" />
        <circle cx="9" cy="12" r="1.25" fill="currentColor" stroke="none" />
        <circle cx="15" cy="12" r="1.25" fill="currentColor" stroke="none" />
        <circle cx="9" cy="19" r="1.25" fill="currentColor" stroke="none" />
        <circle cx="15" cy="19" r="1.25" fill="currentColor" stroke="none" />
      </>
    )
  },
  folder: {
    viewBox: "0 0 24 24",
    content: <path d="M3.5 7.5A2.5 2.5 0 0 1 6 5h3.2l2 2H18a2.5 2.5 0 0 1 2.5 2.5v7A2.5 2.5 0 0 1 18 19H6a2.5 2.5 0 0 1-2.5-2.5Z" />
  },
  trash: {
    viewBox: "0 0 24 24",
    content: (
      <>
        <path d="M4.5 7h15" />
        <path d="M9 3.5h6" />
        <path d="M18 7l-1 12a2 2 0 0 1-2 1.8H9a2 2 0 0 1-2-1.8L6 7" />
        <path d="M10 11v6" />
        <path d="M14 11v6" />
      </>
    )
  }
} satisfies Record<UiIconName, IconDefinition>;

function joinClasses(...classes: Array<string | undefined | false>) {
  return classes.filter(Boolean).join(" ");
}

export function UiIcon({
  name,
  className = "",
  filled,
  title,
  role,
  "aria-hidden": ariaHidden,
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
  ...svgProps
}: UiIconProps) {
  const titleId = useId();
  const { viewBox, content, defaultFillMode = "none" } = ICONS[name] as IconDefinition;
  const fillMode = filled === undefined ? defaultFillMode : filled ? "current" : "none";
  const isDecorative = !title && !role && !ariaLabel && !ariaLabelledBy;
  const resolvedAriaHidden = ariaHidden ?? (isDecorative ? true : undefined);
  const resolvedRole = role ?? (!resolvedAriaHidden ? "img" : undefined);
  const resolvedAriaLabelledBy = ariaLabelledBy ?? (title && !ariaLabel ? titleId : undefined);
  const classes = joinClasses(
    BASE_ICON_CLASSES,
    fillMode === "current" ? "fill-current" : "fill-none",
    className
  );

  return (
    <svg
      className={classes}
      viewBox={viewBox}
      role={resolvedRole}
      aria-hidden={resolvedAriaHidden}
      aria-label={ariaLabel}
      aria-labelledby={resolvedAriaLabelledBy}
      {...svgProps}
    >
      {title ? <title id={titleId}>{title}</title> : null}
      {content}
    </svg>
  );
}
