import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import { DetailSheet } from "./DetailSheet";
import { CLAUDE_TOOL_KEY, CODEX_TOOL_KEY, type BoardCardModel } from "./boardLanes";

export interface ArtifactInspectorProps {
  card: BoardCardModel;
  /** Full source description; never derived from the card summary. */
  description: string | null;
  whenToUse: string | null;
  deploymentMode: string | null;
  sourcePath: string | null;
  syncState: string | null;
  /** Agent key to display name, for canonical and other targets. */
  agentLabels: Record<string, string>;
  /** A target mutation is in flight for this Artifact. */
  busy?: boolean;
  onToggleTarget: (target: "codex" | "claude", enabled: boolean) => void;
  /** Omitted or null when the current data offers no diff to open. */
  onOpenDiff?: (() => void) | null;
  onOpenDetails?: () => void;
  onClose: () => void;
}

export function ArtifactInspector({
  card,
  description,
  whenToUse,
  deploymentMode,
  sourcePath,
  syncState,
  agentLabels,
  busy = false,
  onToggleTarget,
  onOpenDiff,
  onOpenDetails,
  onClose,
}: ArtifactInspectorProps) {
  const { t } = useTranslation();
  const unavailable = t("board.unavailable");
  const label = (key: string) => agentLabels[key] ?? key;

  return (
    <DetailSheet
      open
      variant="docked"
      title={card.title}
      description={
        <p className="whitespace-pre-wrap">{description?.trim() ? description : unavailable}</p>
      }
      onClose={onClose}
    >
      <section className="space-y-4">
        <Field label={t("board.inspector.whenToUse")} value={whenToUse} fallback={unavailable} />

        <div>
          <h3 className="app-section-title">{t("board.inspector.targets")}</h3>
          <div className="mt-2 space-y-1.5">
            <TargetCheckbox
              label={label(CODEX_TOOL_KEY)}
              checked={card.canonicalTargets.codex}
              disabled={busy}
              onChange={(next) => onToggleTarget("codex", next)}
            />
            <TargetCheckbox
              label={label(CLAUDE_TOOL_KEY)}
              checked={card.canonicalTargets.claude}
              disabled={busy}
              onChange={(next) => onToggleTarget("claude", next)}
            />
          </div>
          <p className="mt-2 text-[12px] text-muted">
            {t("board.inspector.otherTargets")}:{" "}
            {card.otherTargets.length > 0
              ? card.otherTargets.map(label).join("、")
              : unavailable}
          </p>
        </div>

        <Field label={t("board.inspector.deploymentMode")} value={deploymentMode} fallback={unavailable} />
        <Field label={t("board.inspector.sourcePath")} value={sourcePath} fallback={unavailable} mono />
        <Field label={t("board.inspector.syncState")} value={syncState} fallback={unavailable} />

        <div className="flex flex-wrap gap-2">
          {/* An unavailable diff is stated, never offered as an executable action. */}
          {onOpenDiff ? (
            <button type="button" onClick={onOpenDiff} className="app-button-secondary py-2 text-[12px]">
              {t("board.inspector.openDiff")}
            </button>
          ) : (
            <span className="text-[12px] text-faint">
              {t("board.inspector.openDiff")}: {unavailable}
            </span>
          )}
          {onOpenDetails && (
            <button type="button" onClick={onOpenDetails} className="app-button-secondary py-2 text-[12px]">
              {t("board.inspector.openDetails")}
            </button>
          )}
        </div>
      </section>
    </DetailSheet>
  );
}

function Field({
  label,
  value,
  fallback,
  mono,
}: {
  label: string;
  value: string | null;
  fallback: string;
  mono?: boolean;
}) {
  const filled = Boolean(value && value.trim());
  return (
    <div>
      <h3 className="app-section-title">{label}</h3>
      <p
        className={cn(
          "mt-1 break-words text-[12px] leading-5",
          filled ? "text-secondary" : "text-faint",
          mono && filled && "font-mono",
        )}
      >
        {filled ? value : fallback}
      </p>
    </div>
  );
}

function TargetCheckbox({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <label
      className={cn(
        "flex items-center gap-2 text-[13px]",
        disabled ? "cursor-not-allowed text-muted" : "cursor-pointer text-secondary",
      )}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        className="h-3.5 w-3.5 accent-[var(--color-action)]"
      />
      {label}
    </label>
  );
}
