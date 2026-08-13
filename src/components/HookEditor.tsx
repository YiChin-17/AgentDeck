import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, Eye, Loader2, RotateCcw, Save, Trash2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import { DocumentDiffViewer } from "./DocumentDiffViewer";
import { getErrorMessage } from "../lib/error";
import * as api from "../lib/tauri";
import type {
  HookAgentRegistry,
  HookEditOperation,
  HookEntry,
  HookHandlerDraft,
  HookRecovery,
  HookSource,
  HookWritePreview,
} from "../lib/tauri";

/**
 * The editor never writes on its own: a draft has to become a backend preview,
 * and only that preview's base revision can be applied. Any edit drops back to
 * `editing`, which is what makes a stale diff impossible to apply.
 */
type EditorState = "editing" | "previewing" | "preview_ready" | "applying" | "applied";

interface Props {
  projectId: string | null;
  source: HookSource;
  /** The handler being edited, or null to create the first one of a draft. */
  entry: HookEntry | null;
  registry: HookAgentRegistry;
  onApplied: () => void;
  onClose: () => void;
}

/** Fields the user can type into, keyed by name. Empty means "not set". */
type FieldInputs = Record<string, string>;

function initialFields(entry: HookEntry | null, registry: HookAgentRegistry): FieldInputs {
  const inputs: FieldInputs = {};
  for (const descriptor of registry.fields) inputs[descriptor.name] = "";
  if (!entry) return inputs;
  for (const field of entry.fields) {
    if (field.known) inputs[field.key] = field.value;
  }
  return inputs;
}

/**
 * Turns one typed input into the JSON value the backend expects for its shape.
 * Returns undefined when the field is simply not set, and null when the text
 * cannot be read as that shape — the backend rejects it either way, but this
 * lets the form say so before a round trip.
 */
function toFieldValue(shape: string, raw: string): unknown | null | undefined {
  const text = raw.trim();
  if (text === "") return undefined;
  switch (shape) {
    case "integer": {
      const value = Number(text);
      return Number.isInteger(value) && value >= 0 ? value : null;
    }
    case "bool":
      if (text === "true") return true;
      if (text === "false") return false;
      return null;
    case "text_list":
    case "table":
      try {
        return JSON.parse(text) as unknown;
      } catch {
        return null;
      }
    default:
      return text;
  }
}

export function HookEditor({
  projectId,
  source,
  entry,
  registry,
  onApplied,
  onClose,
}: Props) {
  const { t } = useTranslation();
  const [state, setState] = useState<EditorState>("editing");
  const [event, setEvent] = useState(entry?.event ?? registry.events[0] ?? "");
  const [matcher, setMatcher] = useState(entry?.matcher ?? "");
  const [handlerType, setHandlerType] = useState(
    entry?.handlerType || registry.handlerTypes[0] || ""
  );
  const [fields, setFields] = useState<FieldInputs>(() => initialFields(entry, registry));
  const [preview, setPreview] = useState<HookWritePreview | null>(null);
  const [recovery, setRecovery] = useState<HookRecovery | null>(null);
  const [restorePreview, setRestorePreview] = useState<HookWritePreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Guards against a slow earlier response overwriting a newer draft.
  const requestIdRef = useRef(0);

  const unknownFields = useMemo(
    () => (entry?.fields ?? []).filter((field) => !field.known),
    [entry]
  );

  /** Every edit invalidates the diff the user was looking at. */
  const invalidate = useCallback(() => {
    requestIdRef.current += 1;
    setPreview(null);
    setState("editing");
  }, []);

  const localized = useCallback(
    (err: unknown) => {
      const message = getErrorMessage(err, t("hooks.edit.failed"));
      const key = `hooks.edit.error.${message}`;
      const translated = t(key);
      return translated === key ? message : translated;
    },
    [t]
  );

  const draft = useCallback((): HookHandlerDraft => {
    const values: Record<string, unknown> = {};
    for (const descriptor of registry.fields) {
      const value = toFieldValue(descriptor.shape, fields[descriptor.name] ?? "");
      if (value !== undefined && value !== null) values[descriptor.name] = value;
    }
    return {
      event,
      matcher: matcher.trim() === "" ? null : matcher.trim(),
      handlerType,
      fields: values,
    };
  }, [registry, fields, event, matcher, handlerType]);

  const operations = useCallback(
    (remove: boolean): HookEditOperation[] => {
      if (remove && entry) {
        return [
          {
            kind: "delete_handler",
            locator: {
              event: entry.event,
              groupIndex: entry.groupIndex,
              handlerIndex: entry.handlerIndex,
            },
          },
        ];
      }
      if (entry) {
        return [
          {
            kind: "update_handler",
            locator: {
              event: entry.event,
              groupIndex: entry.groupIndex,
              handlerIndex: entry.handlerIndex,
            },
            draft: draft(),
          },
        ];
      }
      return [{ kind: "create_handler", draft: draft() }];
    },
    [entry, draft]
  );

  const loadRecovery = useCallback(async () => {
    try {
      setRecovery(await api.getHookRecovery(projectId, source.id));
    } catch {
      // A source with no managed history simply has nothing to restore.
      setRecovery(null);
    }
  }, [projectId, source.id]);

  useEffect(() => {
    let active = true;
    api
      .getHookRecovery(projectId, source.id)
      .then((result) => {
        if (active) setRecovery(result);
      })
      .catch(() => {
        // A source with no managed history simply has nothing to restore.
        if (active) setRecovery(null);
      });
    return () => {
      active = false;
    };
  }, [projectId, source.id]);

  const runPreview = useCallback(
    async (remove: boolean) => {
      const requestId = requestIdRef.current + 1;
      requestIdRef.current = requestId;
      setState("previewing");
      setError(null);
      try {
        const result = await api.previewHookChange(projectId, source.id, operations(remove));
        if (requestIdRef.current !== requestId) return;
        setPreview(result);
        setState("preview_ready");
      } catch (err) {
        if (requestIdRef.current !== requestId) return;
        setPreview(null);
        setState("editing");
        setError(localized(err));
      }
    },
    [projectId, source.id, operations, localized]
  );

  const runApply = useCallback(
    async (remove: boolean) => {
      if (!preview || !preview.canApply) return;
      const requestId = requestIdRef.current + 1;
      requestIdRef.current = requestId;
      setState("applying");
      setError(null);
      try {
        await api.applyHookChange(
          projectId,
          source.id,
          preview.baseRevision,
          operations(remove)
        );
        if (requestIdRef.current !== requestId) return;
        setState("applied");
        setPreview(null);
        await loadRecovery();
        onApplied();
      } catch (err) {
        if (requestIdRef.current !== requestId) return;
        setState("editing");
        setPreview(null);
        setError(localized(err));
      }
    },
    [preview, projectId, source.id, operations, loadRecovery, onApplied, localized]
  );

  const runRestorePreview = useCallback(async () => {
    if (!recovery) return;
    setError(null);
    try {
      setRestorePreview(
        await api.previewHookRestore(projectId, source.id, recovery.backupId)
      );
    } catch (err) {
      setRestorePreview(null);
      setError(localized(err));
    }
  }, [recovery, projectId, source.id, localized]);

  const runRestoreApply = useCallback(async () => {
    if (!recovery || !restorePreview || !restorePreview.canApply) return;
    setError(null);
    try {
      await api.applyHookRestore(
        projectId,
        source.id,
        recovery.backupId,
        restorePreview.baseRevision
      );
      setRestorePreview(null);
      await loadRecovery();
      onApplied();
    } catch (err) {
      setError(localized(err));
    }
  }, [recovery, restorePreview, projectId, source.id, loadRecovery, onApplied, localized]);

  const busy = state === "previewing" || state === "applying";
  const canApply = state === "preview_ready" && preview !== null && preview.canApply;
  const issueFor = (field: string) =>
    preview?.validationIssues.find((issue) => issue.field === field) ?? null;

  return (
    <section className="flex flex-col gap-3 rounded border border-[var(--color-accent)] bg-[var(--color-surface)] p-4">
      <header className="flex items-center gap-2">
        <h3 className="text-sm font-semibold text-[var(--color-text-primary)]">
          {entry ? t("hooks.edit.editTitle") : t("hooks.edit.createTitle")}
        </h3>
        <span className="font-mono text-xs text-[var(--color-text-secondary)]">
          {source.displayPath}
        </span>
        <button
          type="button"
          className="ml-auto rounded p-1 text-[var(--color-text-secondary)]"
          onClick={onClose}
          aria-label={t("common.close")}
        >
          <X className="h-4 w-4" />
        </button>
      </header>

      <div className="grid gap-3 md:grid-cols-3">
        <label className="flex flex-col gap-1 text-xs text-[var(--color-text-secondary)]">
          {t("hooks.edit.event")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={event}
            onChange={(e) => {
              setEvent(e.target.value);
              invalidate();
            }}
          >
            {registry.events.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          {issueFor("event") && (
            <span className="text-red-500">{t("hooks.edit.issue.unknown_event")}</span>
          )}
        </label>

        <label className="flex flex-col gap-1 text-xs text-[var(--color-text-secondary)]">
          {t("hooks.edit.matcher")}
          <input
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={matcher}
            onChange={(e) => {
              setMatcher(e.target.value);
              invalidate();
            }}
          />
        </label>

        <label className="flex flex-col gap-1 text-xs text-[var(--color-text-secondary)]">
          {t("hooks.edit.handlerType")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={handlerType}
            onChange={(e) => {
              setHandlerType(e.target.value);
              invalidate();
            }}
          >
            {registry.handlerTypes.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          {issueFor("handlerType") && (
            <span className="text-red-500">{t("hooks.edit.issue.unknown_handler_type")}</span>
          )}
        </label>
      </div>

      <div className="grid gap-2 md:grid-cols-2">
        {registry.fields.map((descriptor) => {
          const issue = issueFor(descriptor.name);
          return (
            <label
              key={descriptor.name}
              className="flex flex-col gap-1 text-xs text-[var(--color-text-secondary)]"
            >
              <span>
                {descriptor.name}
                <span className="ml-1 opacity-60">
                  {t(`hooks.edit.shape.${descriptor.shape}`)}
                </span>
              </span>
              {descriptor.shape === "bool" ? (
                <select
                  className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
                  value={fields[descriptor.name] ?? ""}
                  onChange={(e) => {
                    setFields((prev) => ({ ...prev, [descriptor.name]: e.target.value }));
                    invalidate();
                  }}
                >
                  <option value="">{t("hooks.edit.unset")}</option>
                  <option value="true">true</option>
                  <option value="false">false</option>
                </select>
              ) : (
                <input
                  className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
                  value={fields[descriptor.name] ?? ""}
                  onChange={(e) => {
                    setFields((prev) => ({ ...prev, [descriptor.name]: e.target.value }));
                    invalidate();
                  }}
                />
              )}
              {issue && (
                <span className="text-red-500">{t(`hooks.edit.issue.${issue.code}`)}</span>
              )}
            </label>
          );
        })}
      </div>

      {unknownFields.length > 0 && (
        <div className="flex flex-col gap-1 rounded border border-[var(--color-border)] p-2">
          <p className="text-xs text-[var(--color-text-secondary)]">
            {t("hooks.edit.unknownFields")}
          </p>
          {unknownFields.map((field) => (
            <p key={field.key} className="font-mono text-xs text-[var(--color-text-secondary)]">
              {field.key}: {field.value}
            </p>
          ))}
        </div>
      )}

      {error && (
        <p className="inline-flex items-start gap-1 text-sm text-red-500">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          {error}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => void runPreview(false)}
          className="inline-flex items-center gap-1 rounded border border-[var(--color-border)] px-2 py-1 text-sm"
        >
          <Eye className="h-3.5 w-3.5" />
          {t("hooks.edit.preview")}
        </button>
        <button
          type="button"
          disabled={!canApply}
          onClick={() => void runApply(false)}
          className={cn(
            "inline-flex items-center gap-1 rounded border px-2 py-1 text-sm",
            canApply
              ? "border-[var(--color-accent)] text-[var(--color-text-primary)]"
              : "border-[var(--color-border)] opacity-50"
          )}
        >
          <Save className="h-3.5 w-3.5" />
          {t("hooks.edit.apply")}
        </button>
        {entry && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void runPreview(true)}
            className="inline-flex items-center gap-1 rounded border border-[var(--color-border)] px-2 py-1 text-sm text-red-500"
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t("hooks.edit.delete")}
          </button>
        )}
        {busy && <Loader2 className="h-4 w-4 animate-spin text-[var(--color-text-secondary)]" />}
        {state === "applied" && (
          <span className="text-xs text-[var(--color-text-secondary)]">
            {t("hooks.edit.applied")}
          </span>
        )}
      </div>

      {preview && (
        <div className="flex flex-col gap-1">
          <p className="text-xs text-[var(--color-text-secondary)]">
            {preview.wouldCreateFile
              ? t("hooks.edit.wouldCreateFile")
              : t("hooks.edit.diffHeading")}
          </p>
          <DocumentDiffViewer
            original={preview.beforeCanonicalText}
            updated={preview.afterCanonicalText}
          />
        </div>
      )}

      <div className="flex flex-col gap-2 border-t border-[var(--color-border)] pt-3">
        <h4 className="text-sm font-semibold text-[var(--color-text-primary)]">
          {t("hooks.edit.restoreHeading")}
        </h4>
        {!recovery ? (
          <p className="text-xs text-[var(--color-text-secondary)]">
            {t("hooks.edit.noRecovery")}
          </p>
        ) : (
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs text-[var(--color-text-secondary)]">
              {recovery.canRestore
                ? t("hooks.edit.recoveryReady")
                : t("hooks.edit.recoveryStale")}
            </span>
            <button
              type="button"
              onClick={() => void runRestorePreview()}
              className="inline-flex items-center gap-1 rounded border border-[var(--color-border)] px-2 py-1 text-sm"
            >
              <Eye className="h-3.5 w-3.5" />
              {t("hooks.edit.restorePreview")}
            </button>
            <button
              type="button"
              disabled={!restorePreview || !restorePreview.canApply}
              onClick={() => void runRestoreApply()}
              className={cn(
                "inline-flex items-center gap-1 rounded border px-2 py-1 text-sm",
                restorePreview?.canApply
                  ? "border-[var(--color-accent)]"
                  : "border-[var(--color-border)] opacity-50"
              )}
            >
              <RotateCcw className="h-3.5 w-3.5" />
              {t("hooks.edit.restoreApply")}
            </button>
          </div>
        )}
        {restorePreview && (
          <DocumentDiffViewer
            original={restorePreview.beforeCanonicalText}
            updated={restorePreview.afterCanonicalText}
          />
        )}
      </div>
    </section>
  );
}
