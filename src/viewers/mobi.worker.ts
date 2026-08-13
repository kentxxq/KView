interface WorkerRequest {
  id: number;
  type: "initialize" | "load-chapter" | "resolve-href" | "destroy";
  buffer?: ArrayBuffer;
  chapterId?: string;
  href?: string;
}

interface WorkerParser {
  getSpine(): Array<{ id: string; linear?: string; text?: string; start?: number; end?: number }>;
  getToc(): Array<{ label: string; href: string; children?: unknown[] }>;
  loadChapter(id: string): { html: string } | undefined | Promise<{ html: string } | undefined>;
  resolveHref(href: string): { id: string; selector: string } | undefined;
  destroy(): void;
}

let parser: WorkerParser | undefined;

function flattenToc(
  items: Array<{ label: string; href: string; children?: unknown[] }>,
  result: Array<{ label: string; href: string }> = [],
) {
  for (const item of items) {
    result.push(item);
    flattenToc(
      (item.children ?? []) as Array<{ label: string; href: string; children?: unknown[] }>,
      result,
    );
  }
  return result;
}

function filePosition(selector: string) {
  return Number(selector.match(/filepos:(\d+)/)?.[1] ?? Number.NaN);
}

function stringIndexAtUtf8Offset(value: string, targetBytes: number) {
  let bytes = 0;
  let index = 0;
  while (index < value.length && bytes < targetBytes) {
    const codePoint = value.codePointAt(index) ?? 0;
    const width = codePoint <= 0x7f ? 1 : codePoint <= 0x7ff ? 2 : codePoint <= 0xffff ? 3 : 4;
    if (bytes + width > targetBytes) break;
    bytes += width;
    index += codePoint > 0xffff ? 2 : 1;
  }
  return index;
}

function injectMobiSectionMarkers(currentParser: WorkerParser) {
  const spine = currentParser.getSpine();
  const itemsById = new Map(spine.map((item) => [item.id, item]));
  const sectionsById = new Map<string, Array<{ selector: string; sectionIndex: number }>>();
  const seen = new Set<string>();
  let sectionIndex = 0;
  for (const item of flattenToc(currentParser.getToc())) {
    const resolved = currentParser.resolveHref(item.href);
    if (!resolved) continue;
    const key = `${resolved.id}\0${resolved.selector}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const grouped = sectionsById.get(resolved.id) ?? [];
    grouped.push({ selector: resolved.selector, sectionIndex });
    sectionsById.set(resolved.id, grouped);
    sectionIndex += 1;
  }

  for (const [id, groupedSections] of sectionsById) {
    const spineItem = itemsById.get(id);
    if (!spineItem?.text) continue;
    const basePosition = filePosition(groupedSections[0].selector);
    if (!Number.isFinite(basePosition)) continue;
    const insertions = groupedSections
      .map(({ selector, sectionIndex: index }) => ({
        index: stringIndexAtUtf8Offset(
          spineItem.text ?? "",
          Math.max(0, filePosition(selector) - basePosition),
        ),
        marker: `<span data-kview-ebook-section="${index}"></span>`,
      }))
      .sort((left, right) => right.index - left.index);
    let markedText = spineItem.text;
    for (const insertion of insertions) {
      markedText = `${markedText.slice(0, insertion.index)}${insertion.marker}${markedText.slice(insertion.index)}`;
    }
    spineItem.text = markedText;
  }
}

function resolvedTocEntries(currentParser: WorkerParser) {
  return flattenToc(currentParser.getToc()).flatMap((item) => {
    const resolved = currentParser.resolveHref(item.href);
    return resolved ? [[item.href, resolved] as const] : [];
  });
}

function chapterLinks(currentParser: WorkerParser, html: string) {
  const links: Array<readonly [string, { id: string; selector: string }]> = [];
  const seen = new Set<string>();
  for (const match of html.matchAll(/\shref=["']([^"']+)["']/gi)) {
    const href = match[1];
    if (seen.has(href)) continue;
    seen.add(href);
    const resolved = currentParser.resolveHref(href);
    if (resolved) links.push([href, resolved]);
  }
  return links;
}

const workerScope = self as unknown as {
  onmessage: ((event: MessageEvent<WorkerRequest>) => void) | null;
  postMessage: (message: unknown) => void;
};

workerScope.onmessage = (event) => {
  const request = event.data;
  void (async () => {
    try {
      if (request.type === "initialize") {
        if (!request.buffer) throw new Error("缺少 MOBI 初始化数据");
        const bytes = new Uint8Array(request.buffer);
        const { initMobiFile } = await import("@lingo-reader/mobi-parser");
        parser = (await initMobiFile(bytes)) as unknown as WorkerParser;
        injectMobiSectionMarkers(parser);
        const spine = parser
          .getSpine()
          .map(({ id, linear, start, end }) => ({ id, linear, start, end }));
        workerScope.postMessage({
          id: request.id,
          result: { spine, toc: parser.getToc(), resolvedToc: resolvedTocEntries(parser) },
        });
        return;
      }
      if (!parser) throw new Error("电子书解析器尚未初始化");
      if (request.type === "load-chapter") {
        if (!request.chapterId) throw new Error("缺少章节编号");
        const chapter = await parser.loadChapter(request.chapterId);
        workerScope.postMessage({
          id: request.id,
          result: chapter
            ? { html: chapter.html, resolvedLinks: chapterLinks(parser, chapter.html) }
            : undefined,
        });
        return;
      }
      if (request.type === "resolve-href") {
        workerScope.postMessage({
          id: request.id,
          result: request.href ? parser.resolveHref(request.href) : undefined,
        });
        return;
      }
      parser.destroy();
      parser = undefined;
      workerScope.postMessage({ id: request.id, result: null });
    } catch (cause) {
      workerScope.postMessage({
        id: request.id,
        error: cause instanceof Error ? cause.message : String(cause),
      });
    }
  })();
};
