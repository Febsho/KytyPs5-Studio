import type { Profile, ProfileCapabilities } from "../types/studio";

export function availableProfileOptions(
  capabilities: ProfileCapabilities | null | undefined,
): string[] {
  if (!capabilities) return [];
  if (capabilities.available_options) return capabilities.available_options;
  return capabilities.options.filter((flag) => {
    const state = capabilities.option_states?.[flag]?.state;
    return state !== "unavailable" && state !== "unsupported";
  });
}

export function profileWithOptions(
  profile: Profile,
  options: Record<string, string>,
): Profile {
  const width = options["--screen-width"];
  const height = options["--screen-height"];
  return {
    ...profile,
    screen_width: width ? Number(width) : null,
    screen_height: height ? Number(height) : null,
    present_mode: options["--present-mode"] || null,
    perf_overlay: options["--perf-overlay"] === "true",
    fullscreen: options["--fullscreen"] === "true",
    controller_color: options["--controller-color"] || null,
    controller_volume: options["--controller-volume"]
      ? Number(options["--controller-volume"])
      : null,
    controller_vibration: options["--controller-vibration"]
      ? Number(options["--controller-vibration"])
      : null,
    keyboard_player: options["--keyboard-player"]
      ? Number(options["--keyboard-player"])
      : null,
    options,
  };
}
