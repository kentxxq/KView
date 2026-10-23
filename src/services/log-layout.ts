import type { LogColor, LogHighlightRule } from "../state/settings";

export interface LogRow { id: number; raw: string; bytes: number; trimmed: boolean }
const palette: Record<LogColor, string> = { red: "#f87171", orange: "#fb923c", yellow: "#facc15", green: "#4ade80", blue: "#60a5fa", purple: "#c084fc" };

interface LayoutSegment { start: number; end: number; left: number; width: number }
interface LineLayout {
  row: LogRow;
  text: string;
  segments: LayoutSegment[];
  offset: number;
  width: number;
  iterator: IterableIterator<Intl.SegmentData>;
}
interface LogPart { text: string; color?: string; tabWidth?: number }

export function createLogLayout(context: CanvasRenderingContext2D) {
  const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
  const cache = new Map<number, LineLayout>();
  let font = "";
  let maximumWidth = 0;
  let tabWidth = 1;
  let nextLine = 0;

  function textFor(raw: string) {
    let end = raw.length;
    while (end && (raw[end - 1] === "\r" || raw[end - 1] === "\n")) end--;
    return raw.slice(0, end);
  }

  function resetIterator(line: LineLayout, offset: number) {
    line.offset = offset;
    line.iterator = segmenter.segment(line.text.slice(offset))[Symbol.iterator]();
  }

  function lineFor(row: LogRow) {
    let line = cache.get(row.id);
    if (line?.row === row) return line;
    const text = textFor(row.raw);
    if (line && !row.trimmed && text.startsWith(line.text)) {
      const tail = line.segments.pop();
      line.text = text;
      line.row = row;
      const offset = tail?.start ?? 0;
      line.width = tail?.left ?? 0;
      resetIterator(line, offset);
    } else {
      line = { row, text, segments: [], offset: 0, width: 0, iterator: segmenter.segment(text)[Symbol.iterator]() };
      cache.set(row.id, line);
    }
    return line;
  }

  function measure(text: string, left: number, tabs?: Map<number, number>) {
    let x = left;
    let at = 0;
    for (let tab = text.indexOf("\t"); tab !== -1; tab = text.indexOf("\t", at)) {
      x += context.measureText(text.slice(at, tab)).width;
      const gap = (Math.floor((x + 1e-7) / tabWidth) + 1) * tabWidth - x;
      tabs?.set(tab, gap);
      x += gap;
      at = tab + 1;
    }
    return x + context.measureText(text.slice(at)).width - left;
  }

  function configure(style: CSSStyleDeclaration, force = false) {
    const nextFont = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    if (!force && font === nextFont) return false;
    font = nextFont;
    context.font = nextFont;
    context.fontKerning = "none";
    tabWidth = context.measureText(" ").width * 8;
    cache.clear();
    maximumWidth = 0;
    return true;
  }

  function retain(rows: readonly LogRow[]) {
    const first = rows[0]?.id;
    const last = rows[rows.length - 1];
    for (const id of cache.keys()) {
      if (first === undefined || last === undefined || id < first || id > last.id) cache.delete(id);
    }
    if (last && cache.has(last.id)) lineFor(last);
    maximumWidth = Math.max(0, ...Array.from(cache.values(), (line) => line.width));
  }

  function advance(rows: readonly LogRow[], deadline: number) {
    const lines = rows.map(lineFor);
    if (!lines.length) return;
    let skipped = 0;
    do {
      const line = lines[nextLine % lines.length]!;
      nextLine = (nextLine + 1) % lines.length;
      if (line.offset >= line.text.length) {
        if (++skipped === lines.length) return;
      } else {
        skipped = 0;
        const start = line.offset;
        let end = start;
        // 字素分段器针对剩余文本，偏移随本段推进后重新绑定。
        for (let count = 0; count < 512; count++) {
          const next = line.iterator.next();
          if (next.done) break;
          end = start + next.value.index + next.value.segment.length;
        }
        const segmentWidth = measure(line.text.slice(start, end), line.width);
        line.segments.push({ start, end, left: line.width, width: segmentWidth });
        line.width += segmentWidth;
        resetIterator(line, end);
        maximumWidth = Math.max(maximumWidth, line.width);
      }
    } while (performance.now() < deadline);
  }

  function visibleSegments(line: LineLayout, left: number, viewportWidth: number) {
    const segments = line.segments;
    let low = 0;
    let high = segments.length;
    while (low < high) {
      const middle = (low + high) >>> 1;
      const segment = segments[middle]!;
      if (segment.left + segment.width < left) low = middle + 1;
      else high = middle;
    }
    const start = Math.max(0, low - 1);
    let end = low;
    while (end < segments.length && segments[end]!.left <= left + viewportWidth) end++;
    return segments.slice(start, Math.min(segments.length, end + 1));
  }

  function fragments(text: string, start: number, end: number, rules: readonly LogHighlightRule[]) {
    const source = text.slice(start, end);
    const glyphs = Array.from(segmenter.segment(source), (glyph) => ({
      start: start + glyph.index,
      end: start + glyph.index + glyph.segment.length,
      color: undefined as string | undefined,
    }));
    let uncolored = glyphs.length;
    for (const rule of rules) {
      if (!uncolored) break;
      let from = Math.max(0, start - rule.keyword.length + 1);
      let to = Math.min(text.length, end + rule.keyword.length - 1);
      if (from > 0 && /[\uDC00-\uDFFF]/.test(text[from]!)) from--;
      if (to < text.length && /[\uDC00-\uDFFF]/.test(text[to]!)) to++;
      const escaped = rule.keyword.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      for (const match of text.slice(from, to).matchAll(new RegExp(`(?=(${escaped}))`, "giu"))) {
        const a = from + match.index;
        const b = a + match[1]!.length;
        if (a >= end) break;
        let low = 0;
        let high = glyphs.length;
        while (low < high) {
          const middle = (low + high) >>> 1;
          if (glyphs[middle]!.end <= a) low = middle + 1;
          else high = middle;
        }
        for (let index = low; index < glyphs.length && glyphs[index]!.start < b; index++) {
          const glyph = glyphs[index]!;
          if (glyph.color === undefined) { glyph.color = palette[rule.color]; uncolored--; }
        }
        if (!uncolored) break;
      }
    }
    const result: LogPart[] = [];
    for (const glyph of glyphs) {
      const value = text.slice(glyph.start, glyph.end);
      const previous = result[result.length - 1];
      if (previous && previous.color === glyph.color) previous.text += value;
      else result.push({ text: value, color: glyph.color });
    }
    return result;
  }

  function render(row: LogRow, left: number, viewportWidth: number, rules: readonly LogHighlightRule[]) {
    const line = cache.get(row.id);
    if (!line) return [];
    return visibleSegments(line, left, viewportWidth).map((segment) => {
      const tabs = new Map<number, number>();
      measure(line.text.slice(segment.start, segment.end), segment.left, tabs);
      let offset = 0;
      const parts: LogPart[] = [];
      for (const fragment of fragments(line.text, segment.start, segment.end, rules)) {
        let at = 0;
        for (let tab = fragment.text.indexOf("\t"); tab !== -1; tab = fragment.text.indexOf("\t", at)) {
          if (tab > at) parts.push({ text: fragment.text.slice(at, tab), color: fragment.color });
          parts.push({ text: "\t", color: fragment.color, tabWidth: tabs.get(offset + tab) });
          at = tab + 1;
        }
        if (at < fragment.text.length) parts.push({ text: fragment.text.slice(at), color: fragment.color });
        offset += fragment.text.length;
      }
      return { ...segment, parts };
    });
  }

  function progress(rows: readonly LogRow[]) {
    let done = 0;
    let total = 0;
    for (const row of rows) {
      const line = cache.get(row.id);
      total += line?.text.length ?? row.raw.length;
      done += line?.offset ?? 0;
    }
    return total ? Math.min(1, done / total) : 1;
  }

  return { configure, retain, advance, render, progress, width: () => maximumWidth, clear: () => { cache.clear(); maximumWidth = 0; } };
}

