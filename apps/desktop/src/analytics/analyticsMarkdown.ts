export type MarkdownAction = { kind: "time"; seconds: number } | { kind: "event"; table: string; index: number };

const escapeHtml = (value: string) => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;")
  .replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&#39;");

function inline(source: string): string {
  const tokens: string[] = [];
  const stash = (html: string) => `\u0000${tokens.push(html) - 1}\u0000`;
  let text = source.replace(/`([^`\n]+)`/g, (_all, code: string) => stash(`<code>${escapeHtml(code)}</code>`));
  text = text.replace(/\[([^\]]+)\]\(((?:[^\s()]|\([^()\s]*\))*)\)/g, (_all, label: string, href: string) => {
    const safeLabel = inline(label);
    const time = /^#t=(\d{1,7})$/.exec(href);
    if (time) return stash(`<button type="button" class="an-md-seek" data-seek-time="${Number(time[1])}">${safeLabel}</button>`);
    const event = /^#ev=([a-z][a-z0-9_-]{0,31})\/(\d{1,7})$/.exec(href);
    if (event) return stash(`<button type="button" class="an-md-seek" data-seek-event="${event[1]}/${Number(event[2])}">${safeLabel}</button>`);
    return stash(safeLabel);
  });
  text = escapeHtml(text);
  text = text.replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*\n]+)\*/g, "<em>$1</em>");
  return text.replace(/\u0000(\d+)\u0000/g, (_all, index: string) => tokens[Number(index)] ?? "");
}

const isRule = (line: string) => /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(line);
const cells = (line: string) => line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map(cell => cell.trim());

/** Converts the supported Markdown subset to escaped, inert HTML. No source HTML is copied through. */
export function renderMarkdownSafe(markdown: string): string {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const out: string[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    if (!line.trim()) { i++; continue; }
    if (/^\s*```/.test(line)) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i]!)) body.push(lines[i++]!);
      if (i < lines.length) i++;
      out.push(`<pre><code>${escapeHtml(body.join("\n"))}</code></pre>`);
      continue;
    }
    const heading = /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line);
    if (heading) { const level = heading[1]!.length; out.push(`<h${level}>${inline(heading[2]!)}</h${level}>`); i++; continue; }
    if (i + 1 < lines.length && line.includes("|") && isRule(lines[i + 1]!)) {
      const headers = cells(line), rows: string[][] = [];
      i += 2;
      while (i < lines.length && lines[i]!.includes("|") && lines[i]!.trim()) rows.push(cells(lines[i++]!));
      out.push(`<table><thead><tr>${headers.map(cell => `<th>${inline(cell)}</th>`).join("")}</tr></thead><tbody>${rows.map(row => `<tr>${headers.map((_head, j) => `<td>${inline(row[j] ?? "")}</td>`).join("")}</tr>`).join("")}</tbody></table>`);
      continue;
    }
    if (/^\s*>/.test(line)) {
      const quote: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i]!)) quote.push(lines[i++]!.replace(/^\s*>\s?/, ""));
      out.push(`<blockquote>${quote.map(value => `<p>${inline(value)}</p>`).join("")}</blockquote>`);
      continue;
    }
    if (/^\s*[-*+]\s+/.test(line) || /^\s*\d+[.)]\s+/.test(line)) {
      const ordered = /^\s*\d/.test(line), tag = ordered ? "ol" : "ul", items: string[] = [];
      const pattern = ordered ? /^\s*\d+[.)]\s+/ : /^\s*[-*+]\s+/;
      while (i < lines.length && pattern.test(lines[i]!)) items.push(`<li>${inline(lines[i++]!.replace(pattern, ""))}</li>`);
      out.push(`<${tag}>${items.join("")}</${tag}>`);
      continue;
    }
    const paragraph = [line]; i++;
    while (i < lines.length && lines[i]!.trim() && !/^(#{1,6}\s|\s*```|\s*>|\s*[-*+]\s|\s*\d+[.)]\s)/.test(lines[i]!)) paragraph.push(lines[i++]!);
    out.push(`<p>${paragraph.map(inline).join("<br>")}</p>`);
  }
  return out.join("");
}

export function actionFromMarkdownTarget(element: Element): MarkdownAction | null {
  const time = element.getAttribute("data-seek-time");
  if (time && /^\d{1,7}$/.test(time)) return { kind: "time", seconds: Number(time) };
  const event = element.getAttribute("data-seek-event");
  const match = event && /^([a-z][a-z0-9_-]{0,31})\/(\d{1,7})$/.exec(event);
  return match ? { kind: "event", table: match[1]!, index: Number(match[2]) } : null;
}
