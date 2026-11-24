import assert from "node:assert/strict";
import { test } from "node:test";
import { actionFromMarkdownTarget, renderMarkdownSafe } from "../src/analytics/analyticsMarkdown.ts";

test("unsafe HTML, image handlers and javascript links stay inert", () => {
  const html = renderMarkdownSafe('<script>alert(1)</script> <img src=x onerror="alert(1)"> [bad](javascript:alert(1)) [web](https://example.com)');
  assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
  assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
  assert.doesNotMatch(html, /<script|<img|javascript:|https:\/\/example/);
  assert.doesNotMatch(html, /data-seek/);
  assert.ok(html.includes("bad web"));
  assert.doesNotMatch(html, /<a\b/);
});

test("only approved time and event references become seek buttons", () => {
  const html = renderMarkdownSafe("[07:45](#t=465) [deaths#13](#ev=deaths/13) [other](#settings) [bad](#ev=../13)");
  assert.match(html, /data-seek-time="465"/);
  assert.match(html, /data-seek-event="deaths\/13"/);
  assert.equal((html.match(/<button/g) ?? []).length, 2);
  assert.deepEqual(actionFromMarkdownTarget({ getAttribute: name => name === "data-seek-time" ? "465" : null } as Element), { kind: "time", seconds: 465 });
  assert.equal(actionFromMarkdownTarget({ getAttribute: () => "javascript" } as unknown as Element), null);
});

test("supported Markdown blocks are rendered as allowed structure", () => {
  const html = renderMarkdownSafe("# Title\n\n**bold** and *italic* with `code`\n\n- one\n- two\n\n> quote\n\n| A | B |\n| --- | --- |\n| x | y |");
  for (const tag of ["<h1>", "<strong>", "<em>", "<code>", "<ul>", "<li>", "<blockquote>", "<table>", "<th>", "<td>"]) assert.ok(html.includes(tag), tag);
});
