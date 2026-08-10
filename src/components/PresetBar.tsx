import { useCallback, useMemo, useState } from "react";
import { Check, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { cn } from "../utils";
import { computePresetStatus } from "../lib/presetStatus";
import { getPresetIconOption } from "../lib/presetIcons";
import type { ManagedSkill, Preset } from "../lib/tauri";
import { getErrorMessage } from "../lib/error";
import { ConfirmDialog } from "./ConfirmDialog";
import { useApp } from "../context/AppContext";

export interface PresetBarProps {
  presets: Preset[];
  managedSkills: ManagedSkill[];
  agentKeys: string[];
  existsInWorkspace: (skill: ManagedSkill, agentKey: string) => boolean;
  onAddSkill: (skill: ManagedSkill, agentKey: string) => Promise<void>;
  onRemoveSkill: (skill: ManagedSkill, agentKey: string) => Promise<void>;
  onComplete: () => Promise<void>;
}

export function PresetBar({
  presets,
  managedSkills,
  agentKeys,
  existsInWorkspace,
  onAddSkill,
  onRemoveSkill,
  onComplete,
}: PresetBarProps) {
  const { t } = useTranslation();
  // Adding or removing a Skill Pack writes to deployment targets, so both stay
  // unavailable while the Library cannot be verified.
  const { libraryOffline } = useApp();
  const [loadingKey, setLoadingKey] = useState<string | null>(null);
  const [pendingRemoval, setPendingRemoval] = useState<{ preset: Preset; count: number } | null>(null);

  const statuses = useMemo(() => {
    const map = new Map<string, ReturnType<typeof computePresetStatus>>();
    for (const preset of presets) {
      map.set(preset.id, computePresetStatus(preset, managedSkills, agentKeys, existsInWorkspace));
    }
    return map;
  }, [presets, managedSkills, agentKeys, existsInWorkspace]);

  const visiblePresets = useMemo(
    () => presets.filter((p) => statuses.get(p.id)?.status !== "empty"),
    [presets, statuses]
  );

  const handleActivate = useCallback(async (preset: Preset) => {
    setLoadingKey(`${preset.id}-add`);
    try {
      const presetSkills = managedSkills.filter((s) => s.preset_ids.includes(preset.id));
      let added = 0, skipped = 0, failed = 0;
      for (const skill of presetSkills) {
        for (const agentKey of agentKeys) {
          if (existsInWorkspace(skill, agentKey)) { skipped++; continue; }
          try { await onAddSkill(skill, agentKey); added++; }
          catch { failed++; }
        }
      }
      if (added > 0) {
        toast.success(t("presetActions.addedToast", { added, skipped }));
      } else if (failed === 0) {
        toast.info(t("presetActions.nothingToAdd"));
      }
      if (failed > 0) toast.error(t("presetActions.partialFailedToast", { count: failed }));
      await onComplete();
    } catch (error) {
      toast.error(getErrorMessage(error, t("common.error")));
    } finally {
      setLoadingKey(null);
    }
  }, [agentKeys, existsInWorkspace, managedSkills, onAddSkill, onComplete, t]);

  const handleDeactivate = useCallback(async (preset: Preset) => {
    setLoadingKey(`${preset.id}-remove`);
    try {
      const presetSkills = managedSkills.filter((s) => s.preset_ids.includes(preset.id));
      let removed = 0, failed = 0;
      for (const skill of presetSkills) {
        for (const agentKey of agentKeys) {
          if (!existsInWorkspace(skill, agentKey)) continue;
          try { await onRemoveSkill(skill, agentKey); removed++; }
          catch { failed++; }
        }
      }
      if (removed > 0) {
        toast.success(t("presetActions.removedToast", { removed }));
      } else if (failed === 0) {
        toast.info(t("presetActions.nothingToRemove"));
      }
      if (failed > 0) toast.error(t("presetActions.partialFailedToast", { count: failed }));
      await onComplete();
    } catch (error) {
      toast.error(getErrorMessage(error, t("common.error")));
    } finally {
      setLoadingKey(null);
    }
  }, [agentKeys, existsInWorkspace, managedSkills, onComplete, onRemoveSkill, t]);

  if (visiblePresets.length === 0) return null;

  const busy = loadingKey !== null;

  return (
    <>
      <div className="flex min-w-0 flex-wrap items-center gap-1.5">
        <span className="shrink-0 text-[12px] text-muted">{t("sidebar.presets")}</span>
        <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto scrollbar-hide">
          {visiblePresets.map((preset) => {
            const s = statuses.get(preset.id)!;
            const presetIcon = getPresetIconOption(preset);
            const Icon = presetIcon.icon;
            const isAdding = loadingKey === `${preset.id}-add`;
            const isRemoving = loadingKey === `${preset.id}-remove`;

            return (
              <div
                key={preset.id}
                className="inline-flex shrink-0 items-stretch overflow-hidden rounded-full border border-border-subtle bg-surface"
              >
                <div
                  title={preset.name}
                  className={cn(
                    "inline-flex items-center gap-1 px-2.5 py-0.5 text-[12px] font-medium",
                    s.status === "active"
                      ? `${presetIcon.activeClass} ${presetIcon.colorClass}`
                      : s.status === "partial"
                      ? "bg-amber-500/8 text-amber-600 dark:text-amber-400"
                      : "text-faint"
                  )}
                >
                  <Icon className="h-3 w-3" />
                  <span className="max-w-[140px] truncate">{preset.name}</span>
                  {s.status === "active" && <Check className="h-3 w-3 shrink-0" />}
                  {s.status === "partial" && (
                    <span className="rounded-full bg-amber-500/20 px-1.5 py-px text-[10px] font-semibold">
                      {s.installed}/{s.total}
                    </span>
                  )}
                </div>
                <button
                  type="button"
                  onClick={() => handleActivate(preset)}
                  disabled={busy || libraryOffline || s.installed === s.total}
                  title={libraryOffline ? t("library.offline.actionBlocked") : undefined}
                  className="border-l border-border-subtle px-2 py-0.5 text-[11px] font-medium text-action transition-colors hover:bg-action-bg disabled:cursor-not-allowed disabled:text-faint disabled:opacity-60"
                >
                  {isAdding ? <Loader2 className="h-3 w-3 animate-spin" /> : t("presetBar.add")}
                </button>
                <button
                  type="button"
                  onClick={() => setPendingRemoval({ preset, count: s.installed })}
                  disabled={busy || libraryOffline || s.installed === 0}
                  title={libraryOffline ? t("library.offline.actionBlocked") : undefined}
                  className="border-l border-border-subtle px-2 py-0.5 text-[11px] font-medium text-red-500 transition-colors hover:bg-red-500/10 disabled:cursor-not-allowed disabled:text-faint disabled:opacity-60"
                >
                  {isRemoving ? <Loader2 className="h-3 w-3 animate-spin" /> : t("presetBar.remove")}
                </button>
              </div>
            );
          })}
        </div>
      </div>

      {pendingRemoval && (
        <ConfirmDialog
          open
          title={t("presetBar.removeTitle")}
          message={t("presetBar.removeConfirm", {
            name: pendingRemoval.preset.name,
            count: pendingRemoval.count,
          })}
          details={[t("presetBar.removeKeepsLibrary")]}
          confirmLabel={t("presetBar.remove")}
          onClose={() => setPendingRemoval(null)}
          onConfirm={() => handleDeactivate(pendingRemoval.preset)}
        />
      )}
    </>
  );
}
