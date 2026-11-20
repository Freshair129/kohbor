export function RailRibbon({ side }: { side: "left" | "right" }) {
  return <span className={`pulse-grid-ribbon pulse-grid-ribbon--${side}`} aria-hidden="true" />;
}
