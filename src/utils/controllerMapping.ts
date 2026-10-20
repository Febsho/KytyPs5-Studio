export const CONTROLLER_ACTIONS = [
  { id: "up", label: "Navigate up", defaultControl: "button:12" },
  { id: "down", label: "Navigate down", defaultControl: "button:13" },
  { id: "left", label: "Navigate left", defaultControl: "button:14" },
  { id: "right", label: "Navigate right", defaultControl: "button:15" },
  { id: "confirm", label: "Confirm / click", defaultControl: "button:0" },
  { id: "cancel", label: "Back / close", defaultControl: "button:1" },
  { id: "previousPage", label: "Previous page", defaultControl: "button:4" },
  { id: "nextPage", label: "Next page", defaultControl: "button:5" },
] as const;

export type ControllerAction = (typeof CONTROLLER_ACTIONS)[number]["id"];
export type ControllerMapping = Record<ControllerAction, string>;

export const DEFAULT_CONTROLLER_MAPPING = Object.fromEntries(
  CONTROLLER_ACTIONS.map(({ id, defaultControl }) => [id, defaultControl]),
) as ControllerMapping;

export function mapControllerControl(
  current: ControllerMapping,
  action: ControllerAction,
  control: string,
): ControllerMapping {
  const displaced = CONTROLLER_ACTIONS.find(
    ({ id }) => id !== action && current[id] === control,
  );
  return {
    ...current,
    [action]: control,
    ...(displaced ? { [displaced.id]: current[action] } : {}),
  };
}

export const controllerControlLabel = (control: string) => {
  if (!control) return "Unassigned";
  if (control.startsWith("axis:")) {
    const [, axis, direction] = control.split(":");
    const names = ["Left stick horizontal", "Left stick vertical", "Right stick horizontal", "Right stick vertical"];
    return `${names[Number(axis)] || `Axis ${Number(axis) + 1}`} ${direction === "-" ? "−" : "+"}`;
  }
  const index = Number(control.split(":")[1]);
  const labels = [
    "South · A / Cross", "East · B / Circle", "West · X / Square", "North · Y / Triangle",
    "Left bumper", "Right bumper", "Left trigger", "Right trigger", "Select / Share",
    "Start / Options", "Left stick press", "Right stick press", "D-pad up", "D-pad down",
    "D-pad left", "D-pad right", "System button", "Touchpad click", "Microphone mute",
  ];
  return labels[index] || `Button ${index + 1}`;
};

export function mappedControlPressed(pad: Gamepad, control: string) {
  if (control.startsWith("axis:")) {
    const [, axisIndex, direction] = control.split(":");
    const value = pad.axes[Number(axisIndex)] || 0;
    return direction === "-" ? value < -0.65 : value > 0.65;
  }
  const index = Number(control.split(":")[1]);
  return !!pad.buttons[index]?.pressed;
}

export function readControllerMapping(value: string | null): ControllerMapping {
  if (!value) return DEFAULT_CONTROLLER_MAPPING;
  try {
    const parsed = JSON.parse(value) as Partial<ControllerMapping>;
    return Object.fromEntries(
      CONTROLLER_ACTIONS.map(({ id }) => [
        id,
        typeof parsed[id] === "string" &&
        (parsed[id] === "" || /^(button:\d+|axis:\d+:[+-])$/.test(parsed[id]!))
          ? parsed[id]
          : DEFAULT_CONTROLLER_MAPPING[id],
      ]),
    ) as ControllerMapping;
  } catch {
    return DEFAULT_CONTROLLER_MAPPING;
  }
}
