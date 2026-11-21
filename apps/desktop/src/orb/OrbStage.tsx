// The orb window (UI-009 / UI-011). Mouse only: nothing here can take keyboard focus, so Dota keeps Escape and
// every key. The host owns reveal, hit regions and audio values; this draws them and forwards clicks.
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type PointerEvent, type ReactNode, type WheelEvent } from "react";
import { mixerApi, orbApi, subscribeMixer, subscribeOrbState } from "./orbApi";
import {
  appLabel, chargeCue, channelNote, controllable, faderPercent, HIDDEN_ORB, orbCopy, reasonText, valueText, wheelStep,
  type ChannelId, type ChannelReading, type MixerReply, type MixerView, type OrbState,
} from "./orbModel";
import "./orb.css";

const LOCALE = "th" as const;
const text = orbCopy[LOCALE];
const REFRESH_MS = 1000;
const DRAG_SEND_MS = 60;

const noFocus = (event: { preventDefault: () => void }) => event.preventDefault();

function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(() => typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true);
  useEffect(() => {
    const query = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    if (!query) return;
    const change = () => setReduced(query.matches);
    query.addEventListener("change", change);
    return () => query.removeEventListener("change", change);
  }, []);
  return reduced;
}

function Mark() {
  return (
    <svg className="orb-mark" viewBox="0 0 34 34" aria-hidden="true">
      <path fill="#D4453A" d="M1 21.5V17A16 12 0 0 1 33 17V17.5L28 18.125L25 12.5L23.5 18.688Z" />
      <path fill="#EDE6D8" d="M2 23.375L15 21.75L15 25.25A4 4 0 0 1 11 29.75L9.5 29.938L7 25.25Z" />
      <path fill="#EDE6D8" d="M19 21.25L32 19.625L32 23.125A4 4 0 0 1 28 27.625L26.5 27.812L24 23.125Z" />
    </svg>
  );
}

function MuteIcon({ muted }: { muted: boolean }) {
  return muted ? (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="M2 6h2.5L8 3v10L4.5 10H2z" /><path d="M11 6l3.5 4M14.5 6L11 10" /></svg>
  ) : (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="M2 6h2.5L8 3v10L4.5 10H2z" /><path d="M10.5 5.8a3 3 0 010 4.4M12.3 4a5.5 5.5 0 010 8" /></svg>
  );
}

function ChannelIcon({ id }: { id: ChannelId }) {
  if (id === "kohbor") return <Mark />;
  if (id === "game") {
    return (
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
        <path d="M6 5.5h8a4.5 4.5 0 0 1 4.4 5.4l-.5 2.4a2 2 0 0 1-3.5.9L13 12.5H7l-1.4 1.7a2 2 0 0 1-3.5-.9l-.5-2.4A4.5 4.5 0 0 1 6 5.5z" />
        <path d="M6.5 8v2.6M5.2 9.3h2.6" /><circle cx="13.6" cy="9.3" r=".6" fill="currentColor" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <path d="M7.5 15V5.2l8.5-2v9.6" /><circle cx="5.3" cy="15" r="2.2" /><circle cx="13.8" cy="12.8" r="2.2" />
    </svg>
  );
}

interface FaderProps {
  id: ChannelId;
  name: string;
  sub: string;
  reading: ChannelReading | null;
  focused?: boolean;
  extra?: ReactNode;
  onLevel: (id: ChannelId, percent: number, final: boolean) => void;
  onMute: (id: ChannelId) => void;
}

function Fader({ id, name, sub, reading, focused, extra, onLevel, onMute }: FaderProps) {
  const track = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const enabled = controllable(reading);
  const shown = drag ?? reading?.percent ?? 0;
  const muted = enabled && reading?.muted === true && drag === null;
  const fromPointer = (event: PointerEvent<HTMLDivElement>) => {
    const box = track.current?.getBoundingClientRect();
    return box ? faderPercent(event.clientY, box.top, box.height) : shown;
  };
  const wheel = (event: WheelEvent<HTMLDivElement>) => {
    if (!enabled) return;
    onLevel(id, wheelStep(reading?.percent ?? 0, event.deltaY), true);
  };
  return (
    <div className={`orb-col${enabled ? "" : " is-disabled"}${muted ? " is-muted" : ""}${focused ? " is-focused" : ""}`}>
      <div className="orb-ico" data-channel={id} aria-hidden="true"><ChannelIcon id={id} /></div>
      <div className="orb-name">{name}</div>
      <div className="orb-sub">{sub}</div>
      <div className="orb-extra">{extra}</div>
      <div className="orb-fader" role="slider" aria-orientation="vertical" aria-label={name} aria-valuemin={0} aria-valuemax={100}
        aria-valuenow={enabled ? shown : undefined} aria-disabled={!enabled} tabIndex={-1}
        onMouseDown={noFocus}
        onPointerDown={(event) => {
          if (!enabled) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          const value = fromPointer(event);
          setDrag(value);
          onLevel(id, value, false);
        }}
        onPointerMove={(event) => {
          if (drag === null) return;
          const value = fromPointer(event);
          setDrag(value);
          onLevel(id, value, false);
        }}
        onPointerUp={(event) => {
          if (drag === null) return;
          const value = fromPointer(event);
          setDrag(null);
          onLevel(id, value, true);
        }}
        onPointerCancel={() => setDrag(null)}
        onWheel={wheel}>
        <div className="orb-track" ref={track}>
          {enabled && <div className="orb-fill" style={{ transform: `scaleY(${shown / 100})` }} />}
          {enabled && <div className="orb-thumb-wrap" style={{ transform: `translateY(${100 - shown}%)` }}><div className="orb-thumb" /></div>}
        </div>
      </div>
      <div className="orb-val">{drag !== null ? `${drag}%` : valueText(reading, LOCALE)}</div>
      <button type="button" className="orb-mute" tabIndex={-1} aria-pressed={muted} aria-label={text.muteLabel(name)}
        disabled={!enabled || (id === "kohbor" && reading?.muted === null)} onMouseDown={noFocus} onClick={() => onMute(id)}>
        <MuteIcon muted={muted} />
      </button>
    </div>
  );
}

function Panel({ state, view, error, onReply, onError }: {
  state: OrbState;
  view: MixerView | null;
  error: string | null;
  onReply: (reply: MixerReply) => void;
  onError: (code: string) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [picking, setPicking] = useState(false);
  const lastSend = useRef(0);
  const layout = state.layout;
  const run = useCallback((call: Promise<MixerReply>) => {
    call.then(onReply).catch((reason: unknown) => onError(typeof reason === "string" ? reason : "MIXER_UNAVAILABLE"));
  }, [onReply, onError]);

  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    let sent = 0;
    const report = () => {
      const height = Math.round(element.getBoundingClientRect().height);
      if (height > 0 && height !== sent) {
        sent = height;
        void orbApi.panelHeight(height).catch(() => {});
      }
    };
    report();
    const observer = new ResizeObserver(report);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  if (!layout) return null;
  const top = state.panel?.y ?? Math.max(8, layout.cy - 39 * layout.sp);
  const style = { left: `${layout.panel_x}px`, top: `${top}px`, "--sp": layout.sp, visibility: state.panel ? "visible" : "hidden" } as CSSProperties;
  const level = (id: ChannelId, percent: number, final: boolean) => {
    const now = performance.now();
    if (!final && now - lastSend.current < DRAG_SEND_MS) return;
    lastSend.current = now;
    run(mixerApi.level(id, percent));
  };
  const mute = (id: ChannelId) => {
    const reading = view?.[id];
    if (!reading || reading.muted === null) return;
    run(mixerApi.mute(id, !reading.muted));
  };
  const note = channelNote(view, LOCALE);
  const focus = view?.focus;
  const musicReady = controllable(view?.music);
  return (
    <div className="orb-panel" ref={box} style={style} role="group" aria-label={text.focus} onMouseDown={noFocus}>
      <div className="orb-fringe" />
      <button type="button" className="orb-focus" tabIndex={-1} aria-pressed={focus?.active === true} disabled={!musicReady && !focus?.active}
        onMouseDown={noFocus} onClick={() => run(mixerApi.focus())}>
        <svg viewBox="0 0 26 26" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><circle cx="13" cy="13" r="9.5" /><circle cx="13" cy="13" r="3.2" fill="currentColor" stroke="none" /><path d="M13 1.5v4M13 20.5v4M1.5 13h4M20.5 13h4" /></svg>
        <span><span className="orb-focus-t">{text.focus}</span>
          <span className="orb-focus-s">{focus?.active ? text.focusOn(focus.prior_percent) : text.focusOff(focus?.percent ?? 20)}</span></span>
      </button>
      <div className="orb-cols">
        <Fader id="game" name={text.game} sub={text.gameSub} reading={view?.game ?? null} onLevel={level} onMute={mute} />
        <Fader id="kohbor" name={text.kohbor} sub={text.kohborSub} reading={view?.kohbor ?? null} onLevel={level} onMute={mute} />
        <Fader id="music" name={text.music} sub={appLabel(view?.music.exe ?? null)} reading={view?.music ?? null} focused={focus?.active}
          extra={<button type="button" className="orb-change" tabIndex={-1} onMouseDown={noFocus} onClick={() => setPicking(!picking)}>{text.change}</button>}
          onLevel={level} onMute={mute} />
      </div>
      {note && <div className="orb-why"><i />{note[0]}<span>{note[1]}</span></div>}
      {error && <div className="orb-error" role="status">{reasonText(error, LOCALE)}</div>}
      <div className="orb-foot">
        <button type="button" className="orb-reset" tabIndex={-1} onMouseDown={noFocus} onClick={() => run(mixerApi.reset())}>{text.reset}</button>
        {!note && <div className="orb-note">{text.resetNote}</div>}
      </div>
      {picking && (
        <div className="orb-pick" role="menu">
          <small>{text.pickTitle}</small>
          {(view?.music_choices.length ?? 0) === 0 && <small>{text.noChoices}</small>}
          {view?.music_choices.map((exe) => (
            <button type="button" key={exe} role="menuitemradio" tabIndex={-1} aria-checked={exe === view.music.exe} onMouseDown={noFocus}
              onClick={() => { setPicking(false); run(mixerApi.app(exe)); }}>{appLabel(exe)}</button>
          ))}
        </div>
      )}
    </div>
  );
}

export function OrbStage() {
  const [state, setState] = useState<OrbState>(HIDDEN_ORB);
  const [receivedAt, setReceivedAt] = useState(() => performance.now());
  const [now, setNow] = useState(() => performance.now());
  const [view, setView] = useState<MixerView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reduced = useReducedMotion();
  const open = state.visible && state.phase === "open";

  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    void subscribeOrbState((next) => { setState(next); setReceivedAt(performance.now()); }).then((stop) => disposed ? stop() : stops.push(stop));
    void subscribeMixer((reply) => { setView(reply.mixer); setError(reply.ok ? null : reply.code); }).then((stop) => disposed ? stop() : stops.push(stop));
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, []);

  // Animate the charge cue between host polls.
  useEffect(() => {
    if (state.phase !== "charging") return;
    let frame = 0;
    const tick = () => { setNow(performance.now()); frame = requestAnimationFrame(tick); };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [state.phase, receivedAt]);

  // Honest values: re-read while the panel is open (also notices new sessions and device changes).
  useEffect(() => {
    if (!open) { setError(null); return; }
    let live = true;
    const read = () => mixerApi.get().then((reply) => { if (live) { setView(reply.mixer); if (reply.ok) setError(null); } }).catch(() => { if (live) setView(null); });
    void read();
    const timer = window.setInterval(read, REFRESH_MS);
    return () => { live = false; window.clearInterval(timer); };
  }, [open]);

  const onReply = useCallback((reply: MixerReply) => { setView(reply.mixer); setError(reply.ok ? null : reply.code); }, []);
  const onError = useCallback((code: string) => setError(code), []);

  if (!state.visible || !state.layout) return <div className="orb-root" />;
  const layout = state.layout;
  const cue = chargeCue(state, state.phase === "charging" ? now - receivedAt : 0, reduced);
  const out = state.phase === "sliding" || state.phase === "out" || state.phase === "open";
  const discStyle = {
    width: `${layout.disc}px`, height: `${layout.disc}px`, top: `${layout.cy - layout.disc / 2}px`,
    left: `${-layout.disc / 2}px`,
    transform: `translateX(${out ? layout.out_cx : 0}px) scale(${cue.scale})`,
    opacity: cue.opacity,
  } as CSSProperties;
  return (
    <div className={`orb-root${reduced ? " is-reduced" : ""}`} onMouseDown={noFocus}
      onPointerDown={(event) => {
        // Owner decision 2026-11-22: one click on the tucked orb (the host's guarded target) opens the panel.
        event.preventDefault();
        if (state.phase === "tucked" || state.phase === "charging") void orbApi.click("disc").catch(() => {});
      }}>
      <div className={`orb-disc${out ? " is-out" : ""}${open ? " is-open" : ""}${state.locked ? " is-locked" : ""}`} style={discStyle}
        onPointerDown={(event) => {
          event.preventDefault();
          if (state.phase === "out" || state.phase === "open") void orbApi.click("disc").catch(() => {});
        }}>
        <Mark />
        {state.phase === "charging" && (
          <svg className="orb-ring" viewBox="-32 -32 64 64" aria-hidden="true">
            <path className="orb-ring-track" d="M0 -29.5A29.5 29.5 0 0 1 0 29.5" />
            <path className="orb-ring-progress" pathLength={1} style={{ strokeDashoffset: 1 - cue.progress }} d="M0 -29.5A29.5 29.5 0 0 1 0 29.5" />
          </svg>
        )}
      </div>
      {open && <Panel state={state} view={view} error={error} onReply={onReply} onError={onError} />}
    </div>
  );
}
