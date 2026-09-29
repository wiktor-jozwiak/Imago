import { useT } from "../hooks/useT";
import type { MessageKey } from "../i18n";
import { useImagoStore } from "../store/useImagoStore";
import { Modal } from "./Modal";

/** Lists the keyboard shortcuts (⌘ replaces Ctrl on macOS). */
export function ShortcutsDialog() {
  const t = useT();
  const closeDialog = useImagoStore((state) => state.closeDialog);
  const mac = useImagoStore((state) => state.appInfo?.platform === "macos");
  const mod = mac ? "⌘" : "Ctrl";
  const rows: [string, MessageKey][] = [
    [`${mod}+O`, "shortcuts.addFiles"],
    [`${mod}+Shift+O`, "shortcuts.addFolder"],
    [`${mod}+Enter`, "shortcuts.start"],
    ["Esc", "shortcuts.cancel"],
    [`${mod}+Shift+L`, "shortcuts.theme"],
    ["?", "shortcuts.help"],
  ];
  return (
    <Modal title={t("shortcuts.title")} onClose={closeDialog}>
      <table className="shortcuts">
        <tbody>
          {rows.map(([keys, label]) => (
            <tr key={keys}>
              <td>
                <kbd>{keys}</kbd>
              </td>
              <td>{t(label)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Modal>
  );
}
