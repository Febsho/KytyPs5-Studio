import { useEffect, useMemo, useState } from "react";
import { RotateCcw, X } from "lucide-react";
import { DualSenseDiagram } from "./DualSenseDiagram";
import {
  CONTROLLER_ACTIONS,
  controllerControlLabel,
  DEFAULT_CONTROLLER_MAPPING,
  mapControllerControl,
  type ControllerAction,
  type ControllerMapping,
} from "../../utils/controllerMapping";

type Props = {
  mapping: ControllerMapping;
  onSave: (mapping: ControllerMapping) => void;
  onClose: () => void;
};

const readPads = () =>
  Array.from(navigator.getGamepads?.() || []).filter(
    (pad): pad is Gamepad => !!pad && pad.connected,
  );

export function ControllerRemapDialog({ mapping, onSave, onClose }: Props) {
  const [draft, setDraft] = useState(mapping);
  const [pads, setPads] = useState(readPads);
  const [listening, setListening] = useState<ControllerAction | null>(null);
  const [selectedControl, setSelectedControl] = useState(mapping.confirm);
  const controls = useMemo(() => {
    const buttons = Math.max(18, ...pads.map((pad) => pad.buttons.length - 1));
    const axes = Math.max(3, ...pads.map((pad) => pad.axes.length - 1));
    return [
      ...Array.from({ length: buttons + 1 }, (_, index) => `button:${index}`),
      ...Array.from({ length: axes + 1 }, (_, index) => [
        `axis:${index}:-`,
        `axis:${index}:+`,
      ]).flat(),
    ];
  }, [pads]);

  useEffect(() => {
    const update = () => {
      const next = readPads();
      setPads((current) =>
        current.length === next.length &&
        current.every((pad, index) => pad.index === next[index].index && pad.id === next[index].id)
          ? current
          : next,
      );
    };
    window.addEventListener("gamepadconnected", update);
    window.addEventListener("gamepaddisconnected", update);
    const timer = window.setInterval(update, 500);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("gamepadconnected", update);
      window.removeEventListener("gamepaddisconnected", update);
    };
  }, []);

  useEffect(() => {
    if (!listening) return;
    const previous = new Map<string, boolean>();
    const timer = window.setInterval(() => {
      const connected = readPads();
      for (const pad of connected) {
        for (let index = 0; index < pad.buttons.length; index += 1) {
          const key = `${pad.index}:button:${index}`;
          const down = !!pad.buttons[index]?.pressed;
          const wasDown = previous.get(key) || false;
          previous.set(key, down);
          if (down && !wasDown) {
            setDraft((current) => mapControllerControl(current, listening, `button:${index}`));
            setListening(null);
            return;
          }
        }
        for (let index = 0; index < pad.axes.length; index += 1) {
          const value = pad.axes[index] || 0;
          const directions = [
            { suffix: "-", down: value < -0.75 },
            { suffix: "+", down: value > 0.75 },
          ];
          for (const direction of directions) {
            const key = `${pad.index}:axis:${index}:${direction.suffix}`;
            const wasDown = previous.get(key) || false;
            previous.set(key, direction.down);
            if (direction.down && !wasDown) {
              setDraft((current) => mapControllerControl(current, listening, `axis:${index}:${direction.suffix}`));
              setListening(null);
              return;
            }
          }
        }
      }
    }, 60);
    return () => window.clearInterval(timer);
  }, [listening]);

  const updateMapping = (action: ControllerAction, control: string) =>
    setDraft((current) => mapControllerControl(current, action, control));
  const selectedAction = CONTROLLER_ACTIONS.find(
    ({ id }) => draft[id] === selectedControl,
  );
  const assignSelectedControl = (action: ControllerAction) =>
    setDraft((current) => mapControllerControl(current, action, selectedControl));
  return (
    <div className="controller-remap-backdrop" onMouseDown={onClose}>
      <section
        className="controller-remap-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="controller-remap-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="controller-remap-header">
          <div>
            <small className="eyebrow">INPUT / WINDOWS & LINUX</small>
            <h2 id="controller-remap-title">Controller remap</h2>
          </div>
          <button className="title-icon" onClick={onClose} aria-label="Close controller remapping">
            <X size={18} />
          </button>
        </header>
        <div className="controller-remap-body">
          <div className="controller-preview">
            <div className="dualsense-heading">
              <div><strong>DualSense layout</strong><span>{pads[0]?.id || "Select a controller input"}</span></div>
              <span className={`controller-status${pads.length ? " connected" : ""}`} />
            </div>
            <DualSenseDiagram mapping={draft} selectedControl={selectedControl} onSelect={setSelectedControl} />
            <div className="selected-controller-control">
              <div><small>SELECTED CONTROL</small><strong>{controllerControlLabel(selectedControl)}</strong></div>
              <label htmlFor="controller-selected-action">Remap to</label>
              <select
                id="controller-selected-action"
                value={selectedAction?.id || ""}
                onChange={(event) => {
                  if (event.target.value) {
                    assignSelectedControl(event.target.value as ControllerAction);
                  } else if (selectedAction) {
                    setDraft((current) => ({ ...current, [selectedAction.id]: "" }));
                  }
                }}
              >
                <option value="">Unassigned</option>
                {CONTROLLER_ACTIONS.map(({ id, label }) => <option value={id} key={id}>{label}</option>)}
              </select>
            </div>
            <span className="controller-connection-label">
              {pads.length ? `${pads.length} controller${pads.length === 1 ? "" : "s"} connected · click a button to select it` : "Connect a controller · you can still click the diagram to assign controls"}
            </span>
          </div>
          <div className="controller-remap-list">
            {CONTROLLER_ACTIONS.map(({ id, label }) => (
              <div className="controller-remap-row" key={id}>
                <label htmlFor={`controller-${id}`}>{label}</label>
                <select
                  id={`controller-${id}`}
                  value={draft[id]}
                  onChange={(event) => updateMapping(id, event.target.value)}
                >
                  <option value="">Unassigned</option>
                  {controls.map((control) => (
                    <option key={control} value={control}>
                      {controllerControlLabel(control)}
                    </option>
                  ))}
                </select>
                <button
                  className={listening === id ? "primary" : "outline"}
                  disabled={!pads.length || (listening !== null && listening !== id)}
                  onClick={() => setListening(listening === id ? null : id)}
                >
                  {listening === id ? "Press a control…" : "Listen"}
                </button>
              </div>
            ))}
            {listening && <p className="controller-listen-hint">Move a stick or press a button on the controller.</p>}
          </div>
        </div>
        <footer className="controller-remap-footer">
          <button className="outline" onClick={() => setDraft({ ...DEFAULT_CONTROLLER_MAPPING })}>
            <RotateCcw size={14} /> Reset to defaults
          </button>
          <div>
            <button className="outline" onClick={onClose}>Cancel</button>
            <button className="primary" onClick={() => onSave(draft)}>Save mapping</button>
          </div>
        </footer>
      </section>
    </div>
  );
}
