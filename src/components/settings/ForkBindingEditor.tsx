import { useEffect, useState } from "react";
import { DualSenseDiagram } from "./DualSenseDiagram";

const groups = [
  ["Face buttons", ["Cross", "Circle", "Square", "Triangle"]],
  ["Directional pad", ["Up", "Down", "Left", "Right"]],
  ["Shoulders", ["L1", "R1", "L2", "R2", "L3", "R3"]],
  ["Sticks", ["LeftStickLeft", "LeftStickRight", "LeftStickUp", "LeftStickDown", "RightStickLeft", "RightStickRight", "RightStickUp", "RightStickDown"]],
  ["Other", ["Options", "TouchPad", "TouchPadRight", "SpeakerVolume", "VibrationIntensity", "TriggerEffectIntensity", "GyroToggle", "TouchPadMode"]],
] as const;
const padSources = ["South", "East", "West", "North", "Back", "Guide", "Start", "LeftStick", "RightStick", "LeftShoulder", "RightShoulder", "DpadUp", "DpadDown", "DpadLeft", "DpadRight", "Misc1", "RightPaddle1", "LeftPaddle1", "RightPaddle2", "LeftPaddle2", "Touchpad", "LeftTrigger", "RightTrigger"];
const mouse = ["Mouse:Left", "Mouse:Middle", "Mouse:Right", "Mouse:X1", "Mouse:X2"];
const reserved = new Set(["Escape", "F1", "F2", "F7", "F8", "F11"]);

// SDL_GetKeyFromName expects SDL key names, not browser KeyboardEvent.code.
function keyName(event: KeyboardEvent): string | null {
  if (reserved.has(event.key) || event.repeat) return null;
  const modifier: Record<string, string> = { Control: "Ctrl", Shift: "Shift", Alt: "Alt", Meta: "GUI" };
  if (modifier[event.key]) return `${event.location === 2 ? "Right" : "Left"} ${modifier[event.key]}`;
  const special: Record<string, string> = { " ": "Space", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right", Backspace: "Backspace", Enter: "Return", Tab: "Tab", Delete: "Delete", Insert: "Insert", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown" };
  if (special[event.key]) return special[event.key];
  if (/^[a-z0-9]$/i.test(event.key)) return event.key.toUpperCase();
  if (/^F(?:[3-6]|9|10|12)$/.test(event.key)) return event.key;
  return null;
}

export function ForkBindingEditor({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const [enabled, setEnabled] = useState(false);
  const [control, setControl] = useState("Cross");
  const [source, setSource] = useState<"keyboard" | "pad">("keyboard");
  const [listening, setListening] = useState(false);
  const [pad, setPad] = useState("South");
  const [error, setError] = useState("");
  const add = (input: string) => {
    const prefix = source === "pad" ? `Pad:${control}=` : `${control}=`;
    const lines = value.split("\n").filter(Boolean).filter((line) => !line.startsWith(prefix));
    onChange([...lines, `${prefix}${input}`].join("\n"));
    setListening(false);
    setError("");
  };
  useEffect(() => {
    if (!listening) return;
    const key = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") { setListening(false); return; }
      const name = keyName(event);
      if (name) add(name); else setError("This key is reserved or cannot be captured. Use the manual mapping fields below for other SDL key names.");
    };
    const click = (event: MouseEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const name = [mouse[0], mouse[1], mouse[2], mouse[3], mouse[4]][event.button];
      if (name) add(name);
    };
    window.addEventListener("keydown", key, true);
    window.addEventListener("mousedown", click, true);
    return () => { window.removeEventListener("keydown", key, true); window.removeEventListener("mousedown", click, true); };
  });
  const padControl = !control.startsWith("LeftStick") && !control.startsWith("RightStick");
  return <div className="keymap-editor">
    <label className="checkbox"><input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} /> Use KytyPS5 fork binding editor</label>
    <small>Enable only if your emulator supports KytyPS5 fork mapping names. The presence of --keymap alone does not verify its syntax.</small>
    {enabled && <>
    <p>Bindings are passed to the selected emulator with --keymap. The first custom keyboard or controller binding replaces that device's default bindings; add every binding you need.</p>
    <div className="fork-binding-diagram"><DualSenseDiagram targets selectedControl={control} onSelect={setControl} /></div>
    <div className="buttonrow">
      <label>Device <select value={source} onChange={(event) => { setSource(event.target.value as "keyboard" | "pad"); setListening(false); }}><option value="keyboard">Keyboard / mouse</option><option value="pad">Controller</option></select></label>
      <label>PS5 control <select value={control} onChange={(event) => setControl(event.target.value)}>{groups.map(([label, controls]) => <optgroup key={label} label={label}>{controls.map((item) => <option key={item} value={item}>{item}</option>)}</optgroup>)}</select></label>
      {source === "keyboard" ? <button type="button" className="outline" onClick={() => setListening(true)}>{listening ? "Press a key or mouse button…" : "Click to bind"}</button> : <><label>Controller input <select value={pad} onChange={(event) => setPad(event.target.value)}>{padSources.map((item) => <option key={item}>{item}</option>)}</select></label><button type="button" className="outline" disabled={!padControl} onClick={() => add(pad)}>Add binding</button></>}
    </div>
    {!padControl && source === "pad" && <small>The fork does not accept gamepad buttons for stick directions.</small>}
    {listening && <small>Press Escape to cancel. F1, F2, F7, F8 and F11 are reserved by the emulator.</small>}
    {error && <small role="alert">{error}</small>}
    </>}
  </div>;
}
