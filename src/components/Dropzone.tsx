import { useT } from "../hooks/useT";
import { useImagoStore } from "../store/useImagoStore";
import { IconFolder, IconOffline, IconPlus, IconShield, IconSparkles, IconUpload } from "./icons";

/** Buttons for adding files and folders (native dialogs). */
export function AddButtons(props: { compact?: boolean }) {
  const t = useT();
  const addFiles = useImagoStore((state) => state.addFilesFromDialog);
  const addFolder = useImagoStore((state) => state.addFolderFromDialog);
  const scanning = useImagoStore((state) => state.scanning);
  return (
    <div className={`add-buttons${props.compact ? " add-buttons--compact" : ""}`}>
      <button
        type="button"
        className="button button--primary"
        disabled={scanning}
        onClick={() => void addFiles(t("actions.addFiles"))}
        title="Ctrl+O"
      >
        <IconPlus /> {t("actions.addFiles")}
      </button>
      <button
        type="button"
        className="button"
        disabled={scanning}
        onClick={() => void addFolder(t("actions.addFolder"))}
        title="Ctrl+Shift+O"
      >
        <IconFolder /> {t("actions.addFolder")}
      </button>
    </div>
  );
}

/**
 * Drop target. Native drag & drop is handled for the whole window
 * (`useFileDrop`); this component shows where to drop and reflects the state.
 */
export function Dropzone() {
  const t = useT();
  const hasFiles = useImagoStore((state) => state.order.length > 0);
  const dragActive = useImagoStore((state) => state.dragActive);
  const scanning = useImagoStore((state) => state.scanning);

  if (hasFiles) {
    return (
      <div className={`dropzone dropzone--compact${dragActive ? " is-active" : ""}`}>
        <IconUpload size={18} />
        <span className="dropzone__compact-text">{scanning ? t("drop.scanning") : t("drop.compact")}</span>
        <AddButtons compact />
      </div>
    );
  }

  return (
    <div className={`dropzone${dragActive ? " is-active" : ""}`} aria-live="polite">
      <div className="dropzone__icon">
        <IconUpload size={34} />
      </div>
      <h2 className="dropzone__title">{dragActive ? t("drop.active") : scanning ? t("drop.scanning") : t("drop.title")}</h2>
      <p className="dropzone__subtitle">{t("drop.subtitle")}</p>
      <AddButtons />
      <ul className="highlights">
        <li>
          <IconOffline size={16} /> {t("highlight.local")}
        </li>
        <li>
          <IconShield size={16} /> {t("highlight.safe")}
        </li>
        <li>
          <IconSparkles size={16} /> {t("highlight.auto")}
        </li>
      </ul>
    </div>
  );
}
