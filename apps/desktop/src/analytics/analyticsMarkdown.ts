import { renderKohborChart, type KohborChartResolvedReference } from "./analyticsChart.ts";

export type MarkdownAction =
  | { kind: "time"; seconds: number }
  | { kind: "event"; table: string; index: number }
  | { kind: "document"; id: string }
  | { kind: "copy_url"; url: string }
  | { kind: "copy_code" };

export type MarkdownRenderOptions = { resolveChartReference?: KohborChartResolvedReference };

const escapeHtml = (value: string) => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;")
  .replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&#39;");
const DOC_ID = /^(?!.*\.\.)[a-zA-Z0-9][a-zA-Z0-9_.-]{0,63}$/;
const SAFE_EVENT = /^#ev=([a-z][a-z0-9_-]{0,31})\/(\d{1,7})$/;
const SAFE_TIME = /^#t=(\d{1,7})$/;

function safeExternalUrl(value: string): string | null {
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.username || url.password || url.href.length > 2048) return null;
    return url.href;
  } catch { return null; }
}

function inline(source: string): string {
  const tokens: string[] = [];
  const stash = (html: string) => `\u0000${tokens.push(html) - 1}\u0000`;
  let text = source.replace(/`([^`\n]+)`/g, (_all, code: string) => stash(`<code>${escapeHtml(code)}</code>`));
  text = text.replace(/\[([^\]\n]+)\]\(([^\s()]*(?:\([^()\s]*\)[^\s()]*)*)\)/g, (_all, rawLabel: string, target: string) => {
    const label = escapeHtml(rawLabel.replaceAll("\\]", "]"));
    const time = SAFE_TIME.exec(target);
    if (time) return stash(`<button type="button" class="an-md-seek" data-seek-time="${Number(time[1])}">${label}</button>`);
    const event = SAFE_EVENT.exec(target);
    if (event) return stash(`<button type="button" class="an-md-seek" data-seek-event="${event[1]}/${Number(event[2])}">${label}</button>`);
    const doc = /^#doc=(.+)$/.exec(target);
    if (doc && DOC_ID.test(doc[1]!)) return stash(`<button type="button" class="an-md-document" data-doc-id="${doc[1]}">${label}</button>`);
    const url = safeExternalUrl(target);
    if (url) return stash(`<button type="button" class="an-md-copy-url" data-md-copy-url="${escapeHtml(url)}">${label}</button>`);
    return stash(label);
  });
  text = escapeHtml(text);
  text = text.replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
    .replace(/__([^_\n]+)__/g, "<strong>$1</strong>")
    .replace(/~~([^~\n]+)~~/g, "<del>$1</del>")
    .replace(/\*([^*\n]+)\*/g, "<em>$1</em>")
    .replace(/_([^_\n]+)_/g, "<em>$1</em>");
  return text.replace(/\u0000(\d+)\u0000/g, (_all, index: string) => tokens[Number(index)] ?? "");
}

const isRule = (line: string) => /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(line);
const cells = (line: string) => line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map(cell => cell.trim());
const isFence = (line: string) => /^\s*```/.test(line);

function codeBlock(language: string, body: string): string {
  const lang = language.replace(/[^a-zA-Z0-9_+-]/g, "").slice(0, 32);
  const code = `<div class="an-md-code-block" data-code-language="${escapeHtml(lang)}"><button type="button" data-md-copy-code aria-label="Copy code">${lang || "Copy code"}</button><pre><code>${escapeHtml(body)}</code></pre></div>`;
  const lines = body.split("\n").length;
  return lines > 20 ? `<details class="an-md-code-collapsible"><summary>${lang || "Code"} · ${lines} lines</summary>${code}</details>` : code;
}

function chartBlock(source: string, resolve?: KohborChartResolvedReference): string {
  const result = renderKohborChart(source, resolve);
  if (result.html) return result.html;
  return `<div class="an-md-chart-error" role="status"><p>Chart unavailable: ${escapeHtml(result.error)}</p>${codeBlock("json", source)}</div>`;
}

function renderBlocks(lines: string[], resolve?: KohborChartResolvedReference): string {
  const out: string[] = [];
  let i = 0;
  const parseList = (start: number, baseIndent: number): { html: string; next: number } => {
    const first = lines[start]!;
    const ordered = /^\s*\d+[.)]\s+/.test(first);
    const listTag = ordered ? "ol" : "ul";
    const pattern = ordered ? /^(\s*)\d+[.)]\s+(.*)$/ : /^(\s*)[-*+]\s+(.*)$/;
    const items: string[] = [];
    let index = start;
    while (index < lines.length) {
      const match = pattern.exec(lines[index]!);
      if (!match) break;
      const indent = match[1]!.length;
      if (indent < baseIndent) break;
      if (indent > baseIndent) {
        if (items.length) {
          const nested = parseList(index, indent);
          items[items.length - 1] = items[items.length - 1]!.replace(/<\/li>$/, `${nested.html}</li>`);
          index = nested.next;
          continue;
        }
        break;
      }
      const value = match[2]!;
      const task = /^\[([ xX])\]\s+(.*)$/.exec(value);
      const content = task ? `<input type="checkbox" disabled${task[1]!.toLowerCase() === "x" ? " checked" : ""} aria-label="Task"> ${inline(task[2]!)}` : inline(value);
      items.push(`<li>${content}</li>`);
      index++;
    }
    return { html: `<${listTag}>${items.join("")}</${listTag}>`, next: index };
  };

  while (i < lines.length) {
    const line = lines[i]!;
    if (!line.trim()) { i++; continue; }
    if (isFence(line)) {
      const opening = /^\s*```\s*([a-zA-Z0-9_+-]{0,32})\s*$/.exec(line);
      const language = opening?.[1] ?? "";
      const body: string[] = [];
      i++;
      while (i < lines.length && !isFence(lines[i]!)) body.push(lines[i++]!);
      if (i < lines.length) i++;
      const content = body.join("\n");
      out.push(language === "kohbor-chart" ? chartBlock(content, resolve) : codeBlock(language, content));
      continue;
    }
    if (/^\s*(?:---+|___+|\*\*\*+)\s*$/.test(line)) { out.push("<hr />"); i++; continue; }
    const heading = /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line);
    if (heading) { const level = heading[1]!.length; out.push(`<h${level}>${inline(heading[2]!)}</h${level}>`); i++; continue; }
    if (i + 1 < lines.length && line.includes("|") && isRule(lines[i + 1]!)) {
      const headers = cells(line), alignments = cells(lines[i + 1]!).map(cell => cell.startsWith(":") ? (cell.endsWith(":") ? "center" : "left") : cell.endsWith(":") ? "right" : "left");
      const rows: string[][] = [];
      i += 2;
      while (i < lines.length && lines[i]!.includes("|") && lines[i]!.trim()) rows.push(cells(lines[i++]!));
      const th = headers.map((cell, j) => `<th style="text-align:${alignments[j] ?? "left"}">${inline(cell)}</th>`).join("");
      const body = rows.map(row => `<tr>${headers.map((_head, j) => `<td style="text-align:${alignments[j] ?? "left"}">${inline(row[j] ?? "")}</td>`).join("")}</tr>`).join("");
      out.push(`<table><thead><tr>${th}</tr></thead><tbody>${body}</tbody></table>`);
      continue;
    }
    if (/^\s*>/.test(line)) {
      const quote: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i]!)) quote.push(lines[i++]!.replace(/^\s*>\s?/, ""));
      out.push(`<blockquote>${quote.map(value => `<p>${inline(value)}</p>`).join("")}</blockquote>`);
      continue;
    }
    const list = /^(\s*)([-*+]\s+|\d+[.)]\s+)/.exec(line);
    if (list) {
      const parsed = parseList(i, list[1]!.length);
      out.push(parsed.html); i = Math.max(i + 1, parsed.next); continue;
    }
    const paragraph = [line]; i++;
    while (i < lines.length && lines[i]!.trim() && !/^(#{1,6}\s|\s*```|\s*>|\s*[-*+]\s|\s*\d+[.)]\s|\s*(?:---+|___+|\*\*\*+)\s*$)/.test(lines[i]!)) paragraph.push(lines[i++]!);
    out.push(`<p>${paragraph.map(value => inline(value.replace(/\\$/, ""))).join("<br>")}</p>`);
  }
  return out.join("");
}

/** Converts the supported Markdown subset to escaped, inert HTML. Source HTML is always text. */
export function renderMarkdownSafe(markdown: string, options: MarkdownRenderOptions = {}): string {
  return renderBlocks(markdown.replace(/\r\n?/g, "\n").split("\n"), options.resolveChartReference);
}

/** Renders safe completed blocks while keeping an unfinished chart fence visibly pending. */
export function renderMarkdownStreaming(markdown: string, options: MarkdownRenderOptions = {}): string {
  const normalized = markdown.replace(/\r\n?/g, "\n");
  const lines = normalized.split("\n");
  let openChart = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    if (isFence(lines[i]!)) { openChart = /^\s*```\s*kohbor-chart\s*$/.test(lines[i]!) ? i : -1; break; }
  }
  if (openChart < 0) return renderMarkdownSafe(normalized, options);
  const prefix = renderBlocks(lines.slice(0, openChart), options.resolveChartReference);
  return `${prefix}<div class="an-md-chart-pending" role="status">Chart is loading</div>`;
}

/** Buffers deltas and exposes an inert partial render; finish is identical to whole-text rendering. */
export function createMarkdownStream(options: MarkdownRenderOptions = {}) {
  let source = "";
  return {
    push(chunk: string): string { source += chunk; return renderMarkdownStreaming(source, options); },
    finish(): string { return renderMarkdownSafe(source, options); },
    get text(): string { return source; },
  };
}

export function actionFromMarkdownTarget(element: Element): MarkdownAction | null {
  const time = element.getAttribute("data-seek-time");
  if (time && /^\d{1,7}$/.test(time)) return { kind: "time", seconds: Number(time) };
  const event = element.getAttribute("data-seek-event");
  const match = event && /^([a-z][a-z0-9_-]{0,31})\/(\d{1,7})$/.exec(event);
  if (match) return { kind: "event", table: match[1]!, index: Number(match[2]) };
  const document = element.hasAttribute?.("data-doc-id") ? element.getAttribute("data-doc-id") : null;
  if (document && DOC_ID.test(document)) return { kind: "document", id: document };
  const copyUrl = element.hasAttribute?.("data-md-copy-url") ? element.getAttribute("data-md-copy-url") : null;
  if (copyUrl && safeExternalUrl(copyUrl) === copyUrl) return { kind: "copy_url", url: copyUrl };
  if (element.hasAttribute("data-md-copy-code")) return { kind: "copy_code" };
  return null;
}
