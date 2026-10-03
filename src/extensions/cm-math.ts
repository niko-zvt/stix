/**
 * Display math: $$ ... $$ renders with KaTeX while the cursor is outside
 * the block, and falls back to the source when the cursor is inside it.
 */

import {
  Decoration,
  EditorView,
  WidgetType,
  type DecorationSet,
} from "@codemirror/view";
import { RangeSetBuilder, StateField, type EditorState } from "@codemirror/state";
import katex from "katex";
import "katex/dist/katex.min.css";
import { rangeInSelection } from "./cm-range-utils";

class MathWidget extends WidgetType {
  constructor(readonly tex: string) {
    super();
  }

  eq(other: MathWidget) {
    return other.tex === this.tex;
  }

  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-math";
    el.innerHTML = katex.renderToString(this.tex, {
      displayMode: false,
      throwOnError: false,
    });
    return el;
  }

  ignoreEvent() {
    return false;
  }
}

const mathField = StateField.define<DecorationSet>({
  create(state) {
    return buildMath(state);
  },
  update(deco, tr) {
    if (!tr.docChanged && !tr.selection) return deco;
    return buildMath(tr.state);
  },
  provide: (field) => EditorView.decorations.from(field),
});

function buildMath(state: EditorState): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const text = state.doc.toString();
  const pattern = /\$\$([\s\S]+?)\$\$/g;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text))) {
    const tex = match[1].trim();
    if (!tex) continue;
    const from = match.index;
    const to = from + match[0].length;
    if (rangeInSelection(state.selection, from, to)) continue;
    const crossesLine = state.doc.lineAt(from).number !== state.doc.lineAt(Math.max(from, to - 1)).number;
    builder.add(
      from,
      to,
      Decoration.replace({ widget: new MathWidget(tex), block: crossesLine }),
    );
  }
  return builder.finish();
}

export const mathPlugin = mathField;
