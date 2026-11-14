export interface AccountMenuState { open: boolean; index: number }
export function reduceAccountMenu(state: AccountMenuState, key: string, count: number): { state: AccountMenuState; returnFocus: boolean } {
  const close = (returnFocus: boolean) => ({ state: { ...state, open: false }, returnFocus });
  if (key === "Escape" || key === "select") return close(true);
  if (key === "outside" || key === "Tab") return close(false);
  if (key === "toggle") return { state: { open: !state.open, index: 0 }, returnFocus: state.open };
  if (count === 0) return { state, returnFocus: false };
  const last = count - 1;
  const index = key === "Home" ? 0 : key === "End" ? last
    : key === "ArrowDown" ? state.open ? (state.index + 1) % count : 0
    : key === "ArrowUp" ? state.open ? (state.index - 1 + count) % count : last : null;
  return { state: index === null ? state : { open: true, index }, returnFocus: false };
}
