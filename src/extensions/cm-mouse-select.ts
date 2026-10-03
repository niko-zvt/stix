/**
 * Left-drag selection in the note editor.
 *
 * CodeMirror calls preventDefault on mousedown, then drops the gesture when
 * WKWebView reports buttons === 0 on the next mousemove. That also cancels
 * the browser's own highlight. This listener runs first, lets the browser
 * keep the gesture, and mirrors it into the editor selection.
 */

import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";

const widgetSelector = [
  "button",
  "a",
  "input",
  "textarea",
  ".cm-task-checkbox",
  ".cm-heading-chevron",
  ".cm-image-widget",
  ".cm-table-widget",
].join(", ");

class MouseSelect {
  private anchor = 0;
  private tracking = false;
  private readonly onMouseDown: (event: MouseEvent) => void;
  private readonly onMouseMove: (event: MouseEvent) => void;
  private readonly onMouseUp: () => void;

  constructor(private readonly view: EditorView) {
    this.onMouseDown = (event) => {
      if (event.button !== 0) return;
      const target = event.target;
      if (target instanceof Element && target.closest(widgetSelector)) return;
      let start: number | null = null;
      try {
        start = view.posAtCoords({ x: event.clientX, y: event.clientY });
      } catch {
        return;
      }
      if (start == null) return;
      // Keep the event from reaching CodeMirror, which would cancel it.
      event.stopImmediatePropagation();
      if (!view.hasFocus) view.focus();
      this.anchor = event.shiftKey ? view.state.selection.main.anchor : start;
      this.tracking = true;
      this.select(event.clientX, event.clientY);
      document.addEventListener("mousemove", this.onMouseMove);
      document.addEventListener("mouseup", this.onMouseUp);
    };
    this.onMouseMove = (event) => {
      if (!this.tracking) return;
      this.select(event.clientX, event.clientY);
    };
    this.onMouseUp = () => {
      this.tracking = false;
      document.removeEventListener("mousemove", this.onMouseMove);
      document.removeEventListener("mouseup", this.onMouseUp);
    };
    view.contentDOM.addEventListener("mousedown", this.onMouseDown, true);
  }

  update(_update: ViewUpdate) {}

  destroy() {
    this.view.contentDOM.removeEventListener("mousedown", this.onMouseDown, true);
    this.onMouseUp();
  }

  private select(x: number, y: number) {
    let pos: number | null = null;
    try {
      pos = this.view.posAtCoords({ x, y });
    } catch {
      return;
    }
    if (pos == null) return;
    const { anchor, head } = this.view.state.selection.main;
    if (pos === head && this.anchor === anchor) return;
    this.view.dispatch({
      selection: { anchor: this.anchor, head: pos },
      userEvent: "select.pointer",
    });
  }
}

export const dragSelect = ViewPlugin.fromClass(MouseSelect);
