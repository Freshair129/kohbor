import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const { footerMetric, footerStateLabel } = await import("../src/shell/statusFooterModel.ts");

test("footer keeps reported metrics and gives unknown metrics a dash and empty bar", () => {
  const cpu = footerMetric({ id: "cpu", label: "CPU", value: 12.5, display: "12.5", unit: "%", tone: "unknown", title: "host CPU", barPercent: 12.5 });
  const ram = footerMetric({ id: "ram", label: "RAM", value: null, display: "—", unit: "", tone: "unknown", title: "host memory", barPercent: 80 });
  assert.equal(cpu.display, "12.5");
  assert.equal(cpu.barPercent, 12.5);
  assert.equal(ram.display, "—");
  assert.equal(ram.barPercent, 0);
});

test("footer state is an existing capture state expressed as one compact label", () => {
  assert.equal(footerStateLabel("live", "th"), "สด");
  assert.equal(footerStateLabel("idle", "th"), "ว่าง");
  assert.equal(footerStateLabel(null, "en"), "—");
});

test("narrow footer drops lower-priority metrics first and does not define GPU metrics", () => {
  const css = readFileSync(new URL("../src/shell/statusFooter.css", import.meta.url), "utf8");
  const component = readFileSync(new URL("../src/shell/StatusFooter.tsx", import.meta.url), "utf8");
  const ids = ["cpu", "ram", "capture", "received", "painted"];
  for (const id of ids) assert.match(component, new RegExp(`id: \\\"${id}\\\"`));
  assert.ok(css.indexOf('data-priority="painted"') < css.indexOf('data-priority="received"'));
  assert.match(component, /aria-label=\{labels\.region\}/);
  assert.match(component, /metric\.display/);
  assert.doesNotMatch(component, /gpu|vram/i);
  assert.doesNotMatch(css, /--brand\s*;/);
});
