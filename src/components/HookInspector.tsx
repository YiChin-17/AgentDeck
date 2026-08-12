import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { HookEntry, HookSource } from "../lib/tauri";

interface HookInspectorProps {
  entry: HookEntry;
  source: HookSource | null;
  onClose: () => void;
}

function UnknownBadge({ label }: { label: string }) {
  return (
    <span className="rounded border border-amber-400/40 px-1.5 text-xs text-amber-500">{label}</span>
  );
}

/**
 * Full detail of one Hook handler, exactly as the Agent config declares it.
 * Values a handler carries — commands, prompts, URLs — are shown because the
 * user asked to inspect them; nothing here writes, stores or runs them.
 */
export function HookInspector({ entry, source, onClose }: HookInspectorProps) {
  const { t } = useTranslation();

  return (
    <aside className="flex flex-col gap-3 rounded border border-[var(--color-border)] bg-[var(--color-surface)] p-4">
      <div className="flex items-start gap-2">
        <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
          {t("hooks.inspector.title")}
        </h2>
        <button
          type="button"
          className="ml-auto rounded p-1 text-[var(--color-text-secondary)]"
          onClick={onClose}
          aria-label={t("common.close")}
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
        <dt className="text-[var(--color-text-secondary)]">{t("hooks.inspector.agent")}</dt>
        <dd className="text-[var(--color-text-primary)]">{t(`hooks.agent.${entry.agent}`)}</dd>

        <dt className="text-[var(--color-text-secondary)]">{t("hooks.inspector.scope")}</dt>
        <dd className="text-[var(--color-text-primary)]">{t(`hooks.scope.${entry.scope}`)}</dd>

        <dt className="text-[var(--color-text-secondary)]">{t("hooks.inspector.source")}</dt>
        <dd className="break-all font-mono text-xs text-[var(--color-text-primary)]">
          {source?.displayPath ?? entry.sourceId}
        </dd>

        <dt className="text-[var(--color-text-secondary)]">{t("hooks.inspector.event")}</dt>
        <dd className="flex items-center gap-2 text-[var(--color-text-primary)]">
          {entry.event}
          {!entry.eventKnown && <UnknownBadge label={t("hooks.unknown")} />}
        </dd>

        <dt className="text-[var(--color-text-secondary)]">{t("hooks.inspector.matcher")}</dt>
        <dd className="font-mono text-xs text-[var(--color-text-primary)]">
          {entry.matcher ?? t("hooks.noMatcher")}
        </dd>

        <dt className="text-[var(--color-text-secondary)]">{t("hooks.inspector.handlerType")}</dt>
        <dd className="flex items-center gap-2 text-[var(--color-text-primary)]">
          {entry.handlerType || t("hooks.noHandlerType")}
          {!entry.handlerTypeKnown && <UnknownBadge label={t("hooks.unknown")} />}
        </dd>

        <dt className="text-[var(--color-text-secondary)]">{t("hooks.inspector.position")}</dt>
        <dd className="text-[var(--color-text-primary)]">
          {t("hooks.inspector.positionValue", {
            group: entry.groupIndex + 1,
            handler: entry.handlerIndex + 1,
          })}
        </dd>
      </dl>

      <div className="flex flex-col gap-1">
        <h3 className="text-xs font-semibold uppercase text-[var(--color-text-secondary)]">
          {t("hooks.inspector.fields")}
        </h3>
        {entry.fields.length === 0 ? (
          <p className="text-sm text-[var(--color-text-secondary)]">
            {t("hooks.inspector.noFields")}
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {entry.fields.map((field) => (
              <li key={field.key} className="flex flex-col gap-0.5">
                <span className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
                  {field.key}
                  {!field.known && <UnknownBadge label={t("hooks.unknown")} />}
                </span>
                <span className="whitespace-pre-wrap break-all rounded bg-[var(--color-bg)] p-2 font-mono text-xs text-[var(--color-text-primary)]">
                  {field.value}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </aside>
  );
}
