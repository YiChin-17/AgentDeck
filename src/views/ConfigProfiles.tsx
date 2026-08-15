import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, FileCog, Info, Loader2, RefreshCw, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useApp } from "../context/AppContext";
import { cn } from "../utils";
import { getErrorMessage } from "../lib/error";
import * as api from "../lib/tauri";
import type {
  ConfigAgentKey,
  ConfigProfile,
  ConfigProfileAssignment,
  ConfigProfileEntryInput,
  ConfigProfileInventory,
  ConfigProfileKey,
  ConfigProfilePreview,
  ConfigProfilePreviewOperation,
  ConfigScopeKey,
  ConfigSetting,
  ConfigValue,
  ConfigValueKindKey,
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

      <ConfigProfileManagement onMutated={() => void load(projectId, agentFilter, scopeFilter)} />
    </div>
  );
}

/** The draft entry set, keyed by `agent/canonicalKey`. */
type Draft = Record<string, ConfigValue | undefined>;

const draftKey = (agent: ConfigAgentKey, canonicalKey: string) => `${agent}/${canonicalKey}`;

/** Turns the draft back into the wire shape, dropping every unset control. */
function draftEntries(keys: ConfigProfileKey[], draft: Draft): ConfigProfileEntryInput[] {
  return keys
    .map((key) => {
      const value = draft[draftKey(key.agent, key.canonicalKey)];
      return value ? { agent: key.agent, canonicalKey: key.canonicalKey, value } : null;
    })
    .filter((entry): entry is ConfigProfileEntryInput => entry !== null);
}

function entriesToDraft(entries: ConfigProfileEntryInput[]): Draft {
  const draft: Draft = {};
  for (const entry of entries) draft[draftKey(entry.agent, entry.canonicalKey)] = entry.value;
  return draft;
}

/** The default a control takes when the user first switches it on. */
function emptyValue(kind: ConfigValueKindKey): ConfigValue {
  if (kind === "boolean") return { type: "boolean", value: false };
  if (kind === "integer") return { type: "integer", value: 0 };
  return { type: "string", value: "" };
}

/**
 * Profile management: create, assign, apply and restore.
 *
 * Kept beside the inventory rather than merged into it. Inspection answers
 * "what is set right now" and has no side effect; management answers "what
 * should be set" and writes only after the user confirms one exact preview.
 * Mixing the two would make it impossible to tell, from the page alone, which
 * of the two a control belongs to.
 */
function ConfigProfileManagement({ onMutated }: { onMutated: () => void }) {
  const { t } = useTranslation();
  const { projects } = useApp();

  const [profiles, setProfiles] = useState<ConfigProfile[]>([]);
  const [keys, setKeys] = useState<ConfigProfileKey[]>([]);
  const [assignments, setAssignments] = useState<ConfigProfileAssignment[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [draft, setDraft] = useState<Draft>({});
  const [preview, setPreview] = useState<ConfigProfilePreview | null>(null);
  // Blocks a second confirm while the first is still writing.
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [manageError, setManageError] = useState<string | null>(null);
  const [assignProjectId, setAssignProjectId] = useState("");
  const [assignAgent, setAssignAgent] = useState<ConfigAgentKey>("codex");
  // Guards against a slow earlier refresh overwriting a newer one.
  const requestIdRef = useRef(0);

  /** A stable backend code becomes a localized sentence; anything else is shown as-is. */
  const describe = useCallback(
    (err: unknown) => {
      const message = getErrorMessage(err, t("configProfiles.loadFailed"));
      const localized = t(`configProfiles.error.${message}`, { defaultValue: "" });
      return localized || message;
    },
    [t]
  );

  const refreshManagement = useCallback(async () => {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    try {
      const [loadedProfiles, loadedKeys, loadedAssignments] = await Promise.all([
        api.listConfigProfiles(),
        api.listConfigProfileKeys(),
        api.listConfigProfileAssignments(),
      ]);
      if (requestIdRef.current !== requestId) return;
      setProfiles(loadedProfiles);
      setKeys(loadedKeys);
      setAssignments(loadedAssignments);
      setManageError(null);
    } catch (err) {
      if (requestIdRef.current !== requestId) return;
      setManageError(describe(err));
    }
  }, [describe]);

  useEffect(() => {
    void refreshManagement();
  }, [refreshManagement]);

  const selected = useMemo(
    () => profiles.find((profile) => profile.id === selectedId) ?? null,
    [profiles, selectedId]
  );

  // The editor follows the selection, so a refresh cannot leave the form
  // describing a profile that is no longer selected.
  useEffect(() => {
    setName(selected?.name ?? "");
    setDraft(selected ? entriesToDraft(selected.entries) : {});
  }, [selected]);

  const selectedAssignments = useMemo(
    () => assignments.filter((assignment) => assignment.profileId === selectedId),
    [assignments, selectedId]
  );

  const projectName = useCallback(
    (id: string) => projects.find((project) => project.id === id)?.name ?? id,
    [projects]
  );

  /**
   * Runs one mutation, then reloads everything the page shows.
   *
   * A partial refresh would leave the revision, the assignment status and the
   * inventory disagreeing about what just happened.
   */
  const run = useCallback(
    async (action: () => Promise<void>, success?: string) => {
      if (busy) return;
      setBusy(true);
      setNotice(null);
      try {
        await action();
        await refreshManagement();
        onMutated();
        setManageError(null);
        if (success) setNotice(success);
      } catch (err) {
        const message = getErrorMessage(err, "");
        // A stale or expired preview is not a failed write: the selection stays
        // and the user reviews a fresh diff instead.
        if (message === "stale_preview" || message === "preview_expired") {
          setPreview(null);
          await refreshManagement();
          onMutated();
        }
        setManageError(describe(err));
      } finally {
        setBusy(false);
      }
    },
    [busy, describe, onMutated, refreshManagement]
  );

  /** Closing the dialog is a state reset. It sends no command. */
  const cancelPreview = () => {
    setPreview(null);
    setNotice(null);
  };

  const saveProfile = () =>
    void run(async () => {
      const entries = draftEntries(keys, draft);
      if (selected) {
        const saved = await api.updateConfigProfile({
          profileId: selected.id,
          expectedRevision: selected.revision,
          name,
          entries,
        });
        setSelectedId(saved.id);
      } else {
        const created = await api.createConfigProfile({ name, entries });
        setSelectedId(created.id);
      }
    });

  const removeProfile = () =>
    void run(async () => {
      if (!selected) return;
      await api.deleteConfigProfile(selected.id);
      setSelectedId(null);
    });

  const assign = () =>
    void run(async () => {
      if (!selected || !assignProjectId) return;
      await api.setConfigProfileAssignment({
        profileId: selected.id,
        projectId: assignProjectId,
        agent: assignAgent,
      });
    });

  const unassign = (assignment: ConfigProfileAssignment) =>
    void run(async () => {
      await api.removeConfigProfileAssignment({
        profileId: assignment.profileId,
        projectId: assignment.projectId,
        agent: assignment.agent,
      });
    });

  const openPreview = (
    assignment: ConfigProfileAssignment,
    operation: ConfigProfilePreviewOperation
  ) =>
    void run(async () => {
      const request = {
        profileId: assignment.profileId,
        projectId: assignment.projectId,
        agent: assignment.agent,
      };
      setPreview(
        operation === "apply"
          ? await api.previewConfigProfileApply(request)
          : await api.previewConfigProfileRestore(request)
      );
    });

  const confirmPreview = () =>
    void run(async () => {
      if (!preview) return;
      if (preview.operation === "apply") {
        await api.applyConfigProfile({ token: preview.token });
      } else {
        await api.applyConfigProfileRestore({ token: preview.token });
      }
      setPreview(null);
    }, preview?.operation === "apply" ? t("configProfiles.manage.applied") : t("configProfiles.manage.restored"));

  return (
    <section className="flex flex-col gap-3 border-t border-[var(--color-border)] pt-4">
      <div className="flex flex-col">
        <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
          {t("configProfiles.manage.heading")}
        </h2>
        <p className="text-xs text-[var(--color-text-secondary)]">
          {t("configProfiles.manage.subtitle")}
        </p>
      </div>

      {manageError && (
        <div className="rounded border border-red-400/40 bg-red-400/10 p-3 text-sm text-red-500">
          {manageError}
        </div>
      )}
      {notice && (
        <div className="rounded border border-emerald-400/40 bg-emerald-400/10 p-3 text-sm text-emerald-600">
          {notice}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <select
          className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
          value={selectedId ?? ""}
          onChange={(event) => setSelectedId(event.target.value || null)}
        >
          <option value="">{t("configProfiles.manage.newProfile")}</option>
          {profiles.map((profile) => (
            <option key={profile.id} value={profile.id}>
              {profile.name}
            </option>
          ))}
        </select>
        {selected && (
          <span className="text-xs text-[var(--color-text-secondary)]">
            {t("configProfiles.manage.revision", { revision: selected.revision })}
          </span>
        )}
      </div>

      {profiles.length === 0 && (
        <p className="text-xs text-[var(--color-text-secondary)]">
          {t("configProfiles.manage.noProfiles")}
        </p>
      )}

      <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
        {t("configProfiles.manage.profileName")}
        <input
          className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
      </label>

      <h3 className="text-xs font-semibold text-[var(--color-text-primary)]">
        {t("configProfiles.manage.entriesHeading")}
      </h3>
      <div className="grid gap-2 md:grid-cols-2">
        {keys.map((key) => {
          const id = draftKey(key.agent, key.canonicalKey);
          const value = draft[id];
          return (
            <div
              key={id}
              className="flex flex-wrap items-center gap-2 rounded border border-[var(--color-border)] px-2 py-1 text-sm"
            >
              <span className="text-xs text-[var(--color-text-secondary)]">
                {t(`configProfiles.agent.${key.agent}`)}
              </span>
              <span className="font-mono text-xs">{key.canonicalKey}</span>
              <input
                type="checkbox"
                className="ml-auto"
                aria-label={key.canonicalKey}
                checked={value !== undefined}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    [id]: event.target.checked ? emptyValue(key.valueKind) : undefined,
                  }))
                }
              />
              {value === undefined ? (
                <span className="text-xs text-[var(--color-text-secondary)]">
                  {t("configProfiles.manage.unset")}
                </span>
              ) : key.valueKind === "boolean" ? (
                <select
                  className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-1 text-xs"
                  value={String(value.value)}
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      [id]: { type: "boolean", value: event.target.value === "true" },
                    }))
                  }
                >
                  <option value="true">true</option>
                  <option value="false">false</option>
                </select>
              ) : (
                <input
                  className="w-32 rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-1 text-xs"
                  type={key.valueKind === "integer" ? "number" : "text"}
                  value={String(value.value)}
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      [id]:
                        key.valueKind === "integer"
                          ? { type: "integer", value: Number(event.target.value) || 0 }
                          : { type: "string", value: event.target.value },
                    }))
                  }
                />
              )}
            </div>
          );
        })}
      </div>

      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          disabled={busy || name.trim().length === 0}
          onClick={saveProfile}
        >
          {t("configProfiles.manage.save")}
        </button>
        {selected && (
          <button
            type="button"
            className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
            disabled={busy}
            onClick={() => {
              if (window.confirm(t("configProfiles.manage.deleteConfirm"))) removeProfile();
            }}
          >
            {t("configProfiles.manage.delete")}
          </button>
        )}
      </div>

      {selected && (
        <>
          <h3 className="text-xs font-semibold text-[var(--color-text-primary)]">
            {t("configProfiles.manage.assignHeading")}
          </h3>
          <div className="flex flex-wrap items-center gap-2">
            <select
              className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
              value={assignProjectId}
              onChange={(event) => setAssignProjectId(event.target.value)}
            >
              <option value="">{t("configProfiles.projectNone")}</option>
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
            <select
              className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
              value={assignAgent}
              onChange={(event) => setAssignAgent(event.target.value as ConfigAgentKey)}
            >
              {AGENT_OPTIONS.map((agent) => (
                <option key={agent} value={agent}>
                  {t(`configProfiles.agent.${agent}`)}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
              disabled={busy || assignProjectId === ""}
              onClick={assign}
            >
              {t("configProfiles.manage.assign")}
            </button>
          </div>

          {selectedAssignments.length === 0 ? (
            <p className="text-xs text-[var(--color-text-secondary)]">
              {t("configProfiles.manage.noAssignments")}
            </p>
          ) : (
            <div className="overflow-x-auto rounded border border-[var(--color-border)]">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-[var(--color-border)] text-xs text-[var(--color-text-secondary)]">
                    <th className="px-3 py-2">{t("configProfiles.filter.project")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.agent")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.source")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.status")}</th>
                    <th className="px-3 py-2" />
                  </tr>
                </thead>
                <tbody>
                  {selectedAssignments.map((assignment) => (
                    <tr
                      key={`${assignment.projectId}/${assignment.agent}`}
                      className="border-b border-[var(--color-border)] last:border-b-0"
                    >
                      <td className="px-3 py-2">{projectName(assignment.projectId)}</td>
                      <td className="px-3 py-2">
                        {t(`configProfiles.agent.${assignment.agent}`)}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs">{assignment.sourceId}</td>
                      <td className="px-3 py-2 text-xs">
                        {t(`configProfiles.manage.status.${assignment.status}`, {
                          defaultValue: assignment.status,
                        })}
                        <span className="ml-2 text-[var(--color-text-secondary)]">
                          {assignment.lastAppliedAt
                            ? t("configProfiles.manage.lastApplied", {
                                when: new Date(assignment.lastAppliedAt * 1000).toLocaleString(),
                              })
                            : t("configProfiles.manage.neverApplied")}
                        </span>
                        {assignment.hasRecoveryPoint && (
                          <span className="ml-2 text-[var(--color-text-secondary)]">
                            {t("configProfiles.manage.hasRecovery")}
                          </span>
                        )}
                      </td>
                      <td className="flex flex-wrap gap-1 px-3 py-2">
                        <button
                          type="button"
                          className="rounded border border-[var(--color-border)] px-2 py-1 text-xs"
                          disabled={busy}
                          onClick={() => openPreview(assignment, "apply")}
                        >
                          {t("configProfiles.manage.applyAction")}
                        </button>
                        <button
                          type="button"
                          className="rounded border border-[var(--color-border)] px-2 py-1 text-xs"
                          disabled={busy || !assignment.hasRecoveryPoint}
                          onClick={() => openPreview(assignment, "restore")}
                        >
                          {t("configProfiles.manage.restoreAction")}
                        </button>
                        <button
                          type="button"
                          className="rounded border border-[var(--color-border)] px-2 py-1 text-xs"
                          disabled={busy}
                          onClick={() => unassign(assignment)}
                        >
                          {t("configProfiles.manage.unassign")}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}

      {preview && (
        <div
          role="dialog"
          aria-modal="true"
          className="flex flex-col gap-2 rounded border border-[var(--color-border)] bg-[var(--color-surface)] p-3"
        >
          <h3 className="text-sm font-semibold text-[var(--color-text-primary)]">
            {t("configProfiles.manage.previewHeading")}
          </h3>
          <p className="text-xs text-[var(--color-text-secondary)]">
            {preview.operation === "apply"
              ? t("configProfiles.manage.previewApply", {
                  profile: preview.profileName,
                  project: projectName(preview.projectId),
                })
              : t("configProfiles.manage.previewRestore", {
                  project: projectName(preview.projectId),
                })}
          </p>
          <p className="font-mono text-xs text-[var(--color-text-secondary)]">
            {t(`configProfiles.agent.${preview.agent}`)} · {preview.sourceId}
          </p>
          {preview.wouldCreateFile && (
            <p className="inline-flex items-start gap-1 text-xs text-[var(--color-text-secondary)]">
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span>{t("configProfiles.manage.wouldCreateFile")}</span>
            </p>
          )}
          {preview.wouldRemoveFile && (
            <p className="inline-flex items-start gap-1 text-xs text-[var(--color-text-secondary)]">
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span>{t("configProfiles.manage.wouldRemoveFile")}</span>
            </p>
          )}
          {preview.diff.length === 0 ? (
            <p className="text-xs text-[var(--color-text-secondary)]">
              {t("configProfiles.manage.noChanges")}
            </p>
          ) : (
            <div className="overflow-x-auto rounded border border-[var(--color-border)]">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-[var(--color-border)] text-xs text-[var(--color-text-secondary)]">
                    <th className="px-3 py-2">{t("configProfiles.column.key")}</th>
                    <th className="px-3 py-2">{t("configProfiles.manage.column.before")}</th>
                    <th className="px-3 py-2">{t("configProfiles.manage.column.after")}</th>
                    <th className="px-3 py-2">{t("configProfiles.column.status")}</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.diff.map((entry) => (
                    <tr
                      key={`${entry.agent}/${entry.canonicalKey}`}
                      className="border-b border-[var(--color-border)] last:border-b-0"
                    >
                      <td className="px-3 py-2 font-mono text-xs">{entry.canonicalKey}</td>
                      <td className="px-3 py-2 font-mono text-xs">
                        {entry.before ? renderValue(entry.before) : "—"}
                      </td>
                      <td className="px-3 py-2 font-mono text-xs">
                        {entry.after ? renderValue(entry.after) : "—"}
                      </td>
                      <td className="px-3 py-2 text-xs">
                        {t(`configProfiles.diff.${entry.status}`)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="flex gap-2">
            <button
              type="button"
              className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
              disabled={busy}
              onClick={confirmPreview}
            >
              {t("configProfiles.manage.confirm")}
            </button>
            <button
              type="button"
              className="rounded border border-[var(--color-border)] px-2 py-1 text-sm"
              onClick={cancelPreview}
            >
              {t("common.cancel")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
