import { useEffect } from "react";

import { Dropzone } from "./components/Dropzone";
import { FileList } from "./components/FileList";
import { Header } from "./components/Header";
import { PreflightDialog } from "./components/PreflightDialog";
import { PreviewDialog } from "./components/PreviewDialog";
import { SettingsPanel } from "./components/SettingsPanel";
import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { SummaryPanel } from "./components/SummaryPanel";
import { Toasts } from "./components/Toasts";
import { useFileDrop } from "./hooks/useFileDrop";
import { useShortcuts } from "./hooks/useShortcuts";
import { useThemeEffect } from "./hooks/useThemeEffect";
import { useImagoStore } from "./store/useImagoStore";

export function App() {
  const init = useImagoStore((state) => state.init);
  const hasFiles = useImagoStore((state) => state.order.length > 0);
  const dialog = useImagoStore((state) => state.dialog);
  const dragActive = useImagoStore((state) => state.dragActive);

  useEffect(() => {
    void init();
  }, [init]);
  useThemeEffect();
  useShortcuts();
  useFileDrop();

  return (
    <div className={`app${dragActive ? " app--dragging" : ""}`}>
      <Header />
      <div className="workspace">
        <main className="workspace__main">
          <Dropzone />
          {hasFiles && <FileList />}
        </main>
        <SettingsPanel />
      </div>
      <SummaryPanel />
      <Toasts />
      {dialog?.kind === "preflight" && <PreflightDialog run={dialog.run} />}
      {dialog?.kind === "preview" && <PreviewDialog id={dialog.id} />}
      {dialog?.kind === "shortcuts" && <ShortcutsDialog />}
    </div>
  );
}
