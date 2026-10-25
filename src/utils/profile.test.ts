import { describe, expect, it } from "vitest";
import type { Profile } from "../types/studio";
import { availableProfileOptions, profileWithOptions } from "./profile";

const emptyProfile: Profile = {
  screen_width: null,
  screen_height: null,
  present_mode: null,
  perf_overlay: false,
  fullscreen: false,
  controller_color: null,
  controller_volume: null,
  controller_vibration: null,
  keyboard_player: null,
  options: {},
};

describe("profileWithOptions", () => {
  it("synchronizes legacy fields with inherited CLI overrides", () => {
    const options = {
      "--screen-width": "1920",
      "--screen-height": "1080",
      "--present-mode": "Mailbox",
      "--perf-overlay": "true",
      "--fullscreen": "true",
      "--controller-volume": "65",
      "--keyboard-player": "2",
    };
    expect(profileWithOptions(emptyProfile, options)).toMatchObject({
      screen_width: 1920,
      screen_height: 1080,
      present_mode: "Mailbox",
      perf_overlay: true,
      fullscreen: true,
      controller_volume: 65,
      keyboard_player: 2,
      options,
    });
  });

  it("clears legacy values when the override is removed", () => {
    const previous: Profile = {
      ...emptyProfile,
      screen_width: 1920,
      fullscreen: true,
      options: { "--screen-width": "1920", "--fullscreen": "true" },
    };
    expect(profileWithOptions(previous, {})).toMatchObject({
      screen_width: null,
      fullscreen: false,
      options: {},
    });
  });
});

describe("availableProfileOptions", () => {
  it("uses this build's advertised options while excluding host-unavailable flags", () => {
    expect(availableProfileOptions({
      screen_width: false,
      screen_height: false,
      present_mode: false,
      perf_overlay: false,
      fullscreen: false,
      controller_color: false,
      controller_volume: false,
      controller_vibration: false,
      keyboard_player: false,
      options: ["--old-build-option", "--windows-only"],
      available_options: ["--old-build-option"],
      verified_options: [],
      option_states: {
        "--old-build-option": { state: "advertised_unverified", explanation: "In this build help." },
        "--windows-only": { state: "unavailable", explanation: "Host restriction." },
      },
    })).toEqual(["--old-build-option"]);
  });
});
