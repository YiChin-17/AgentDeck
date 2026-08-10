import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { StatusBanner } from "./StatusBanner";
import { useApp } from "../context/AppContext";
import { getErrorMessage } from "../lib/error";

/**
 * Global notice that the configured Library is unreachable.
 *
 * Rendered from the shared shell so it is visible on every page, including the
 * Board routes. The state comes from the backend verdict — the path string
 * alone cannot tell one volume from another.
 */
export function LibraryOfflineBanner() {
  const { t } = useTranslation();
  const { libraryAvailability, retryLibrary } = useApp();
  const [retrying, setRetrying] = useState(false);

  if (!libraryAvailability || libraryAvailability.state !== "offline") return null;

  const handleRetry = async () => {
    if (retrying) return;
    setRetrying(true);
    try {
      const next = await retryLibrary();
      if (next.state === "online") {
        toast.success(t("library.offline.reconnected"));
      } else {
        toast.error(t(`library.reason.${next.reason}`));
      }
    } catch (error) {
      toast.error(getErrorMessage(error, t("library.offline.retryFailed")));
    } finally {
      setRetrying(false);
    }
  };

  return (
    <StatusBanner
      compact
      tone="danger"
      title={t("library.offline.title")}
      description={t("library.offline.description", {
        path: libraryAvailability.configured_path,
        reason: t(`library.reason.${libraryAvailability.reason}`),
      })}
      actionLabel={retrying ? t("library.offline.retrying") : t("common.retry")}
      onAction={handleRetry}
    />
  );
}
