export type KohborChartKind = "bar" | "line" | "area" | "timeline";
export type KohborChartReference = "hero_series" | "advantage_per_min" | "lane_series" | "deaths" | "objectives";
export type KohborChartPoint = { x: number; y: number | null };
export type KohborChartSeries = { name: string; values?: (number | null)[]; points?: KohborChartPoint[] };
export type KohborChartReferenceRequest = {
  ref: KohborChartReference;
  hero?: string;
  metric?: string;
  lane?: string;
  from?: number;
  to?: number;
};
export type KohborChartInput = {
  type: KohborChartKind;
  title?: string;
  unit?: string;
  categories?: string[];
  series?: (KohborChartSeries | KohborChartReferenceRequest)[];
  events?: { t: number; label: string }[];
  from?: number;
  to?: number;
  marks?: { t: number; label: string }[];
};
export type KohborChartResolvedReference = (reference: KohborChartReferenceRequest) => KohborChartSeries | null;

const MAX_CHART_BYTES = 8 * 1024;
const MAX_SERIES = 6;
const MAX_POINTS = 120;
const MAX_NUMBER = 1_000_000_000_000;
const WIDTH = 640;
const HEIGHT = 320;
const LEFT = 58;
const RIGHT = 624;
const TOP = 34;
const BOTTOM = 252;

const escape = (value: unknown) => String(value ?? "").replaceAll("&", "&amp;").replaceAll("<", "&lt;")
  .replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&#39;");
const label = (value: unknown, fallback = "") => typeof value === "string" ? value.slice(0, 80) : fallback;
const numberOrNull = (value: unknown): value is number | null => value === null || (typeof value === "number" && Number.isFinite(value) && Math.abs(value) <= MAX_NUMBER);
const colors = ["#53d2a3", "#7da7ff", "#f0b759", "#cf8cff", "#ef7981", "#5ac7d9"] as const;
const ALLOWED_REFS = new Set<KohborChartReference>(["hero_series", "advantage_per_min", "lane_series", "deaths", "objectives"]);

export type KohborChartResult = { html: string; error?: never } | { html?: never; error: string };

function validRange(from: unknown, to: unknown): boolean {
  return typeof from === "number" && Number.isFinite(from) && Math.abs(from) <= MAX_NUMBER && typeof to === "number" && Number.isFinite(to) && Math.abs(to) <= MAX_NUMBER && to > from;
}

function plotFrame(title: string, unit: string, body: string, legend: string, description: string): string {
  return `<figure class="an-md-chart"><svg viewBox="0 0 ${WIDTH} ${HEIGHT}" role="img" aria-label="${escape(description)}" xmlns="http://www.w3.org/2000/svg"><title>${escape(title || "Chart")}</title><desc>${escape(description)}</desc><path class="an-chart-axis" d="M${LEFT} ${TOP}V${BOTTOM}H${RIGHT}"/><text class="an-chart-unit" x="${LEFT}" y="20">${escape(unit)}</text>${body}${legend}</svg></figure>`;
}

function legendFor(series: readonly KohborChartSeries[]): string {
  const start = Math.max(LEFT, RIGHT - Math.min(series.length, MAX_SERIES) * 100);
  return `<g class="an-chart-legend">${series.map((item, index) => `<g transform="translate(${start + index * 100} 292)"><circle cx="0" cy="0" r="4" fill="${colors[index % colors.length]}"/><text x="8" y="4">${escape(label(item.name, `Series ${index + 1}`))}</text></g>`).join("")}</g>`;
}

function dataSeries(chart: KohborChartInput, resolve?: KohborChartResolvedReference): KohborChartSeries[] {
  if (!Array.isArray(chart.series) || chart.series.length < 1 || chart.series.length > MAX_SERIES) throw new Error(`Use 1 to ${MAX_SERIES} series.`);
  return chart.series.map((entry, index) => {
    if (!entry || typeof entry !== "object") throw new Error("A series must be an object.");
    let value: KohborChartSeries;
    if ("ref" in entry) {
      if (!ALLOWED_REFS.has(entry.ref)) throw new Error("This reference is not supported.");
      value = resolve?.(entry) ?? (() => { throw new Error("The selected match does not have this chart data."); })();
    } else value = entry;
    if (!value || typeof value.name !== "string") throw new Error("Every series needs a name.");
    if (value.points !== undefined) {
      if (!Array.isArray(value.points) || value.points.length > MAX_POINTS) throw new Error(`A series may have at most ${MAX_POINTS} points.`);
      for (const point of value.points) if (!point || !numberOrNull(point.x) || point.x === null || !numberOrNull(point.y)) throw new Error("Chart points must contain bounded finite x values and finite y values or null.");
    } else {
      if (!Array.isArray(value.values) || value.values.length > MAX_POINTS) throw new Error(`A series may have at most ${MAX_POINTS} points.`);
      if (!value.values.every(numberOrNull)) throw new Error("Chart values must be finite numbers or null.");
    }
    return { ...value, name: label(value.name, `Series ${index + 1}`) };
  });
}

function seriesPoints(series: KohborChartSeries[]): { x: number; y: number | null }[][] {
  return series.map(item => item.points ?? (item.values ?? []).map((y, index) => ({ x: index, y })));
}

function scaleY(values: number[], includeZero: boolean): (value: number) => number {
  let min = Math.min(...values), max = Math.max(...values);
  if (includeZero) { min = Math.min(0, min); max = Math.max(0, max); }
  if (min === max) { const pad = Math.max(1, Math.abs(min) * 0.1); min -= pad; max += pad; }
  const span = max - min;
  return value => BOTTOM - ((value - min) / span) * (BOTTOM - TOP);
}

function renderBar(chart: KohborChartInput, series: KohborChartSeries[]): string {
  if (!Array.isArray(chart.categories) || chart.categories.length < 1 || chart.categories.length > MAX_POINTS) throw new Error(`Bar charts need 1 to ${MAX_POINTS} categories.`);
  if (series.some(item => item.points || item.values?.length !== chart.categories!.length)) throw new Error("Every bar series must match the category count.");
  const values = series.flatMap(item => item.values ?? []).filter((value): value is number => value !== null);
  if (!values.length) throw new Error("The chart has no known numeric values.");
  const y = scaleY(values, true);
  const slot = (RIGHT - LEFT) / chart.categories.length;
  const groupWidth = Math.min(slot * 0.72, 52);
  const barWidth = Math.max(1, groupWidth / series.length - 3);
  const zero = y(0);
  const bars = chart.categories.map((category, categoryIndex) => {
    const x0 = LEFT + categoryIndex * slot + (slot - groupWidth) / 2;
    const group = series.map((item, seriesIndex) => {
      const value = item.values![categoryIndex]!;
      if (value === null) return "";
      const yv = y(value), height = Math.max(1, Math.abs(zero - yv));
      return `<rect x="${x0 + seriesIndex * (groupWidth / series.length) + 1}" y="${Math.min(zero, yv)}" width="${barWidth}" height="${height}" fill="${colors[seriesIndex % colors.length]}"><title>${escape(item.name)}: ${value}</title></rect>`;
    }).join("");
    const short = label(category, "");
    return `${group}<text class="an-chart-label" x="${x0 + groupWidth / 2}" y="${BOTTOM + 20}" text-anchor="middle">${escape(short)}</text>`;
  }).join("");
  return plotFrame(label(chart.title, ""), label(chart.unit, ""), bars, legendFor(series), `${label(chart.title, "Bar chart")}. ${series.length} data series.`);
}

function renderLine(chart: KohborChartInput, series: KohborChartSeries[], area: boolean, resolve?: KohborChartResolvedReference): string {
  const points = seriesPoints(series);
  const all = points.flat().filter((point): point is { x: number; y: number } => point.y !== null);
  if (!all.length) throw new Error("The chart has no known numeric values.");
  const range = validRange(chart.from, chart.to) ? [chart.from!, chart.to!] as const : null;
  const xs = range ? [range[0], range[1]] : all.map(point => point.x);
  const minX = Math.min(...xs), maxX = Math.max(...xs);
  if (maxX === minX) throw new Error("The chart needs at least two distinct x positions.");
  const y = scaleY(all.map(point => point.y), false);
  const x = (value: number) => LEFT + (value - minX) / (maxX - minX) * (RIGHT - LEFT);
  const paths = points.map((list, seriesIndex) => {
    let segment: { x: number; y: number }[] = [];
    const segments: { x: number; y: number }[][] = [];
    for (const point of list) {
      const inRange = !range || (point.x >= range[0] && point.x <= range[1]);
      if (point.y === null || !inRange) { if (segment.length) segments.push(segment); segment = []; continue; }
      segment.push({ x: point.x, y: point.y });
    }
    if (segment.length) segments.push(segment);
    return segments.map(part => {
      if (part.length === 0) return "";
      const d = part.map((point, index) => `${index ? "L" : "M"}${x(point.x).toFixed(1)} ${y(point.y).toFixed(1)}`).join(" ");
      const fill = area && part.length > 1 ? `<path d="${d} L${x(part[part.length - 1]!.x).toFixed(1)} ${BOTTOM} L${x(part[0]!.x).toFixed(1)} ${BOTTOM} Z" fill="${colors[seriesIndex % colors.length]}" opacity="0.16"/>` : "";
      return `${fill}<path d="${d}" fill="none" stroke="${colors[seriesIndex % colors.length]}" stroke-width="2"/>${part.map(point => `<circle cx="${x(point.x).toFixed(1)}" cy="${y(point.y).toFixed(1)}" r="2.5" fill="${colors[seriesIndex % colors.length]}"><title>${escape(series[seriesIndex]!.name)}: ${point.y}</title></circle>`).join("")}`;
    }).join("");
  }).join("");
  const marks = (chart.marks ?? []).slice(0, MAX_POINTS).filter(mark => Number.isFinite(mark.t) && (!range || (mark.t >= range[0] && mark.t <= range[1])))
    .map(mark => `<g class="an-chart-mark an-md-seek" role="button" tabindex="0" data-seek-time="${Math.round(mark.t)}" aria-label="Seek to ${Math.round(mark.t)} seconds"><path d="M${x(mark.t)} ${TOP}V${BOTTOM}"/><text x="${x(mark.t)}" y="${TOP - 5}" text-anchor="middle">${escape(label(mark.label))}</text></g>`).join("");
  const ticks = `<text class="an-chart-label" x="${LEFT}" y="${BOTTOM + 20}">${escape(String(Math.round(minX)))}</text><text class="an-chart-label" x="${RIGHT}" y="${BOTTOM + 20}" text-anchor="end">${escape(String(Math.round(maxX)))}</text>`;
  void resolve;
  return plotFrame(label(chart.title, ""), label(chart.unit, ""), `${paths}${marks}${ticks}`, legendFor(series), `${label(chart.title, area ? "Area chart" : "Line chart")}. ${series.length} data series.`);
}

function renderTimeline(chart: KohborChartInput): string {
  if (!Array.isArray(chart.events) || chart.events.length > MAX_POINTS) throw new Error(`Timeline charts may have at most ${MAX_POINTS} events.`);
  if (!chart.events.length) throw new Error("The timeline has no events.");
  const times = chart.events.map(event => event?.t);
  if (!times.every(time => typeof time === "number" && Number.isFinite(time))) throw new Error("Timeline events need finite times.");
  const min = typeof chart.from === "number" && Number.isFinite(chart.from) && Math.abs(chart.from) <= MAX_NUMBER ? chart.from : Math.min(0, ...times);
  const max = typeof chart.to === "number" && Number.isFinite(chart.to) && Math.abs(chart.to) <= MAX_NUMBER ? chart.to : Math.max(1, ...times);
  if (max <= min) throw new Error("The timeline needs a range with increasing times.");
  const x = (time: number) => LEFT + (time - min) / (max - min) * (RIGHT - LEFT);
  const events = chart.events.map((event, index) => {
    if (event.t < min || event.t > max) return "";
    const cx = x(event.t), cy = index % 2 === 0 ? 128 : 166;
    const text = label(event.label, "Event");
    return `<g class="an-chart-event an-md-seek" role="button" tabindex="0" data-seek-time="${Math.round(event.t)}" aria-label="${escape(text)} at ${Math.round(event.t)} seconds"><path d="M${cx} ${cy + 6}V184"/><circle cx="${cx}" cy="${cy}" r="6"><title>${escape(text)} · ${Math.round(event.t)}s</title></circle><text x="${cx}" y="${cy - 12}" text-anchor="middle">${escape(text)}</text></g>`;
  }).join("");
  const ticks = `<text class="an-chart-label" x="${LEFT}" y="222">${escape(String(Math.round(min)))}s</text><text class="an-chart-label" x="${RIGHT}" y="222" text-anchor="end">${escape(String(Math.round(max)))}s</text>`;
  return plotFrame(label(chart.title, "Match timeline"), "time", `<path class="an-chart-timeline" d="M${LEFT} 184H${RIGHT}"/>${events}${ticks}`, "", `${label(chart.title, "Match timeline")}. ${chart.events.length} events.`);
}

/** Validate and render one bounded chart. All model strings are escaped and all geometry is derived from finite numbers. */
export function renderKohborChart(source: string, resolve?: KohborChartResolvedReference): KohborChartResult {
  if (new TextEncoder().encode(source).byteLength > MAX_CHART_BYTES) return { error: `Chart data exceeds ${MAX_CHART_BYTES} bytes.` };
  let chart: KohborChartInput;
  try { chart = JSON.parse(source) as KohborChartInput; }
  catch { return { error: "Chart data must be valid JSON." }; }
  if (!chart || typeof chart !== "object" || !["bar", "line", "area", "timeline"].includes(chart.type)) return { error: "Chart type must be bar, line, area or timeline." };
  try {
    if (chart.type === "timeline") return { html: renderTimeline(chart) };
    const series = dataSeries(chart, resolve);
    return { html: chart.type === "bar" ? renderBar(chart, series) : renderLine(chart, series, chart.type === "area", resolve) };
  } catch (reason) {
    return { error: reason instanceof Error ? reason.message : "Chart data is invalid." };
  }
}

export const KOHBOR_CHART_LIMITS = { maxBytes: MAX_CHART_BYTES, maxSeries: MAX_SERIES, maxPoints: MAX_POINTS, maxAbsoluteValue: MAX_NUMBER } as const;
