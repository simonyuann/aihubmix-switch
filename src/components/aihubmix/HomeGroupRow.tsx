import { useTranslation } from "react-i18next";
import { useEffect, useId, useRef, useState } from "react";
import type { ComponentProps, ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { isKeyboardModality } from "@/lib/inputModality";
import { HomeImageModelSelect } from "./HomeImageModelSelect";

type ModelSettings = Omit<
  ComponentProps<typeof HomeImageModelSelect>,
  "open" | "onOpenChange"
>;

export function HomeGroupRow({
  name,
  icon,
  activeLabel,
  status,
  modelSettings,
  children,
}: {
  name: string;
  icon: ReactNode;
  activeLabel?: string;
  status: string;
  modelSettings?: ModelSettings;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const row = useRef<HTMLElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const [hovered, setHovered] = useState(false);
  const [pinnedOpen, setPinnedOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const expanded = hovered || pinnedOpen || menuOpen;
  const hasModelSettings = modelSettings !== undefined;

  useEffect(() => {
    const element = row.current;
    if (!element || !hasModelSettings) return;
    // Native boundaries treat the portalled menu as separate from the card.
    const enter = (event: PointerEvent) => {
      if (event.pointerType !== "touch") setHovered(true);
    };
    const leave = (event: PointerEvent) => {
      if (event.pointerType !== "touch") setHovered(false);
    };
    element.addEventListener("pointerenter", enter);
    element.addEventListener("pointerleave", leave);
    return () => {
      element.removeEventListener("pointerenter", enter);
      element.removeEventListener("pointerleave", leave);
    };
  }, [hasModelSettings]);

  useEffect(() => {
    if (!pinnedOpen || menuOpen) return;
    const dismissOutside = (event: PointerEvent) => {
      if (event.target instanceof Node && !row.current?.contains(event.target))
        setPinnedOpen(false);
    };
    document.addEventListener("pointerdown", dismissOutside, true);
    return () =>
      document.removeEventListener("pointerdown", dismissOutside, true);
  }, [pinnedOpen, menuOpen]);

  return (
    <article
      ref={row}
      className="aihub-group-row"
      data-active={activeLabel ? "" : undefined}
      data-expanded={modelSettings ? expanded : undefined}
      onPointerDown={
        modelSettings
          ? (event) => {
              if (event.pointerType !== "touch") setPinnedOpen(false);
            }
          : undefined
      }
      onFocus={
        modelSettings
          ? () => {
              if (isKeyboardModality()) setPinnedOpen(true);
            }
          : undefined
      }
      onBlur={
        modelSettings
          ? (event) => {
              if (!event.currentTarget.contains(event.relatedTarget))
                setPinnedOpen(false);
            }
          : undefined
      }
    >
      <div className="aihub-group-summary">
        {icon}
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <h3 className="min-w-0 truncate text-[15px] font-semibold text-fg-1">
              {modelSettings ? (
                <button
                  type="button"
                  aria-label={t("homeGroupRow.aria.imageModel", { name })}
                  aria-expanded={expanded}
                  aria-controls={panelId}
                  className="aihub-group-disclosure"
                  onClick={() => {
                    if (isKeyboardModality() && pinnedOpen) {
                      panel.current
                        ?.querySelector<HTMLButtonElement>('[role="combobox"]')
                        ?.focus();
                    } else if (!hovered) {
                      setPinnedOpen(!pinnedOpen);
                    }
                  }}
                >
                  <span className="truncate">{name}</span>
                  <ChevronDown aria-hidden="true" size={13} />
                </button>
              ) : (
                name
              )}
            </h3>
            {activeLabel && (
              <span className="aihub-active-group text-caption font-medium text-success-text">
                {activeLabel}
              </span>
            )}
          </div>
          <p className="mt-1 text-caption text-fg-3">{status}</p>
        </div>
        {children}
      </div>
      {modelSettings && (
        <div
          ref={panel}
          id={panelId}
          className="aihub-image-reveal"
          aria-hidden={!expanded}
          {...(expanded ? {} : { inert: "" })}
        >
          <div className="aihub-image-clip">
            <div className="aihub-image-drawer">
              <HomeImageModelSelect
                {...modelSettings}
                open={menuOpen}
                onOpenChange={(nextOpen) => {
                  setMenuOpen(nextOpen);
                  if (!nextOpen) setPinnedOpen(isKeyboardModality());
                }}
              />
            </div>
          </div>
        </div>
      )}
    </article>
  );
}
