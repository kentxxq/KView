import DOMPurify from "dompurify";
import type { OpenedFile, PresentationDocument } from "../types/document";
import { desktopInvoke } from "./desktop-api";

const textStyles = new Set([
  "font-family", "font-size", "font-weight", "font-style", "color", "background-color",
  "text-align", "text-decoration", "text-decoration-line", "text-decoration-style",
  "vertical-align", "white-space",
]);

function sanitizeSlideHtml(html: string) {
  const fragment = DOMPurify.sanitize(html, {
    RETURN_DOM_FRAGMENT: true,
    ALLOWED_TAGS: [
      "p", "div", "section", "span", "h1", "h2", "h3", "h4", "h5", "h6",
      "strong", "b", "em", "i", "u", "s", "sub", "sup", "br", "hr",
      "ul", "ol", "li", "blockquote", "pre", "code", "table", "thead", "tbody",
      "tfoot", "tr", "td", "th", "caption", "img",
    ],
    ALLOWED_ATTR: ["src", "alt", "style", "colspan", "rowspan", "start"],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
  });
  for (const image of fragment.querySelectorAll("img")) {
    if (!/^data:image\/(?:png|jpeg|gif|webp|bmp|avif|x-icon|vnd\.microsoft\.icon|jxl|tiff);base64,[a-z\d+/=\s]+$/i.test(image.getAttribute("src") ?? "")) {
      image.replaceWith(document.createTextNode("[图片未加载：仅支持内嵌的原生栅格图片]"));
    }
  }
  for (const element of fragment.querySelectorAll<HTMLElement>("[style]")) {
    const style = element.style;
    for (const property of Array.from(style)) {
      if (!textStyles.has(property) || /url\s*\(|var\s*\(|expression\s*\(/i.test(style.getPropertyValue(property))) {
        style.removeProperty(property);
      }
    }
  }
  const container = document.createElement("div");
  container.append(fragment);
  return container.innerHTML;
}

export async function readPresentation(file: OpenedFile): Promise<PresentationDocument> {
  const document = await desktopInvoke<PresentationDocument>("read_presentation", {
    path: file.path,
    expectedModifiedAt: file.modifiedAt,
  });
  return {
    slides: document.slides.map((slide) => ({ ...slide, html: sanitizeSlideHtml(slide.html) })),
  };
}
