import { act, fireEvent, render, screen, within } from "@testing-library/react";

import { App } from "./App";
import { ROW_HEIGHT, visibleRange } from "./components/FileList";
import { useImagoStore } from "./store/useImagoStore";
import { defaultConfig } from "./utils/defaults";
import { createEntry } from "./utils/files";

function resetStore(language: "pl" | "en" = "pl") {
  useImagoStore.setState({
    config: defaultConfig(),
    language,
    entries: {},
    order: [],
    nextId: 1,
    dialog: null,
    toasts: [],
    filter: "all",
  });
}

describe("App", () => {
  beforeEach(() => {
    vi.spyOn(window.navigator, "languages", "get").mockReturnValue(["pl-PL"]);
    resetStore();
  });
  afterEach(() => vi.restoreAllMocks());

  it("shows the product name, taglines and the add buttons", async () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: /Imago/ })).toBeTruthy();
    expect(screen.getByText("Fast. Local. Smaller images.")).toBeTruthy();
    expect(await screen.findByRole("button", { name: /Dodaj pliki/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Dodaj folder/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Rozpocznij optymalizację/ })).toBeTruthy();
  });

  it("switches the language", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "EN" }));
    expect(screen.getByRole("button", { name: /Add files/ })).toBeTruthy();
  });

  it("lists added files with their status", async () => {
    render(<App />);
    act(() => {
      useImagoStore.setState({
        entries: {
          1: createEntry(1, { path: "/zdjęcia/żółw.png", root: null, size: 2048 }, null),
          2: { ...createEntry(2, { path: "/zdjęcia/zepsuty.png", root: null, size: 10 }, null), status: "failed" },
        },
        order: [1, 2],
        nextId: 3,
      });
    });
    const list = await screen.findByRole("list", { name: /Plików: 2/ });
    expect(within(list).getByText("żółw.png")).toBeTruthy();
    expect(within(list).getByText("Oczekuje")).toBeTruthy();
    expect(within(list).getByText("Błąd")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Błąd/ }));
    const filtered = await screen.findByRole("list", { name: /Plików: 1/ });
    expect(within(filtered).queryByText("żółw.png")).toBeNull();
  });

  it("offers every output format and the quality setting", async () => {
    render(<App />);
    for (const label of ["Automatycznie", "Zachowaj format", "JPEG", "PNG", "WebP", "AVIF", "TIFF", "JPEG XL"]) {
      expect(await screen.findByText(label)).toBeTruthy();
    }
    const quality = screen.getByRole("slider", { name: "Jakość" }) as HTMLInputElement;
    expect(quality.value).toBe("82");
    expect(screen.getByRole("button", { name: /Zrównoważona/ }).getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(screen.getByRole("button", { name: /Wysoka jakość/ }));
    expect(quality.value).toBe("92");
  });

  it("collapses settings cards into one-line summaries", async () => {
    render(<App />);
    const resize = await screen.findByRole("button", { name: /Skalowanie/ });
    expect(resize.getAttribute("aria-expanded")).toBe("false");
    expect(within(resize).getByText("Oryginalne wymiary")).toBeTruthy();

    fireEvent.click(resize);
    expect(resize.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByRole("radio", { name: "Maksymalne wymiary" })).toBeTruthy();
  });
});

describe("virtual list", () => {
  it("renders only visible rows plus overscan", () => {
    const viewport = ROW_HEIGHT * 10;
    expect(visibleRange(0, viewport, 10_000)).toEqual({ start: 0, end: 16 });
    const middle = visibleRange(ROW_HEIGHT * 5000, viewport, 10_000);
    expect(middle.start).toBe(4994);
    expect(middle.end).toBe(5016);
    expect(visibleRange(0, viewport, 3)).toEqual({ start: 0, end: 3 });
  });
});
