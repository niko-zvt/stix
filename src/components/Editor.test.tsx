import { describe, it, expect, afterEach, vi } from "vitest";
import { readFileSync } from "node:fs";
import { render, fireEvent, act, waitFor, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { createRef } from "react";
import { getCM, Vim } from "@replit/codemirror-vim";
import Editor, { type EditorRef } from "./Editor";
import { setLocale } from "@/i18n";
import { foldEffect, foldedRanges } from "@codemirror/language";
import { undo } from "@codemirror/commands";

// Editor pulls in Tauri APIs through its image/link handlers; stub the bridge
// so the component can mount under jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {}),
    label: "postit",
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));

// jsdom omits these layout APIs, so spyOn cannot wrap an existing method.
const originalRangeGeometry = {
  getClientRects: Object.getOwnPropertyDescriptor(Range.prototype, "getClientRects"),
  getBoundingClientRect: Object.getOwnPropertyDescriptor(Range.prototype, "getBoundingClientRect"),
};

afterEach(() => {
  setLocale("en");
  vi.restoreAllMocks();
  for (const [key, descriptor] of Object.entries(originalRangeGeometry)) {
    if (descriptor) Object.defineProperty(Range.prototype, key, descriptor);
    else Reflect.deleteProperty(Range.prototype, key);
  }
});

/// Regression guard for the placeholder surviving a language switch.
///
/// The editor is created once with empty deps, so the placeholder used to be
/// frozen at whatever the locale was at mount. Switching to English left the
/// Chinese string on screen until the window was recreated.
describe("Editor placeholder", () => {
  it.each([{ metaKey: true }, { ctrlKey: true }])("scopes select-all to the focused table cell (%j)", (modifiers) => {
    const ref = createRef<EditorRef>();
    const content = "Intro\n\n| A | B |\n| --- | --- |\n| C | D |\n\nTail";
    render(<Editor ref={ref} onChange={() => {}} initialContent={content} />);
    const view = ref.current!.getView()!;
    const cell = view.dom.querySelector<HTMLElement>("tbody .cm-table-cell")!;
    cell.focus();
    expect(fireEvent.keyDown(cell, { key: "a", ...modifiers })).toBe(false);
    expect(window.getSelection()?.toString()).toBe("C");
    expect(cell.contains(window.getSelection()!.anchorNode)).toBe(true);
    expect(view.state.doc.toString()).toBe(content);
  });

  it("keeps later table write positions current and leaves typing/composition to the browser", () => {
    const ref = createRef<EditorRef>();
    const first = "| A | B |\n| --- | --- |\n| C | D |";
    const second = "| E | F |\n| --- | --- |\n| G | H |";
    render(<Editor ref={ref} onChange={() => {}} initialContent={`Intro\n\n${first}\n\nBetween\n\n${second}\n\nTail`} />);
    const view = ref.current!.getView()!;
    const firstCell = view.dom.querySelector<HTMLElement>("tbody .cm-table-cell")!;
    firstCell.textContent = "A longer first value";
    fireEvent.input(firstCell);
    const laterCells = view.dom.querySelectorAll("table")[1].querySelectorAll<HTMLElement>("tbody .cm-table-cell");
    const later = laterCells[0];
    later.focus();
    expect(fireEvent.keyDown(later, { key: "a" })).toBe(true);
    expect(fireEvent.keyDown(later, { key: "Enter", isComposing: true })).toBe(true);
    expect(later).toHaveFocus();
    later.textContent = "Updated second value";
    fireEvent.input(later);
    expect(view.state.doc.toString()).toBe(`Intro\n\n${first.replace("| C |", "| A longer first value |")}\n\nBetween\n\n${second.replace("| G |", "| Updated second value |")}\n\nTail`);
    expect(fireEvent.keyDown(later, { key: "Tab" })).toBe(false);
    expect(laterCells[1]).toHaveFocus();
  });

  it.each(["C | X", "Backslash \\| pipe", "First\nsecond & <br>", '<img src="https://example.test/pixel">'])("round-trips literal table cell text %j without changing adjacent cells", async (text) => {
    const ref = createRef<EditorRef>();
    const { unmount } = render(<Editor ref={ref} onChange={() => {}} initialContent={"Intro\n\n| A | B |\n| --- | --- |\n| C | D |\n\nTail"} />);
    const view = ref.current!.getView()!;
    const cell = view.dom.querySelector<HTMLElement>("tbody .cm-table-cell")!;
    cell.textContent = text;
    fireEvent.input(cell);
    const saved = view.state.doc.toString();
    unmount();
    render(<Editor ref={ref} onChange={() => {}} initialContent={saved} />);
    const cells = ref.current!.getView()!.dom.querySelectorAll("tbody .cm-table-cell");
    expect(cells).toHaveLength(2);
    expect(cells[0].textContent).toBe(text);
    expect(cells[1].textContent).toBe("D");
    expect(ref.current!.getView()!.dom.querySelector("img")).toBeNull();
  });

  it.each(["", "Prefix "])("preserves an edited table through language changes after preceding edit %j", async (prefix) => {
    Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect = () => new DOMRect();
    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent={"Intro\n\n| A | B |\n| --- | --- |\n| C | D |\n\nTail"} />);
    const view = ref.current!.getView()!;
    await waitFor(() => expect(view.dom.querySelector(".cm-table-cell")).not.toBeNull());
    if (prefix) act(() => view.dispatch({ changes: { from: 0, insert: prefix } }));
    const cell = view.dom.querySelector<HTMLElement>(".cm-table-cell")!;
    cell.focus();
    cell.textContent = "Edited A";
    fireEvent.input(cell);
    expect(view.state.doc.toString()).toBe(`${prefix}Intro\n\n| Edited A | B |\n| --- | --- |\n| C | D |\n\nTail`);
    await act(async () => setLocale("zh-CN"));
    expect(view.dom.querySelector(".cm-table-cell")).toBe(cell);
    expect(cell).toHaveFocus();
    expect(cell).toHaveTextContent("Edited A");
    expect(view.dom.querySelector(".cm-table-add-row")).toHaveAttribute("title", "在下方添加行");
    await act(async () => setLocale("en"));
    expect(view.dom.querySelector(".cm-table-cell")).toBe(cell);
    expect(cell).toHaveFocus();
  });

  it("refreshes existing widget labels without resetting edits, folds or image consent", async () => {
    Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect = () => new DOMRect();
    setLocale("en");
    const ref = createRef<EditorRef>();
    const onChange = vi.fn();
    const original = "Intro\n\n# Tasks\n- [x] Done\n- [ ] Pending\n\n# Folded\nHidden body\n\n# Table\n| A | B |\n| --- | --- |\n| C | D |\n\n# Images\n![User alt](https://images.example/blocked.png)\n![](https://images.example/once.png)\n";
    render(<Editor ref={ref} onChange={onChange} initialContent={original} placeholder="Fixed name" />);
    const view = ref.current!.getView()!;
    act(() => view.dispatch({ changes: { from: 0, insert: "Draft " }, selection: { anchor: 6 } }));
    const heading = view.state.doc.line(7);
    const hidden = view.state.doc.line(8);
    act(() => view.dispatch({ effects: foldEffect.of({ from: heading.to, to: hidden.to }) }));
    const savedState = view.state;
    const doc = savedState.doc.toString();
    const cell = view.dom.querySelector<HTMLElement>(".cm-table-cell")!;
    expect(cell).not.toBeNull();
    const wrappers = view.dom.querySelectorAll<HTMLElement>(".cm-image-widget");
    expect(wrappers).toHaveLength(2);
    const blocked = wrappers[0];
    const loaded = wrappers[1];
    fireEvent.click(loaded.querySelector("button")!);
    const img = loaded.querySelector("img")!;
    expect(img).not.toBeNull();
    const assignSource = vi.spyOn(HTMLImageElement.prototype, "src", "set");
    const callsBeforeLocale = onChange.mock.calls.length;
    cell.focus();

    await act(async () => setLocale("zh-CN"));
    expect(view.dom.querySelector('[role="checkbox"][aria-checked="true"]')).toHaveAttribute("aria-label", "已完成的任务");
    expect(view.dom.querySelector(".cm-heading-chevron-folded")).toHaveAttribute("aria-label", "展开此节");
    expect(view.dom.querySelector(".cm-heading-fold-placeholder")).toHaveAttribute("aria-label", "已折叠的内容。点按以展开。");
    expect(view.dom.querySelector(".cm-table-add-row")).toHaveAttribute("title", "在下方添加行");
    expect(blocked).toHaveTextContent("为保护隐私，已阻止来自 images.example 的远程图片: User alt");
    expect(blocked.querySelector("button")).toHaveTextContent("仅加载一次");
    expect(blocked.querySelector("img")).toBeNull();
    expect(loaded.querySelector("img")).toBe(img);
    expect(view.dom.querySelector(".cm-table-cell")).toBe(cell);
    expect(cell).toHaveFocus();

    await act(async () => setLocale("en"));
    expect(view.dom.querySelector('[role="checkbox"][aria-checked="true"]')).toHaveAttribute("aria-label", "Completed task");
    expect(view.dom.querySelector(".cm-heading-chevron-folded")).toHaveAttribute("aria-label", "Unfold section");
    expect(view.dom.querySelector(".cm-heading-fold-placeholder")).toHaveAttribute("title", "Unfold");
    expect(view.dom.querySelector(".cm-table-add-col")).toHaveAttribute("title", "Add column to the right");
    expect(blocked.querySelector("button")).toHaveTextContent("Load once");
    expect(ref.current!.getView()).toBe(view);
    expect(view.state.doc.toString()).toBe(doc);
    expect(view.state.selection.eq(savedState.selection)).toBe(true);
    const folds: [number, number][] = [];
    foldedRanges(view.state).between(0, view.state.doc.length, (from, to) => { folds.push([from, to]); });
    expect(folds).toEqual([[heading.to, hidden.to]]);
    expect(onChange).toHaveBeenCalledTimes(callsBeforeLocale);
    expect(assignSource).not.toHaveBeenCalled();
    assignSource.mockRestore();
    act(() => { undo(view); });
    expect(view.state.doc.toString()).toBe(original);
  });

  it("follows the active locale without remounting", async () => {
    setLocale("en");

    render(<Editor onChange={() => {}} initialContent="" />);

    // CodeMirror renders the placeholder into .cm-placeholder
    const readPlaceholder = () =>
      document.querySelector(".cm-placeholder")?.textContent ?? "";

    expect(readPlaceholder()).toBe("Start typing...");

    await act(async () => {
      setLocale("zh-CN");
    });
    expect(readPlaceholder()).toBe("开始输入…");

    // The bug: switching back left the previous string in place.
    await act(async () => {
      setLocale("en");
    });
    expect(readPlaceholder()).toBe("Start typing...");
  });

  it("prefers an explicit placeholder prop over the default", () => {
    setLocale("en");
    render(<Editor onChange={() => {}} initialContent="" placeholder="Custom text" />);
    expect(document.querySelector(".cm-placeholder")?.textContent).toBe("Custom text");
  });
});

describe("Editor Vim commands", () => {
  it.each(["wq", "x", "q", "q!"])("routes :%s to the current callback after a re-render", (command) => {
    const ref = createRef<EditorRef>();
    const originalCallback = vi.fn();
    const onSave = vi.fn();
    const onClose = vi.fn();
    const { rerender } = render(
      <Editor ref={ref} onChange={() => {}} vimEnabled
        onVimSaveAndClose={originalCallback} onVimCloseWithoutSaving={originalCallback} />,
    );
    const view = ref.current!.getView()!;
    rerender(
      <Editor ref={ref} onChange={() => {}} vimEnabled
        onVimSaveAndClose={onSave} onVimCloseWithoutSaving={onClose} />,
    );

    const cm = getCM(view);
    if (!cm?.state.vim) throw new Error("Vim state was not initialized");
    // handleEx requires Vim state; getCM's type also permits non-Vim editors.
    act(() => Vim.handleEx(cm as Parameters<typeof Vim.handleEx>[0], command));

    expect(ref.current!.getView()).toBe(view);
    expect(originalCallback).not.toHaveBeenCalled();
    expect(onSave).toHaveBeenCalledTimes(command === "wq" || command === "x" ? 1 : 0);
    expect(onClose).toHaveBeenCalledTimes(command === "q" || command === "q!" ? 1 : 0);
  });

  it("reports mode changes from the Vim editor", () => {
    const ref = createRef<EditorRef>();
    const onModeChange = vi.fn();
    render(<Editor ref={ref} onChange={() => {}} vimEnabled onVimModeChange={onModeChange} />);

    act(() => Vim.handleKey(getCM(ref.current!.getView()!)!, "i", "user"));

    expect(onModeChange).toHaveBeenLastCalledWith("insert");
  });
});

describe("Editor link shortcut", () => {
  it("inserts a link around the selected text through the editor keymap", () => {
    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent="Stix" />);
    const view = ref.current!.getView()!;
    act(() => view.dispatch({ selection: { anchor: 0, head: 4 } }));

    fireEvent.keyDown(view.contentDOM, { key: "k", code: "KeyK", ctrlKey: true });

    expect(view.state.doc.toString()).toBe("[Stix](url)");
    expect(view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)).toBe("url");
  });
});

describe("Editor text selection", () => {
  it("keeps note text selectable when the window chrome is not", () => {
    const css = readFileSync("src/styles/globals.css", "utf8");
    const root = css.match(/html,\s*body,\s*#root\s*\{[^}]*\}/)?.[0] ?? "";
    expect(root).not.toMatch(/user-select:\s*none/);
    expect(css).toMatch(/\.cm-content,\s*\.cm-content \.cm-line\s*\{[^}]*-webkit-user-select:\s*text !important/s);
    expect(css).toMatch(/\.cm-scroller > \.cm-selectionLayer\s*\{[^}]*contain:\s*none !important/s);
  });

  it("extends the selection while the pointer drags across the note", () => {
    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent="hello" />);
    const view = ref.current!.getView()!;
    view.posAndSideAtCoords = (coords) => ({
      pos: coords.x < 30 ? 0 : 5,
      assoc: 1,
    });

    fireEvent.mouseDown(view.contentDOM, { button: 0, clientX: 10, clientY: 12 });
    fireEvent.mouseMove(document, { buttons: 1, clientX: 80, clientY: 12 });
    fireEvent.mouseUp(document, { button: 0, clientX: 80, clientY: 12 });

    expect(view.state.selection.main.from).toBe(0);
    expect(view.state.selection.main.to).toBe(5);
  });

  it("keeps a left-drag selection when mousemove reports no buttons", () => {
    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent="hello" />);
    const view = ref.current!.getView()!;
    view.posAtCoords = (coords) => (coords.x < 30 ? 0 : 5);

    const down = new MouseEvent("mousedown", {
      button: 0,
      detail: 1,
      clientX: 10,
      clientY: 12,
      bubbles: true,
      cancelable: true,
    });
    view.contentDOM.dispatchEvent(down);
    document.dispatchEvent(new MouseEvent("mousemove", {
      buttons: 0,
      clientX: 80,
      clientY: 12,
      bubbles: true,
    }));
    document.dispatchEvent(new MouseEvent("mouseup", {
      button: 0,
      clientX: 80,
      clientY: 12,
      bubbles: true,
    }));

    expect(down.defaultPrevented).toBe(false);
    expect(view.state.selection.main.from).toBe(0);
    expect(view.state.selection.main.to).toBe(5);
  });

  it("inserts a latex block from the format toolbar", () => {
    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent="E=mc^2" showFormatToolbar />);
    const view = ref.current!.getView()!;
    act(() => view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } }));
    fireEvent.click(screen.getByRole("button", { name: "Formula" }));
    expect(ref.current!.getText()).toBe("$$\nE=mc^2\n$$");
  });

  it("renders a latex block while the cursor is outside it", () => {
    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent={"$$\nE=mc^2\n$$\n\nnote"} />);
    const view = ref.current!.getView()!;
    act(() => view.dispatch({ selection: { anchor: view.state.doc.length } }));
    const math = view.dom.querySelector(".cm-math");
    expect(math?.tagName).toBe("SPAN");
    expect(math?.querySelector(".katex")).not.toBeNull();
    expect(math?.querySelector(".katex-display")).toBeNull();
    expect(view.dom.textContent).toContain("note");
  });

  it("renders an inline formula in the same line as the surrounding text", () => {
    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent="hello $$x=f(y)$$ there" />);
    const view = ref.current!.getView()!;
    act(() => view.dispatch({ selection: { anchor: view.state.doc.length } }));
    const line = view.dom.querySelector(".cm-line");
    expect(line?.querySelector(".cm-math")?.tagName).toBe("SPAN");
    expect(line?.querySelector(".katex-display")).toBeNull();
    expect(line?.textContent).toContain("hello");
    expect(line?.textContent).toContain("there");
  });
});

describe("Editor Control clipboard and history", () => {
  it("selects, copies, pastes, cuts, and undoes with Control", async () => {
    let clipboard = "";
    vi.mocked(invoke).mockImplementation((cmd: unknown, args?: unknown) => {
      if (cmd === "write_clipboard_text") {
        clipboard = (args as { text?: string } | undefined)?.text ?? "";
        return Promise.resolve(null);
      }
      if (cmd === "read_clipboard_text") return Promise.resolve(clipboard);
      return Promise.resolve(null);
    });

    const ref = createRef<EditorRef>();
    render(<Editor ref={ref} onChange={() => {}} initialContent="hello" />);
    const view = ref.current!.getView()!;
    const press = (key: string) =>
      fireEvent.keyDown(view.contentDOM, {
        key,
        code: `Key${key.toUpperCase()}`,
        ctrlKey: true,
      });

    act(() => view.dispatch({ selection: { anchor: 0, head: 5 } }));
    press("c");
    await waitFor(() => expect(clipboard).toBe("hello"));

    act(() => view.dispatch({ selection: { anchor: 5, head: 5 } }));
    press("v");
    await waitFor(() => expect(view.state.doc.toString()).toBe("hellohello"));

    press("z");
    expect(view.state.doc.toString()).toBe("hello");
    press("y");
    expect(view.state.doc.toString()).toBe("hellohello");

    press("a");
    expect(view.state.selection.main.from).toBe(0);
    expect(view.state.selection.main.to).toBe(view.state.doc.length);

    press("x");
    await waitFor(() => expect(view.state.doc.toString()).toBe(""));
    expect(clipboard).toBe("hellohello");
  });
});
