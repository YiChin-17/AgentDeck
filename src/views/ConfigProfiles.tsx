import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, FileCog, Info, Loader2, RefreshCw, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useApp } from "../context/AppContext";
import { cn } from "../utils";
import { getErrorMessage } from "../lib/error";
import * as api from "../lib/tauri";
import type {
  ConfigAgentKey,
  ConfigProfileInventory,
  ConfigScopeKey,
  ConfigSetting,
  ConfigValue,
} from "../lib/tauri";

const ALL = "all";

const AGENT_OPTIONS: ConfigAgentKey[] = ["codex", "claude_code"];
const SCOPE_OPTIONS: ConfigScopeKey[] = ["user", "project", "project_local"];

/** Statuses that mean the source could not contribute any setting. */
const FAILED_STATUSES = new Set([
  "unreadable",
  "too_large",
  "unsupported_symlink",
  "invalid_format",
]);

/** The typed value as one line of text. The backend already fixed the shape. */
function renderValue(value: ConfigValue): string {
  return typeof value.value === "boolean" ? String(value.value) : String(value.value);
}

/**
 * Read-only Config Profile inspection for Codex and Claude Code.
 *
 * The page owns its response and its filters and shares neither: what it shows
 * is one snapshot of fixed configuration files, not application state, so
 * nothing here reaches the shared context or storage that would outlive the
 * view. There is no write path — the only command it can reach reads.
 */
export function ConfigProfiles() {
  const { t } = useTranslation();
  const { projects } = useApp();
  const [inventory, setInventory] = useState<ConfigProfileInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [agentFilter, setAgentFilter] = useState<string>(ALL);
  const [scopeFilter, setScopeFilter] = useState<string>(ALL);
  // Guards against a slow earlier response overwriting a newer selection.
  const requestIdRef = useRef(0);

  const load = useCallback(
    async (selected: string | null, agent: string, scope: string) => {
      const requestId = requestIdRef.current + 1;
      requestIdRef.current = requestId;
      setLoading(true);
      try {
        const result = await api.getConfigProfileInventory({
          projectId: selected,
          agent: agent === ALL ? null : (agent as ConfigAgentKey),
          scope: scope === ALL ? null : (scope as ConfigScopeKey),
        });
        if (requestIdRef.current !== requestId) return;
        setInventory(result);
        setError(null);
      } catch (err) {
        if (requestIdRef.current !== requestId) return;
        setInventory(null);
        const message = getErrorMessage(err, t("configProfiles.loadFailed"));
        setError(
          message === "project_not_found" ? t("configProfiles.projectNotFound") : message
        );
      } finally {
        if (requestIdRef.current === requestId) setLoading(false);
      }
    },
    [t]
  );

  useEffect(() => {
    void load(projectId, agentFilter, scopeFilter);
  }, [load, projectId, agentFilter, scopeFilter]);

  // A Project that disappeared from the registry must not stay selected.
  useEffect(() => {
    if (projectId !== null && !projects.some((project) => project.id === projectId)) {
      setProjectId(null);
    }
  }, [projects, projectId]);

  const visibleSettings = useMemo<ConfigSetting[]>(
    () => inventory?.settings ?? [],
    [inventory]
  );

  const hasFailedSource = useMemo(
    () => (inventory?.sources ?? []).some((source) => FAILED_STATUSES.has(source.status)),
    [inventory]
  );

  const sourcePath = useCallback(
    (sourceId: string) =>
      inventory?.sources.find((source) => source.id === sourceId)?.displayPath ?? sourceId,
    [inventory]
  );

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center gap-3">
        <div className="flex flex-col">
          <h1 className="text-xl font-semibold text-[var(--color-text-primary)]">
            {t("configProfiles.title")}
          </h1>
          <p className="text-sm text-[var(--color-text-secondary)]">
            {t("configProfiles.subtitle")}
          </p>
        </div>
        <span className="ml-auto inline-flex items-center gap-1 rounded-full border border-[var(--color-border)] px-3 py-1 text-xs text-[var(--color-text-secondary)]">
          <ShieldCheck className="h-3.5 w-3.5" />
          {t("configProfiles.readOnlyBadge")}
        </span>
      </header>

      <p className="inline-flex items-start gap-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)] p-3 text-xs text-[var(--color-text-secondary)]">
        <Info className="mt-0.5 h-3.5 w-3.5 shrink-0" />
        <span>{t("configProfiles.runtimeLimitation")}</span>
      </p>

      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("configProfiles.filter.project")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={projectId ?? ""}
            onChange={(event) => setProjectId(event.target.value || null)}
          >
            <option value="">{t("configProfiles.projectNone")}</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.name}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("configProfiles.filter.agent")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={agentFilter}
            onChange={(event) => setAgentFilter(event.target.value)}
          >
            <option value={ALL}>{t("configProfiles.filter.all")}</option>
            {AGENT_OPTIONS.map((agent) => (
              <option key={agent} value={agent}>
                {t(`configProfiles.agent.${agent}`)}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("configProfiles.filter.scope")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={scopeFilter}
            onChange={(event) => setScopeFilter(event.target.value)}
          >
            <option value={ALL}>{t("configProfiles.filter.all")}</option>
            {SCOPE_OPTIONS.map((scope) => (
              <option key={scope} value={scope}>
                {t(`configProfiles.scope.${scope}`)}
              </option>
            ))}
          </select>
        </label>

        <button
          type="button"
          className="inline-flex items-center gap-1 rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          onClick={() => void load(projectId, agentFilter, scopeFilter)}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("common.refresh")}
        </button>
        {loading && <Loader2 className="h-4 w-4 animate-spin text-[var(--color-text-secondary)]" />}
      </div>

      {projects.length === 0 && (
        <p className="text-xs text-[var(--color-text-secondary)]">
          {t("configProfiles.noProjects")}
        </p>
      )}

      {error && (
        <div className="rounded border border-red-400/40 bg-red-400/10 p-3 text-sm text-red-500">
          {error}
        </div>
      )}

      {inventory && (
        <>
          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("configProfiles.sourcesHeading")}
            </h2>
            <div className="overflow-x-auto rounded border border-[var(--color-border)]">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-[var(--color-border)] text-xs text-[var(--color-text-secondary)]">
                    <th className="px-3 py-2">{t("configProfiles.column.agent")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.scope")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.path")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.status")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.fingerprint")}</th>
                  </tr>
                </thead>
                <tbody>
                  {inventory.sources.map((source) => (
                    <tr
                      key={source.id}
                      className="border-b border-[var(--color-border)] last:border-b-0"
                    >
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`configProfiles.agent.${source.agent}`)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`configProfiles.scope.${source.scope}`)}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs text-[var(--color-text-secondary)]">
                        {source.displayPath}
                      </td>
                      <td
                        className={cn(
                          "px-3 py-2 text-xs",
                          FAILED_STATUSES.has(source.status)
                            ? "text-amber-500"
                            : "text-[var(--color-text-secondary)]"
                        )}
                      >
                        {t(`configProfiles.status.${source.status}`)}
                        {source.hasUnexposedFields && (
                          <span className="ml-2 text-[var(--color-text-secondary)]">
                            {t("configProfiles.hasUnexposedFields")}
                          </span>
                        )}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs text-[var(--color-text-secondary)]">
                        {source.fingerprint ? source.fingerprint.slice(0, 12) : "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {hasFailedSource && (
              <p className="inline-flex items-start gap-1 text-xs text-amber-500">
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span>{t("configProfiles.sourceFailureHint")}</span>
              </p>
            )}
          </section>

          {inventory.diagnostics.length > 0 && (
            <section className="flex flex-col gap-1">
              <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
                {t("configProfiles.diagnosticsHeading")}
              </h2>
              {inventory.diagnostics.map((diagnostic, index) => (
                <p
                  key={`${diagnostic.sourceId}:${diagnostic.code}:${diagnostic.key ?? index}`}
                  className="inline-flex items-start gap-1 text-xs text-amber-500"
                >
                  <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                  <span>
                    <span className="font-mono">{sourcePath(diagnostic.sourceId)}</span>
                    {": "}
                    {t(`configProfiles.diagnostic.${diagnostic.code}`)}
                    {diagnostic.key !== null && (
                      <span className="font-mono"> ({diagnostic.key})</span>
                    )}
                  </span>
                </p>
              ))}
            </section>
          )}

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("configProfiles.settingsHeading")}
            </h2>
            <div className="overflow-x-auto rounded border border-[var(--color-border)]">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-[var(--color-border)] text-xs text-[var(--color-text-secondary)]">
                    <th className="px-3 py-2">{t("configProfiles.column.agent")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.key")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.value")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.scope")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.source")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.resolution")}</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleSettings.map((setting) => (
                    <tr
                      key={`${setting.sourceId}:${setting.canonicalKey}`}
                      className="border-b border-[var(--color-border)] last:border-b-0"
                    >
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`configProfiles.agent.${setting.agent}`)}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs text-[var(--color-text-primary)]">
                        {setting.nativeKey}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs text-[var(--color-text-primary)]">
                        {renderValue(setting.value)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`configProfiles.scope.${setting.scope}`)}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs text-[var(--color-text-secondary)]">
                        {sourcePath(setting.sourceId)}
                      </td>
                      <td
                        className={cn(
                          "px-3 py-2 text-xs",
                          setting.resolution === "observed_overridden"
                            ? "text-[var(--color-text-secondary)] line-through"
                            : "text-[var(--color-text-primary)]"
                        )}
                      >
                        {t(`configProfiles.resolution.${setting.resolution}`)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {visibleSettings.length === 0 && (
                <p className="p-3 text-sm text-[var(--color-text-secondary)]">
                  {t("configProfiles.noSettings")}
                </p>
              )}
            </div>
          </section>

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("configProfiles.diffHeading")}
            </h2>
            <div className="overflow-x-auto rounded border border-[var(--color-border)]">
              <table className="w-full text-left text-sm">
                <tbody>
                  {inventory.diffs.map((entry) => (
                    <tr
                      key={`${entry.agent}:${entry.canonicalKey}:${entry.baseScope}:${entry.compareScope}`}
                      className="border-b border-[var(--color-border)] last:border-b-0"
                    >
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`configProfiles.agent.${entry.agent}`)}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs text-[var(--color-text-primary)]">
                        {entry.canonicalKey}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`configProfiles.scope.${entry.baseScope}`)} →{" "}
                        {t(`configProfiles.scope.${entry.compareScope}`)}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs text-[var(--color-text-secondary)]">
                        {entry.baseValue ? renderValue(entry.baseValue) : "—"} →{" "}
                        {entry.compareValue ? renderValue(entry.compareValue) : "—"}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-primary)]">
                        {t(`configProfiles.diff.${entry.status}`)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {inventory.diffs.length === 0 && (
                <p className="inline-flex items-start gap-1 p-3 text-sm text-[var(--color-text-secondary)]">
                  <FileCog className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                  <span>{t("configProfiles.noDiff")}</span>
                </p>
              )}
            </div>
          </section>
        </>
      )}
    </div>
  );
}
