import { CONTROLLER_ACTIONS } from "../../utils/controllerMapping";
import type { ControllerMapping } from "../../utils/controllerMapping";

type Props = {
  mapping?: ControllerMapping;
  targets?: boolean;
  selectedControl: string;
  onSelect: (control: string) => void;
};

const targetControls: Record<string, string> = {
  "button:6": "L2", "button:4": "L1", "button:7": "R2", "button:5": "R1",
  "button:9": "Options", "button:17": "TouchPad", "button:12": "Up", "button:13": "Down", "button:14": "Left", "button:15": "Right",
  "axis:0:-": "LeftStickLeft", "axis:0:+": "LeftStickRight", "axis:1:-": "LeftStickUp", "axis:1:+": "LeftStickDown",
  "axis:2:-": "RightStickLeft", "axis:2:+": "RightStickRight", "axis:3:-": "RightStickUp", "axis:3:+": "RightStickDown",
  "button:10": "L3", "button:11": "R3", "button:3": "Triangle", "button:1": "Circle", "button:2": "Square", "button:0": "Cross",
};

export function DualSenseDiagram({ mapping, targets = false, selectedControl, onSelect }: Props) {
  const hotspot = (control: string, label: string, x: number, y: number, width: number, height: number) => {
    const target = targets ? targetControls[control] : control;
    if (!target) return null;
    const assignment = mapping && CONTROLLER_ACTIONS.find(({ id }) => mapping[id] === control)?.label;
    return (
      <g
        key={control}
        className={`dualsense-hotspot${selectedControl === target ? " selected" : ""}`}
        role="button"
        tabIndex={0}
        aria-label={targets ? label : `${label}, ${assignment ? `assigned to ${assignment}` : "unassigned"}`}
        onClick={() => onSelect(target)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onSelect(target);
          }
        }}
      >
        <title>{targets ? label : `${label} — ${assignment || "Unassigned"}`}</title>
        <rect x={x} y={y} width={width} height={height} rx={Math.min(width, height) / 2} />
      </g>
    );
  };

  return (
    <svg className="dualsense-illustration" viewBox="0 0 1000 660" role="group" aria-label="Interactive PlayStation 5 DualSense controller. Select a button to remap it.">
      <g className="dualsense-drawing">
        <path className="dualsense-trigger" d="M187 91V68q0-21 28-29l99-17 9 70M813 91V68q0-21-28-29l-99-17-9 70" />
        <path className="dualsense-shoulder" d="M183 111q8-29 49-34l83-10 24 37M817 111q-8-29-49-34l-83-10-24 37" />
        <path className="dualsense-outline" d="M335 84q-38-10-90-8-57 0-91 38-33 37-55 123-30 116-18 253 7 97 42 133 20 20 43 18 22-2 39-31l113-197q24-42 63-40h238q39-2 63 40l113 197q17 29 39 31 23 2 43-18 35-36 42-133 12-137-18-253-22-86-55-123-34-38-91-38-52-2-90 8-28-20-82-22H417q-54 2-82 22Z" />
        <path className="dualsense-panel-seam" d="M334 84q15 13 19 40l19 139q4 31-20 61-23 28-51 65-56 73-119 226M666 84q-15 13-19 40l-19 139q-4 31 20 61 23 28 51 65 56 73 119 226" />
        <path className="dualsense-core-seam" d="M353 274q24 24 65 25h164q41-1 65-25" />
        <path className="dualsense-touchpad" d="M363 91q19-15 60-17h154q41 2 60 17l-16 143q-3 50-55 51H434q-52-1-55-51Z" />
        <path className="dualsense-touch-rim" d="M380 265q13 30 55 31h130q42-1 55-31" />
        <path className="dualsense-small-button" d="M325 130q10-4 15 4l7 28q2 11-9 14-10 2-13-8l-7-26q-2-8 7-12ZM675 130q-10-4-15 4l-7 28q-2 11 9 14 10 2 13-8l7-26q2-8-7-12Z" />
        <path className="dualsense-detail" d="m321 115-7-11m14 9V99m8 16 6-10m322 10 6-10m2 8V99m8 16 7-11" />
        <g className="dualsense-dpad">
          <path d="M244 184h28q12 0 12 12v25q0 8-7 14l-12 10q-7 6-14 0l-12-10q-7-6-7-14v-25q0-12 12-12Z" />
          <path d="M251 286q7-6 14 0l12 10q7 6 7 14v25q0 12-12 12h-28q-12 0-12-12v-25q0-8 7-14Z" />
          <path d="M193 252q0-12 12-12h25q8 0 14 7l10 12q6 7 0 14l-10 12q-6 7-14 7h-25q-12 0-12-12Z" />
          <path d="M272 259l10-12q6-7 14-7h25q12 0 12 12v28q0 12-12 12h-25q-8 0-14-7l-10-12q-6-7 0-14Z" />
          <path className="dualsense-glyph" d="m258 199-9 11h18Zm0 101-9-11h18Zm-50-36 11-9v18Zm101 0-11-9v18Z" />
        </g>
        <g className="dualsense-face-buttons">
          <circle cx="756" cy="204" r="34"/><circle cx="826" cy="264" r="34"/>
          <circle cx="686" cy="264" r="34"/><circle cx="756" cy="324" r="34"/>
          <path className="dualsense-glyph" d="m756 185-18 31h36Zm-86 63h32v32h-32Zm70 62 32 29m0-29-32 29" />
          <circle className="dualsense-glyph" cx="826" cy="264" r="17" />
        </g>
        <g className="dualsense-stick"><circle cx="370" cy="397" r="63"/><circle cx="370" cy="397" r="51"/><circle cx="370" cy="397" r="43"/><circle cx="630" cy="397" r="63"/><circle cx="630" cy="397" r="51"/><circle cx="630" cy="397" r="43"/></g>
        <g className="dualsense-speaker"><circle cx="468" cy="319" r="3"/><circle cx="484" cy="315" r="3"/><circle cx="500" cy="314" r="3"/><circle cx="516" cy="315" r="3"/><circle cx="532" cy="319" r="3"/><circle cx="476" cy="332" r="3"/><circle cx="492" cy="330" r="3"/><circle cx="508" cy="330" r="3"/><circle cx="524" cy="332" r="3"/></g>
        <path className="dualsense-ps-button" d="M491 353v34l9-6 11 7v-35q-9-7-20 0Zm-8 35q17 11 34 0" />
        <rect className="dualsense-mute" x="478" y="428" width="44" height="16" rx="8" />
        <path className="dualsense-detail" d="M500 456v13m-6-8q6 8 12 0m-8-10 4 14" />
        <text className="dualsense-button-label" x="245" y="54">L2</text><text className="dualsense-button-label" x="755" y="54">R2</text>
        <text className="dualsense-button-label" x="246" y="103">L1</text><text className="dualsense-button-label" x="754" y="103">R1</text>
        <text className="dualsense-stick-label" x="370" y="402">L3</text><text className="dualsense-stick-label" x="630" y="402">R3</text>
      </g>
      {hotspot("button:6", "L2 / left trigger", 181, 29, 136, 50)}
      {hotspot("button:4", "L1 / left bumper", 180, 79, 140, 44)}
      {hotspot("button:7", "R2 / right trigger", 683, 29, 136, 50)}
      {hotspot("button:5", "R1 / right bumper", 680, 79, 140, 44)}
      {hotspot("button:8", "Create / Share", 314, 124, 39, 55)}
      {hotspot("button:9", "Options", 647, 124, 39, 55)}
      {hotspot("button:17", "Touchpad click", 366, 83, 268, 195)}
      {hotspot("button:12", "D-pad up", 229, 181, 58, 75)}
      {hotspot("button:13", "D-pad down", 229, 265, 58, 55)}
      {hotspot("button:14", "D-pad left", 190, 233, 64, 58)}
      {hotspot("button:15", "D-pad right", 272, 233, 54, 58)}
      {hotspot("axis:0:-", "Left stick left", 306, 386, 25, 22)}
      {hotspot("axis:0:+", "Left stick right", 409, 386, 25, 22)}
      {hotspot("axis:1:-", "Left stick up", 359, 333, 22, 25)}
      {hotspot("axis:1:+", "Left stick down", 359, 436, 22, 25)}
      {hotspot("axis:2:-", "Right stick left", 566, 386, 25, 22)}
      {hotspot("axis:2:+", "Right stick right", 669, 386, 25, 22)}
      {hotspot("axis:3:-", "Right stick up", 619, 333, 22, 25)}
      {hotspot("axis:3:+", "Right stick down", 619, 436, 22, 25)}
      {hotspot("button:10", "Left stick press", 340, 367, 60, 60)}
      {hotspot("button:11", "Right stick press", 600, 367, 60, 60)}
      {hotspot("button:16", "PlayStation button", 477, 347, 46, 51)}
      {hotspot("button:18", "Microphone mute", 476, 424, 48, 25)}
      {hotspot("button:3", "Triangle / north", 722, 170, 68, 68)}
      {hotspot("button:1", "Circle / east", 792, 230, 68, 68)}
      {hotspot("button:2", "Square / west", 652, 230, 68, 68)}
      {hotspot("button:0", "Cross / south", 722, 290, 68, 68)}
    </svg>
  );
}
