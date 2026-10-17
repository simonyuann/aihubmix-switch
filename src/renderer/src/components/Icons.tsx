import type { ReactNode, SVGProps } from 'react'

type IconProps = SVGProps<SVGSVGElement> & { size?: number }

function Base({ size = 18, children, ...props }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  )
}

export function SparkIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 3l1.7 5.1L19 10l-5.3 1.9L12 17l-1.7-5.1L5 10l5.3-1.9L12 3Z" />
      <path d="M19 15.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8.8-2.2Z" />
    </Base>
  )
}

export function KeyIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="8" cy="15" r="3.2" />
      <path d="M10.4 12.6 19 4M15.5 7.5l2 2M13.5 9.5l2 2" />
    </Base>
  )
}

export function EyeIcon({ open = false, ...props }: IconProps & { open?: boolean }) {
  return (
    <Base {...props}>
      {open ? (
        <>
          <path d="M2.5 12S6 5.8 12 5.8 21.5 12 21.5 12 18 18.2 12 18.2 2.5 12 2.5 12Z" />
          <circle cx="12" cy="12" r="3" />
        </>
      ) : (
        <>
          <path d="M3 3l18 18" />
          <path d="M10.6 6.1A9.6 9.6 0 0 1 12 6c6 0 9.5 6 9.5 6a17 17 0 0 1-3 3.7M6.2 6.7A17 17 0 0 0 2.5 12S6 18 12 18c1 0 1.9-.2 2.8-.5" />
          <path d="M9.9 9.9a3 3 0 0 0 4.2 4.2" />
        </>
      )}
    </Base>
  )
}

export function CheckIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M4 12.5l5 5L20 6.5" />
    </Base>
  )
}

export function CopyIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="9" y="9" width="11" height="11" rx="2.5" />
      <path d="M15 9V6.5A2.5 2.5 0 0 0 12.5 4h-6A2.5 2.5 0 0 0 4 6.5v6A2.5 2.5 0 0 0 6.5 15H9" />
    </Base>
  )
}

export function FolderIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M3.5 7.5A2.5 2.5 0 0 1 6 5h4l2 2h6a2.5 2.5 0 0 1 2.5 2.5v7A2.5 2.5 0 0 1 18 19H6a2.5 2.5 0 0 1-2.5-2.5v-9Z" />
    </Base>
  )
}

export function RefreshIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M20 11a8 8 0 0 0-14.8-3M4 5v4h4" />
      <path d="M4 13a8 8 0 0 0 14.8 3M20 19v-4h-4" />
    </Base>
  )
}

export function SettingsIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06-1.8 1.8-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V20h-2.54v-.09a1.7 1.7 0 0 0-1.03-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06-1.8-1.8.06-.06A1.7 1.7 0 0 0 8.4 15a1.7 1.7 0 0 0-1.56-1.03H6.8v-2.54h.09A1.7 1.7 0 0 0 8.4 10.4a1.7 1.7 0 0 0-.34-1.87l-.06-.06 1.8-1.8.06.06a1.7 1.7 0 0 0 1.87.34A1.7 1.7 0 0 0 12.76 5.5V5.4h2.54v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06 1.8 1.8-.06.06a1.7 1.7 0 0 0-.34 1.87 1.7 1.7 0 0 0 1.56 1.03h.09v2.54h-.09A1.7 1.7 0 0 0 19.4 15Z" />
    </Base>
  )
}

export function UserIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="12" cy="8" r="3.6" />
      <path d="M4.5 20a7.5 7.5 0 0 1 15 0" />
    </Base>
  )
}

export function ExternalIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M14 4h6v6M20 4l-9 9" />
      <path d="M18 14v4.5A1.5 1.5 0 0 1 16.5 20h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 5.5 6H10" />
    </Base>
  )
}

export function AlertIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 3.5 21 20H3L12 3.5Z" />
      <path d="M12 9v4.5M12 17h.01" />
    </Base>
  )
}

export function ShieldIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 3l7 2.8v5.4c0 4.5-2.9 7.7-7 9.8-4.1-2.1-7-5.3-7-9.8V5.8L12 3Z" />
      <path d="M9 12l2 2 4-4" />
    </Base>
  )
}

export function LogoutIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M10 5H6.5A1.5 1.5 0 0 0 5 6.5v11A1.5 1.5 0 0 0 6.5 19H10" />
      <path d="M15 8l4 4-4 4M19 12H9" />
    </Base>
  )
}

export function LoaderIcon({ size = 18, ...props }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      className="spin"
      {...props}
    >
      <circle cx="12" cy="12" r="8.5" stroke="currentColor" strokeOpacity=".22" strokeWidth="2.4" />
      <path
        d="M20.5 12a8.5 8.5 0 0 0-8.5-8.5"
        stroke="currentColor"
        strokeWidth="2.4"
        strokeLinecap="round"
      />
    </svg>
  )
}

export function ChevronIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M6 9.5l6 6 6-6" />
    </Base>
  )
}

export function TerminalIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="4.5" width="18" height="15" rx="2.5" />
      <path d="M7 9l3 3-3 3M13 15h4" />
    </Base>
  )
}

export function TrashIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M4 7h16M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7" />
      <path d="M6.5 7l.8 12A1.5 1.5 0 0 0 8.8 20.5h6.4a1.5 1.5 0 0 0 1.5-1.5l.8-12" />
      <path d="M10 11v5M14 11v5" />
    </Base>
  )
}

export function PlusIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 5v14M5 12h14" />
    </Base>
  )
}
