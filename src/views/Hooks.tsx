import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, FileQuestion, Loader2, RefreshCw, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import { DocumentDiffViewer } from "../components/DocumentDiffViewer";
import { HookInspector } from "../components/HookInspector";
import { useApp } from "../context/AppContext";
import { getErrorMessage } from "../lib/error";
import * as api from "../lib/tauri";
import type {
  HookAgentKey,
  HookEntry,
  HookInspection,
  HookScopeKey,
  HookSource,
  HookSourceStatusKey,
  HookSupportKey,
} from "../lib/tauri";

const ALL = "all";

const AGENT_OPTIONS: HookAgentKey[] = ["codex", "claude_code"];
const SCOPE_OPTIONS: HookScopeKey[] = ["user", "project", "project_local"];
const STATUS_OPTIONS: HookSourceStatusKey[] = ["valid", "missing", "invalid", "too_large"];

/**
 * Why a pair of sources cannot be text-compared, or null when it can.
 *
 * Cross-Agent and cross-format pairs are different grammars, so a line diff
 * would report noise as change; oversized and broken sources never reach the
 * O(n²) diff at all.
 */
function compareRefusal(left: HookSource | null, right: HookSource | null): string | null {
  if (!left || !right) return "pick_two";
  if (left.id === right.id) return "same_source";
  if (left.agent !== right.agent) return "cross_agent";
  if (left.format !== right.format) return "format_mismatch";
  if (!left.diffAvailable || !right.diffAvailable) return "not_diffable";
  return null;
}

function supportTone(support: HookSupportKey): string {
  switch (support) {
    case "supported":
      return "text-emerald-500";
    case "unsupported":
      return "text-[var(--color-text-secondary)]";
    default:
      return "text-amber-500";
  }
}

function statusTone(status: HookSourceStatusKey): string {
  switch (status) {
    case "valid":
      return "border-emerald-400/40 text-emerald-500";
    case "invalid":
      return "border-red-400/40 text-red-500";
    case "too_large":
      return "border-amber-400/40 text-amber-500";
    default:
      return "border-[var(--color-border)] text-[var(--color-text-secondary)]";
  }
}

/**
 * Read-only Hook inspection. The page owns its own load and filter state
 * instead of AppContext: it is the only consumer of this data.
 */
export function Hooks() {
  const { t } = useTranslation();
  const { projects } = useApp();
  const [projectId, setProjectId] = useState<string | null>(null);
  const [data, setData] = useState<HookInspection | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [agentFilter, setAgentFilter] = useState<string>(ALL);
  const [scopeFilter, setScopeFilter] = useState<string>(ALL);
  const [eventFilter, setEventFilter] = useState<string>(ALL);
  const [statusFilter, setStatusFilter] = useState<string>(ALL);
  const [selectedEntryId, setSelectedEntryId] = useState<string | null>(null);
  const [compareLeftId, setCompareLeftId] = useState<string>("");
  const [compareRightId, setCompareRightId] = useState<string>("");
  // Guards against a slow earlier response overwriting a newer selection.
  const requestIdRef = useRef(0);

  const load = useCallback(
    async (selected: string | null) => {
      const requestId = requestIdRef.current + 1;
      requestIdRef.current = requestId;
      setLoading(true);
      try {
        const result = await api.getHookInspection(selected);
        if (requestIdRef.current !== requestId) return;
        setData(result);
        setError(null);
      } catch (err) {
        if (requestIdRef.current !== requestId) return;
        setData(null);
        setError(getErrorMessage(err, t("hooks.loadFailed")));
      } finally {
        if (requestIdRef.current === requestId) setLoading(false);
      }
    },
    [t]
  );

  useEffect(() => {
    void load(projectId);
  }, [load, projectId]);

  // A selection can point at something the new response no longer has.
  useEffect(() => {
    setSelectedEntryId(null);
    setCompareLeftId("");
    setCompareRightId("");
  }, [data]);

  const visibleSources = useMemo<HookSource[]>(() => {
    if (!data) return [];
    return data.sources.filter(
      (source) =>
        (agentFilter === ALL || source.agent === agentFilter) &&
        (scopeFilter === ALL || source.scope === scopeFilter) &&
        (statusFilter === ALL || source.status === statusFilter)
    );
  }, [data, agentFilter, scopeFilter, statusFilter]);

  const events = useMemo(() => {
    if (!data) return [];
    return Array.from(new Set(data.entries.map((entry) => entry.event))).sort();
  }, [data]);

  const visibleEntries = useMemo<HookEntry[]>(() => {
    if (!data) return [];
    const sourceIds = new Set(visibleSources.map((source) => source.id));
    return data.entries.filter(
      (entry) =>
        sourceIds.has(entry.sourceId) && (eventFilter === ALL || entry.event === eventFilter)
    );
  }, [data, visibleSources, eventFilter]);

  const sourceById = useCallback(
    (id: string) => data?.sources.find((source) => source.id === id) ?? null,
    [data]
  );

  const selectedEntry = useMemo(
    () => visibleEntries.find((entry) => entry.id === selectedEntryId) ?? null,
    [visibleEntries, selectedEntryId]
  );

  const compareLeft = sourceById(compareLeftId);
  const compareRight = sourceById(compareRightId);
  const refusal = compareRefusal(compareLeft, compareRight);

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center gap-3">
        <div className="flex flex-col">
          <h1 className="text-xl font-semibold text-[var(--color-text-primary)]">
            {t("hooks.title")}
          </h1>
          <p className="text-sm text-[var(--color-text-secondary)]">{t("hooks.subtitle")}</p>
        </div>
        <span className="ml-auto inline-flex items-center gap-1 rounded-full border border-[var(--color-border)] px-3 py-1 text-xs text-[var(--color-text-secondary)]">
          <ShieldCheck className="h-3.5 w-3.5" />
          {t("hooks.readOnlyBadge")}
        </span>
      </header>

      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("hooks.projectLabel")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={projectId ?? ""}
            onChange={(event) => setProjectId(event.target.value || null)}
          >
            <option value="">{t("hooks.projectNone")}</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.name}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("hooks.filter.agent")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={agentFilter}
            onChange={(event) => setAgentFilter(event.target.value)}
          >
            <option value={ALL}>{t("hooks.filter.all")}</option>
            {AGENT_OPTIONS.map((agent) => (
              <option key={agent} value={agent}>
                {t(`hooks.agent.${agent}`)}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("hooks.filter.scope")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={scopeFilter}
            onChange={(event) => setScopeFilter(event.target.value)}
          >
            <option value={ALL}>{t("hooks.filter.all")}</option>
            {SCOPE_OPTIONS.map((scope) => (
              <option key={scope} value={scope}>
                {t(`hooks.scope.${scope}`)}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("hooks.filter.event")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={eventFilter}
            onChange={(event) => setEventFilter(event.target.value)}
          >
            <option value={ALL}>{t("hooks.filter.all")}</option>
            {events.map((event) => (
              <option key={event} value={event}>
                {event}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("hooks.filter.status")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={statusFilter}
            onChange={(event) => setStatusFilter(event.target.value)}
          >
            <option value={ALL}>{t("hooks.filter.all")}</option>
            {STATUS_OPTIONS.map((status) => (
              <option key={status} value={status}>
                {t(`hooks.status.${status}`)}
              </option>
            ))}
          </select>
        </label>

        <button
          type="button"
          className="inline-flex items-center gap-1 rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          onClick={() => void load(projectId)}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("common.refresh")}
        </button>
        {loading && <Loader2 className="h-4 w-4 animate-spin text-[var(--color-text-secondary)]" />}
      </div>

      {error && (
        <div className="rounded border border-red-400/40 bg-red-400/10 p-3 text-sm text-red-500">
          {error === "invalid_project" ? t("hooks.invalidProject") : error}
        </div>
      )}

      {data && (
        <>
          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("hooks.sourcesHeading")}
            </h2>
            <div className="grid gap-2 md:grid-cols-2">
              {visibleSources.map((source) => (
                <article
                  key={source.id}
                  className="flex flex-col gap-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)] p-3"
                >
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    <span className="font-medium text-[var(--color-text-primary)]">
                      {t(`hooks.agent.${source.agent}`)}
                    </span>
                    <span className="text-[var(--color-text-secondary)]">
                      {t(`hooks.scope.${source.scope}`)}
                    </span>
                    <span className="rounded border border-[var(--color-border)] px-1.5 text-xs uppercase text-[var(--color-text-secondary)]">
                      {source.format}
                    </span>
                    <span
                      className={cn(
                        "ml-auto rounded-full border px-2 py-0.5 text-xs",
                        statusTone(source.status)
                      )}
                    >
                      {t(`hooks.status.${source.status}`)}
                    </span>
                  </div>
                  <p className="break-all font-mono text-xs text-[var(--color-text-secondary)]">
                    {source.displayPath}
                  </p>
                  {source.status === "valid" && (
                    <p className="text-xs text-[var(--color-text-secondary)]">
                      {t("hooks.entryCount", { count: source.entryCount })}
                    </p>
                  )}
                  {source.status === "missing" && (
                    <p className="inline-flex items-center gap-1 text-xs text-[var(--color-text-secondary)]">
                      <FileQuestion className="h-3.5 w-3.5" />
                      {t("hooks.missingHint")}
                    </p>
                  )}
                  {source.diagnostic && (
                    <p className="inline-flex items-start gap-1 text-xs text-amber-500">
                      <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                      <span>
                        {t(`hooks.diagnostic.${source.diagnostic.kind}`)}
                        {": "}
                        <span className="font-mono">{source.diagnostic.message}</span>
                      </span>
                    </p>
                  )}
                </article>
              ))}
              {visibleSources.length === 0 && (
                <p className="text-sm text-[var(--color-text-secondary)]">
                  {t("hooks.noSources")}
                </p>
              )}
            </div>
          </section>

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("hooks.entriesHeading")}
            </h2>
            {visibleEntries.length === 0 ? (
              <p className="text-sm text-[var(--color-text-secondary)]">{t("hooks.noEntries")}</p>
            ) : (
              <ul className="flex flex-col gap-1">
                {visibleEntries.map((entry) => (
                  <li key={entry.id}>
                    <button
                      type="button"
                      onClick={() => setSelectedEntryId(entry.id)}
                      className={cn(
                        "flex w-full flex-wrap items-center gap-2 rounded border px-3 py-2 text-left text-sm",
                        entry.id === selectedEntryId
                          ? "border-[var(--color-accent)] bg-[var(--color-surface)]"
                          : "border-[var(--color-border)] bg-[var(--color-surface)]"
                      )}
                    >
                      <span className="font-medium text-[var(--color-text-primary)]">
                        {entry.event}
                      </span>
                      {!entry.eventKnown && (
                        <span className="rounded border border-amber-400/40 px-1.5 text-xs text-amber-500">
                          {t("hooks.unknown")}
                        </span>
                      )}
                      <span className="text-xs text-[var(--color-text-secondary)]">
                        {entry.matcher ?? t("hooks.noMatcher")}
                      </span>
                      <span className="ml-auto text-xs text-[var(--color-text-secondary)]">
                        {t(`hooks.agent.${entry.agent}`)} · {t(`hooks.scope.${entry.scope}`)} ·{" "}
                        {entry.handlerType || t("hooks.noHandlerType")}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          {selectedEntry && (
            <HookInspector
              entry={selectedEntry}
              source={sourceById(selectedEntry.sourceId)}
              onClose={() => setSelectedEntryId(null)}
            />
          )}

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("hooks.compareHeading")}
            </h2>
            <div className="flex flex-wrap items-center gap-3">
              <select
                className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
                value={compareLeftId}
                onChange={(event) => setCompareLeftId(event.target.value)}
              >
                <option value="">{t("hooks.comparePick")}</option>
                {data.sources.map((source) => (
                  <option key={source.id} value={source.id}>
                    {source.displayPath}
                  </option>
                ))}
              </select>
              <select
                className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
                value={compareRightId}
                onChange={(event) => setCompareRightId(event.target.value)}
              >
                <option value="">{t("hooks.comparePick")}</option>
                {data.sources.map((source) => (
                  <option key={source.id} value={source.id}>
                    {source.displayPath}
                  </option>
                ))}
              </select>
            </div>
            {refusal ? (
              <p className="text-sm text-[var(--color-text-secondary)]">
                {t(`hooks.compareRefusal.${refusal}`)}
              </p>
            ) : (
              compareLeft &&
              compareRight && (
                <DocumentDiffViewer
                  original={compareLeft.canonicalText}
                  updated={compareRight.canonicalText}
                />
              )
            )}
          </section>

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("hooks.matrixHeading")}
            </h2>
            <p className="text-xs text-[var(--color-text-secondary)]">
              {t("hooks.matrixSnapshot", { date: data.snapshotDate })}
            </p>
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-sm">
                <thead>
                  <tr className="text-left text-xs uppercase text-[var(--color-text-secondary)]">
                    <th className="py-1 pr-3">{t("hooks.matrixName")}</th>
                    <th className="py-1 pr-3">{t("hooks.agent.codex")}</th>
                    <th className="py-1">{t("hooks.agent.claude_code")}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.compatibility.map((row) => (
                    <tr key={`${row.category}:${row.name}`} className="border-t border-[var(--color-border)]">
                      <td className="py-1 pr-3">
                        <span className="text-[var(--color-text-primary)]">{row.name}</span>
                        <span className="ml-2 text-xs text-[var(--color-text-secondary)]">
                          {t(`hooks.category.${row.category}`)}
                        </span>
                      </td>
                      <td className={cn("py-1 pr-3", supportTone(row.codex.support))}>
                        {t(`hooks.support.${row.codex.support}`)}
                        {row.codex.note && (
                          <span className="ml-1 text-xs text-[var(--color-text-secondary)]">
                            {t(`hooks.note.${row.codex.note}`)}
                          </span>
                        )}
                      </td>
                      <td className={cn("py-1", supportTone(row.claudeCode.support))}>
                        {t(`hooks.support.${row.claudeCode.support}`)}
                        {row.claudeCode.note && (
                          <span className="ml-1 text-xs text-[var(--color-text-secondary)]">
                            {t(`hooks.note.${row.claudeCode.note}`)}
                          </span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
