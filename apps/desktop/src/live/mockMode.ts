export function liveMockRequested(): boolean {
  if (!import.meta.env.DEV || typeof window === "undefined") return false;
  const params = new URLSearchParams(window.location.search);
  return params.get("mock") === "1" && params.get("live") === "1";
}
