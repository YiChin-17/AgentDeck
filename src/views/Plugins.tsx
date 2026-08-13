import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, Loader2, PlugZap, RefreshCw, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import { getErrorMessage } from "../lib/error";
import * as api from "../lib/tauri";
import type {
  PluginAgentInventory,
  PluginInventory,
  PluginInventoryItem,
} from "../lib/tauri";

const ALL = "all";

const PRESENCE_OPTIONS = ["installed", "available", "not_installed", "unknown"] as const;
const SCOPE_OPTIONS = ["user", "project", "local", "unknown"] as const;
const STATUS_OPTIONS = ["enabled", "disabled", "unknown"] as const;

/**
 * Read-only Plugin inventory for Codex and Claude Code.
 *
 * The page owns its response and its filters and shares neither: the inventory
 * is what two CLIs reported once, not application state, so nothing here goes
 * to the shared context or to storage that would outlive the view.
 */
export function Plugins() {
  const { t } = useTranslation();
  const [data, setData] = useState<PluginInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [agentFilter, setAgentFilter] = useState<string>(ALL);
  const [presenceFilter, setPresenceFilter] = useState<string>(ALL);
  const [scopeFilter, setScopeFilter] = useState<string>(ALL);
  const [marketplaceFilter, setMarketplaceFilter] = useState<string>(ALL);
  const [statusFilter, setStatusFilter] = useState<string>(ALL);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // Guards against a slow earlier refresh overwriting a newer one.
  const requestIdRef = useRef(0);

  const load = useCallback(async () => {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setLoading(true);
    try {
      const result = await api.getPluginInventory();
      if (requestIdRef.current !== requestId) return;
      setData(result);
      setError(null);
    } catch (err) {
      if (requestIdRef.current !== requestId) return;
      setData(null);
      setError(getErrorMessage(err, t("plugins.loadFailed")));
    } finally {
      if (requestIdRef.current === requestId) setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void load();
  }, [load]);

  // A selection can point at something the new response no longer has.
  useEffect(() => {
    setSelectedId(null);
  }, [data]);

  const unknown = t("plugins.unknown");
  const orUnknown = (value: string | null) => (value === null ? unknown : value);

  const marketplaceNames = useMemo(() => {
    if (!data) return [];
    const names = new Set<string>();
    for (const agent of data.agents) {
      for (const market of agent.marketplaces) names.add(market.name);
      for (const item of agent.items) if (item.marketplace) names.add(item.marketplace);
    }
    return Array.from(names).sort();
  }, [data]);

  const allItems = useMemo<PluginInventoryItem[]>(
    () => (data ? data.agents.flatMap((agent) => agent.items) : []),
    [data]
  );

  const agentCards = useMemo<PluginAgentInventory[]>(() => {
    if (!data) return [];
    return data.agents.filter((agent) => agentFilter === ALL || agent.agent === agentFilter);
  }, [data, agentFilter]);

  const visibleItems = useMemo<PluginInventoryItem[]>(
    () =>
      allItems.filter(
        (item) =>
          (agentFilter === ALL || item.agent === agentFilter) &&
          (presenceFilter === ALL ||
            (presenceFilter === "available"
              ? item.available === "available"
              : item.installed === presenceFilter)) &&
          (scopeFilter === ALL || item.scope === scopeFilter) &&
          (marketplaceFilter === ALL || item.marketplace === marketplaceFilter) &&
          (statusFilter === ALL || item.enabled === statusFilter)
      ),
    [allItems, agentFilter, presenceFilter, scopeFilter, marketplaceFilter, statusFilter]
  );

  const selected = useMemo(
    () => visibleItems.find((item) => item.id === selectedId) ?? null,
    [visibleItems, selectedId]
  );

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center gap-3">
        <div className="flex flex-col">
          <h1 className="text-xl font-semibold text-[var(--color-text-primary)]">
            {t("plugins.title")}
          </h1>
          <p className="text-sm text-[var(--color-text-secondary)]">{t("plugins.subtitle")}</p>
        </div>
        <span className="ml-auto inline-flex items-center gap-1 rounded-full border border-[var(--color-border)] px-3 py-1 text-xs text-[var(--color-text-secondary)]">
          <ShieldCheck className="h-3.5 w-3.5" />
          {t("plugins.readOnlyBadge")}
        </span>
      </header>

      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("plugins.filter.agent")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={agentFilter}
            onChange={(event) => setAgentFilter(event.target.value)}
          >
            <option value={ALL}>{t("plugins.filter.all")}</option>
            <option value="codex">{t("plugins.agent.codex")}</option>
            <option value="claude_code">{t("plugins.agent.claude_code")}</option>
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("plugins.filter.presence")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={presenceFilter}
            onChange={(event) => setPresenceFilter(event.target.value)}
          >
            <option value={ALL}>{t("plugins.filter.all")}</option>
            {PRESENCE_OPTIONS.map((presence) => (
              <option key={presence} value={presence}>
                {presence === "available"
                  ? t("plugins.available.available")
                  : t(`plugins.installed.${presence}`)}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("plugins.filter.scope")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={scopeFilter}
            onChange={(event) => setScopeFilter(event.target.value)}
          >
            <option value={ALL}>{t("plugins.filter.all")}</option>
            {SCOPE_OPTIONS.map((scope) => (
              <option key={scope} value={scope}>
                {t(`plugins.scope.${scope}`)}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("plugins.filter.marketplace")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={marketplaceFilter}
            onChange={(event) => setMarketplaceFilter(event.target.value)}
          >
            <option value={ALL}>{t("plugins.filter.all")}</option>
            {marketplaceNames.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>

        <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
          {t("plugins.filter.status")}
          <select
            className="rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-1 text-sm"
            value={statusFilter}
            onChange={(event) => setStatusFilter(event.target.value)}
          >
            <option value={ALL}>{t("plugins.filter.all")}</option>
            {STATUS_OPTIONS.map((status) => (
              <option key={status} value={status}>
                {t(`plugins.enabled.${status}`)}
              </option>
            ))}
          </select>
        </label>

        <button
          type="button"
          className="inline-flex items-center gap-1 rounded border border-[var(--color-border)] px-2 py-1 text-sm"
          onClick={() => void load()}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("common.refresh")}
        </button>
        {loading && <Loader2 className="h-4 w-4 animate-spin text-[var(--color-text-secondary)]" />}
      </div>

      {error && (
        <div className="rounded border border-red-400/40 bg-red-400/10 p-3 text-sm text-red-500">
          {error}
        </div>
      )}

      {data && (
        <>
          <section className="grid gap-2 md:grid-cols-2">
            {agentCards.map((agent) => (
              <article
                key={agent.agent}
                className="flex flex-col gap-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)] p-3"
              >
                <div className="flex flex-wrap items-center gap-2 text-sm">
                  <PlugZap className="h-4 w-4 text-[var(--color-text-secondary)]" />
                  <span className="font-medium text-[var(--color-text-primary)]">
                    {t(`plugins.agent.${agent.agent}`)}
                  </span>
                  <span className="ml-auto font-mono text-xs text-[var(--color-text-secondary)]">
                    {t("plugins.cliVersion")}: {orUnknown(agent.cliVersion)}
                  </span>
                </div>
                <p className="text-xs text-[var(--color-text-secondary)]">
                  {t("plugins.capabilities")}:{" "}
                  {agent.capabilities.length === 0
                    ? unknown
                    : agent.capabilities
                        .map((capability) => t(`plugins.capability.${capability}`))
                        .join("、")}
                </p>
                <p className="text-xs text-[var(--color-text-secondary)]">
                  {t("plugins.marketplaces")}:{" "}
                  {agent.marketplaces.length === 0
                    ? unknown
                    : agent.marketplaces
                        .map((market) => `${market.name} (${orUnknown(market.source)})`)
                        .join("、")}
                </p>
                {agent.diagnostics.map((diagnostic) => (
                  <p
                    key={`${diagnostic.capability}:${diagnostic.code}`}
                    className="inline-flex items-start gap-1 text-xs text-amber-500"
                  >
                    <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                    <span>
                      {t(`plugins.capability.${diagnostic.capability}`)}
                      {": "}
                      {t(`plugins.diagnostic.${diagnostic.code}`)}
                      {diagnostic.exitStatus !== null && (
                        <span className="font-mono"> ({diagnostic.exitStatus})</span>
                      )}
                    </span>
                  </p>
                ))}
                {agent.diagnostics.some((diagnostic) => diagnostic.code === "cli_missing") && (
                  <p className="text-xs text-[var(--color-text-secondary)]">
                    {t("plugins.cliMissingHint")}
                  </p>
                )}
              </article>
            ))}
          </section>

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              {t("plugins.itemsHeading")}
            </h2>
            <div className="overflow-x-auto rounded border border-[var(--color-border)]">
              <table className="w-full text-left text-sm">
                <tbody>
                  {visibleItems.map((item) => (
                    <tr
                      key={item.id}
                      className={cn(
                        "cursor-pointer border-b border-[var(--color-border)] last:border-b-0",
                        item.id === selectedId && "bg-[var(--color-surface)]"
                      )}
                      onClick={() => setSelectedId(item.id)}
                    >
                      <td className="px-3 py-2 font-medium text-[var(--color-text-primary)]">
                        {item.displayName}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`plugins.agent.${item.agent}`)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {orUnknown(item.marketplace)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`plugins.installed.${item.installed}`)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`plugins.available.${item.available}`)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`plugins.scope.${item.scope}`)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`plugins.enabled.${item.enabled}`)}
                      </td>
                      <td className="px-3 py-2 text-xs text-[var(--color-text-secondary)]">
                        {t(`plugins.update.${item.update}`)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {visibleItems.length === 0 && (
                <p className="p-3 text-sm text-[var(--color-text-secondary)]">
                  {t("plugins.noItems")}
                </p>
              )}
            </div>
          </section>

          {selected && (
            <section className="flex flex-col gap-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)] p-3 text-sm">
              <h2 className="font-semibold text-[var(--color-text-primary)]">
                {t("plugins.detailsHeading")}
              </h2>
              <p className="font-mono text-xs text-[var(--color-text-secondary)]">{selected.id}</p>
              <p className="text-xs text-[var(--color-text-secondary)]">
                {t("plugins.installedVersion")}: {orUnknown(selected.installedVersion)}
              </p>
              <p className="text-xs text-[var(--color-text-secondary)]">
                {t("plugins.availableVersion")}: {orUnknown(selected.availableVersion)}
              </p>
            </section>
          )}
        </>
      )}
    </div>
  );
}
