import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { CliOptionMetadata } from "../../types/studio";

export const cliOptionInfo: Record<
  string,
  {
    label: string;
    group: "Graphics" | "Controls" | "System" | "Content" | "Advanced";
    kind:
      | "switch"
      | "bool"
      | "text"
      | "number"
      | "enum"
      | "path"
      | "keymap"
      | "color";
    choices?: string[];
    choiceLabels?: Record<string, string>;
    directory?: boolean;
    hint?: string;
  }
> = {
  "--screen-width": {
    label: "Screen width",
    group: "Graphics",
    kind: "number",
    hint: "Window resolution request; the display may choose a different mode.",
  },
  "--screen-height": {
    label: "Screen height",
    group: "Graphics",
    kind: "number",
    hint: "Window resolution request; the display may choose a different mode.",
  },
  "--present-mode": {
    label: "Present mode",
    group: "Graphics",
    kind: "enum",
    choices: ["Fifo", "Mailbox", "Immediate"],
    hint: "Requests a Vulkan present mode; support depends on the selected device.",
  },
  "--fullscreen": {
    label: "Fullscreen window",
    group: "Graphics",
    kind: "switch",
  },
  "--perf-overlay": {
    label: "Performance overlay",
    group: "Graphics",
    kind: "bool",
  },
  "--gpu": {
    label: "Vulkan GPU",
    group: "Graphics",
    kind: "number",
    hint: "-1 selects automatically. Otherwise use the device index reported by Vulkan diagnostics; without diagnostics, enter the KytyPS5 physical-device index.",
  },
  "--hdr": {
    label: "Request HDR10 output",
    group: "Graphics",
    kind: "switch",
    hint: "Requests HDR10 output; Studio cannot confirm display or game HDR support.",
  },
  "--hide-cursor": {
    label: "Hide cursor after inactivity",
    group: "Graphics",
    kind: "switch",
  },
  "--vblank-frequency": {
    label: "Virtual VBlank frequency (Hz)",
    group: "Graphics",
    kind: "number",
    hint: "KytyPS5 accepts 30–360 Hz. This sets virtual VBlank; it does not guarantee monitor refresh or game support.",
  },
  "--tessellation": {
    label: "Enable tessellation",
    group: "Graphics",
    kind: "switch",
  },
  "--shader-optimization-type": {
    label: "Shader optimization",
    group: "Graphics",
    kind: "enum",
    choices: ["None", "Size", "Performance"],
    hint: "This changes shader optimization, not precompilation. Shader precompile support depends on the selected fork.",
  },
  "--controller-color": {
    label: "Controller lightbar color",
    group: "Controls",
    kind: "color",
  },
  "--controller-volume": {
    label: "Controller speaker volume",
    group: "Controls",
    kind: "number",
  },
  "--controller-vibration": {
    label: "Controller vibration intensity",
    group: "Controls",
    kind: "number",
  },
  "--keyboard-player": {
    label: "Keyboard player (0–4)",
    group: "Controls",
    kind: "number",
  },
  "--keymap": {
    label: "Controller key mappings",
    group: "Controls",
    kind: "keymap",
    hint: "One mapping per line, Control=Input. Repeated as separate --keymap arguments.",
  },
  "--mic": {
    label: "Microphone device",
    group: "Controls",
    kind: "text",
    hint: "Enter the device name recognized by KytyPS5's audio backend. Leave empty to disable microphone capture.",
  },
  "--user-name": { label: "Local username", group: "System", kind: "text" },
  "--user-id": {
    label: "User ID",
    group: "System",
    kind: "number",
    hint: "KytyPS5 allows 0–2,147,482,879 except reserved IDs 254 and 255.",
  },
  "--console-language": {
    label: "Console language",
    group: "System",
    kind: "enum",
    choices: Array.from({ length: 30 }, (_, index) => String(index)),
    choiceLabels: {
      "0": "Japanese", "1": "English (United States)", "2": "French (France)",
      "3": "Spanish (Spain)", "4": "German", "5": "Italian", "6": "Dutch",
      "7": "Portuguese (Portugal)", "8": "Russian", "9": "Korean",
      "10": "Chinese (Traditional)", "11": "Chinese (Simplified)", "12": "Finnish",
      "13": "Swedish", "14": "Danish", "15": "Norwegian", "16": "Polish",
      "17": "Portuguese (Brazil)", "18": "English (United Kingdom)", "19": "Turkish",
      "20": "Spanish (Latin America)", "21": "Arabic", "22": "French (Canada)",
      "23": "Czech", "24": "Hungarian", "25": "Greek", "26": "Romanian",
      "27": "Thai", "28": "Vietnamese", "29": "Indonesian",
    },
  },
  "--network-connected": {
    label: "Network connected",
    group: "System",
    kind: "bool",
  },
  "--no-splash": {
    label: "Skip startup splash",
    group: "System",
    kind: "switch",
  },
  "--skip-notice-screen": {
    label: "Skip notice screen",
    group: "System",
    kind: "bool",
  },
  "--trophy-notifications": {
    label: "Trophy notifications",
    group: "System",
    kind: "bool",
  },
  "--trophy-duration": {
    label: "Trophy notification duration (5–10 sec)",
    group: "System",
    kind: "number",
  },
  "--game-patch": { label: "Game patch file", group: "Content", kind: "path" },
  "--addcont-dir": {
    label: "Add-on content directory",
    group: "Content",
    kind: "path",
    directory: true,
  },
  "--playgo-hack": {
    label: "PlayGo fallback",
    group: "Content",
    kind: "switch",
  },
  "--amd-cpu": {
    label: "AMD CPU instruction patches",
    group: "Content",
    kind: "switch",
  },
  "--vr": { label: "Virtual VR headset", group: "Content", kind: "switch" },
  "--vulkan-validation": {
    label: "Vulkan validation",
    group: "Advanced",
    kind: "bool",
  },
  "--gpu-assisted-validation": {
    label: "GPU-assisted validation",
    group: "Advanced",
    kind: "bool",
  },
  "--shader-validation": {
    label: "Shader validation",
    group: "Advanced",
    kind: "bool",
  },
  "--shader-log-direction": {
    label: "Shader log direction",
    group: "Advanced",
    kind: "enum",
    choices: ["Silent", "Console", "File"],
  },
  "--shader-log-folder": {
    label: "Shader log folder",
    group: "Advanced",
    kind: "path",
    directory: true,
  },
  "--command-buffer-dump": {
    label: "Command buffer dumps",
    group: "Advanced",
    kind: "bool",
  },
  "--command-buffer-dump-folder": {
    label: "Command buffer dump folder",
    group: "Advanced",
    kind: "path",
    directory: true,
  },
  "--graphics-debug-dump": {
    label: "Graphics debug dumps",
    group: "Advanced",
    kind: "bool",
  },
  "--printf-direction": {
    label: "Guest printf direction",
    group: "Advanced",
    kind: "enum",
    choices: ["Silent", "Console", "File"],
  },
  "--printf-output-file": {
    label: "Guest printf output file",
    group: "Advanced",
    kind: "path",
  },
  "--profile": { label: "Tracy profiler", group: "Advanced", kind: "switch" },
  "--spirv-debug-printf": {
    label: "SPIR-V debug printf",
    group: "Advanced",
    kind: "bool",
  },
  "--readback-linear-images": {
    label: "Linear image readback",
    group: "Advanced",
    kind: "bool",
  },
  "--sync-raw-image-buffers": {
    label: "Synchronize raw image buffers",
    group: "Advanced",
    kind: "bool",
  },
  "--rd": { label: "RenderDoc capture", group: "Advanced", kind: "switch" },
};
export function profileArgumentPreview(
  options: Record<string, string>,
  inheritedOptions: Record<string, string>,
  available: string[],
  switchOptions: string[] = [],
): string {
  const effective = { ...inheritedOptions, ...options };
  const args = Object.entries(effective)
    .filter(([flag]) => available.includes(flag))
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
    .flatMap(([flag, value]) => {
      const kind = cliOptionInfo[flag]?.kind;
      if (kind === "switch" || switchOptions.includes(flag))
        return value === "true" ? [flag] : [];
      if (kind === "keymap")
        return value
          .split("\n")
          .filter(Boolean)
          .flatMap((mapping) => [flag, mapping.trim()]);
      return [flag, value];
    });
  return JSON.stringify(args, null, 2);
}
type GpuDevice = { index: number; name: string };
type Props = {
  available: string[];
  options: Record<string, string>;
  onChange: (options: Record<string, string>) => void;
  gpuDevices?: GpuDevice[];
  inheritedOptions?: Record<string, string>;
  switchOptions?: string[];
  previewTitle?: string;
  showUnavailable?: boolean;
  optionMetadata?: Record<string, CliOptionMetadata>;
};

export function ProfileOptionFields({
  available,
  options,
  onChange,
  gpuDevices = [],
  inheritedOptions = {},
  switchOptions = [],
  previewTitle = "CLI argument preview",
  showUnavailable = true,
  optionMetadata = {},
}: Props) {
  const [dialogError, setDialogError] = useState("");
  const update = (flag: string, value: string) => {
    const next = { ...options };
    if (value === "") delete next[flag];
    else next[flag] = value;
    if (flag === "--amd-cpu" && value === "true") delete next["--no-amd-cpu"];
    if (flag === "--no-amd-cpu" && value === "true") delete next["--amd-cpu"];
    onChange(next);
  };
  return (
    <>
      {(["Graphics", "Controls", "System", "Content", "Advanced"] as const).map(
        (group) => {
          const fields = available.filter((flag) => (cliOptionInfo[flag]?.group ?? "Advanced") === group);
          if (!fields.length) return null;
          const body = (
            <>
              <div className="formgrid">
                {fields.map((flag) => {
                  const staticInfo = cliOptionInfo[flag];
                  const metadata = optionMetadata[flag];
                  const metadataBoolean = metadata?.choices?.length === 2 &&
                    metadata.choices.includes("true") && metadata.choices.includes("false");
                  const metadataHasChoices = (metadata?.choices?.length ?? 0) > 1;
                  const inferredKind = metadata?.kind === "bool" && switchOptions.includes(flag)
                    ? "switch"
                    : metadata?.kind;
                  const info = staticInfo
                    ? {
                        ...staticInfo,
                        kind: (staticInfo.kind === "switch" || staticInfo.kind === "bool"
                          ? staticInfo.kind
                          : metadataBoolean
                            ? "bool"
                            : metadataHasChoices
                              ? "enum"
                              : staticInfo.kind) as typeof staticInfo.kind,
                        choices: metadata?.choices?.length ? metadata.choices : staticInfo.choices,
                        hint: staticInfo.hint || metadata?.description,
                      }
                    : {
                      label: flag,
                      group: "Advanced" as const,
                      kind: switchOptions.includes(flag) ? ("switch" as const) : ((inferredKind || "text") as "switch" | "bool" | "text" | "number" | "enum" | "path" | "keymap" | "color"),
                      choices: metadata?.choices,
                      hint: metadata?.description || "Advertised by this emulator build; Studio passes this value as an individual argument.",
                    };
                  const isInherited =
                    options[flag] === undefined &&
                    inheritedOptions[flag] !== undefined;
                  const value = options[flag] ?? inheritedOptions[flag] ?? "";
                  const set = (next: string) => update(flag, next);
                  return (
                    <div
                      className={info.kind === "keymap" ? "wide" : ""}
                      key={flag}
                    >
                      <label>{info.label}</label>
                      {info.kind === "switch" || info.kind === "bool" ? (
                        <label className="checkbox">
                          <input
                            type="checkbox"
                            checked={value === "true"}
                            onChange={(event) =>
                              set(event.target.checked ? "true" : "false")
                            }
                          />
                          Enabled
                        </label>
                      ) : info.kind === "color" ? (
                        <input
                          type="color"
                          value={value || "#ffffff"}
                          onChange={(event) => set(event.target.value)}
                        />
                      ) : info.kind === "enum" ? (
                        <select
                          aria-label={info.label}
                          value={value}
                          onChange={(event) => set(event.target.value)}
                        >
                          <option value="">Emulator default</option>
                          {value && !info.choices?.includes(value) && (
                            <option value={value}>{value} · saved value</option>
                          )}
                          {info.choices?.map((choice) => (
                            <option key={choice} value={choice}>{info.choiceLabels?.[choice] ? `${info.choiceLabels[choice]} (${choice})` : choice}</option>
                          ))}
                        </select>
                      ) : flag === "--gpu" && gpuDevices.length > 0 ? (
                        <select
                          aria-label={info.label}
                          value={value || "-1"}
                          onChange={(event) => set(event.target.value)}
                        >
                          <option value="-1">Automatic selection</option>
                          {gpuDevices.map((device) => (
                            <option key={device.index} value={device.index}>
                              {device.index} · {device.name}
                            </option>
                          ))}
                          {value !== "" &&
                            value !== "-1" &&
                            !gpuDevices.some(
                              (device) => String(device.index) === value,
                            ) && (
                              <option value={value}>
                                {value} · Not reported by Vulkan diagnostics
                              </option>
                            )}
                        </select>
                      ) : info.kind === "keymap" ? (
                        <div className="keymap-editor">
                          {value
                            .split("\n")
                            .filter(Boolean)
                            .map((mapping, index) => {
                              const separator = mapping.indexOf("=");
                              const control =
                                separator < 0
                                  ? mapping
                                  : mapping.slice(0, separator);
                              const input =
                                separator < 0
                                  ? ""
                                  : mapping.slice(separator + 1);
                              const updateMapping = (
                                nextControl: string,
                                nextInput: string,
                              ) => {
                                const mappings = value
                                  .split("\n")
                                  .filter(Boolean);
                                mappings[index] = `${nextControl}=${nextInput}`;
                                set(mappings.join("\n"));
                              };
                              return (
                                <div className="keymap-row" key={index}>
                                  <input
                                    aria-label={`Controller control ${index + 1}`}
                                    value={control}
                                    placeholder="Control"
                                    onChange={(event) =>
                                      updateMapping(event.target.value, input)
                                    }
                                  />
                                  <span aria-hidden="true">→</span>
                                  <input
                                    aria-label={`Keyboard input ${index + 1}`}
                                    value={input}
                                    placeholder="Input"
                                    onChange={(event) =>
                                      updateMapping(control, event.target.value)
                                    }
                                  />
                                  <button
                                    type="button"
                                    className="outline"
                                    aria-label={`Remove mapping ${index + 1}`}
                                    onClick={() =>
                                      set(
                                        value
                                          .split("\n")
                                          .filter(Boolean)
                                          .filter((_, row) => row !== index)
                                          .join("\n"),
                                      )
                                    }
                                  >
                                    Remove
                                  </button>
                                </div>
                              );
                            })}
                          <button
                            type="button"
                            className="outline"
                            onClick={() =>
                              set(
                                [
                                  ...value.split("\n").filter(Boolean),
                                  "=",
                                ].join("\n"),
                              )
                            }
                          >
                            Add mapping
                          </button>
                        </div>
                      ) : (
                        <div className="buttonrow">
                          <input
                            aria-label={info.label}
                            type={info.kind === "number" ? "number" : "text"}
                            min={
                              metadata?.minimum ?? (flag === "--user-id"
                                ? 0
                                : flag === "--gpu"
                                  ? -1
                                  : flag === "--vblank-frequency"
                                    ? 30
                                    : undefined)
                            }
                            max={
                              metadata?.maximum ?? (flag === "--user-id"
                                ? 2147482879
                                : flag === "--vblank-frequency"
                                  ? 360
                                  : undefined)
                            }
                            placeholder={
                              flag === "--mic"
                                ? "Device name (empty disables capture)"
                                : "Emulator default"
                            }
                            value={value}
                            onChange={(event) => set(event.target.value)}
                          />
                          {info.kind === "path" && (
                            <button
                              type="button"
                              className="outline"
                              onClick={async () => {
                                setDialogError("");
                                try {
                                  const path =
                                    flag === "--printf-output-file"
                                      ? await save({
                                          defaultPath: value || undefined,
                                        })
                                      : await open({
                                          directory: !!info.directory,
                                          multiple: false,
                                        });
                                  if (typeof path === "string") set(path);
                                } catch (error) {
                                  setDialogError(`Could not open the file picker: ${String(error)}`);
                                }
                              }}
                            >
                              Browse
                            </button>
                          )}
                        </div>
                      )}
                      {options[flag] !== undefined && (
                        <button
                          type="button"
                          className="text-button"
                          onClick={() => update(flag, "")}
                        >
                          {inheritedOptions[flag] !== undefined
                            ? "Reset override"
                            : "Reset to emulator default"}
                        </button>
                      )}
                      {isInherited && (
                        <small className="hint">
                          Inherited from global defaults
                        </small>
                      )}
                      {info.hint && <small className="hint">{info.hint}</small>}
                    </div>
                  );
                })}
              </div>
            </>
          );
          return group === "Advanced" ? (
            <details className="advanced-profile" key={group}>
              <summary>
                Advanced diagnostics <small>Expensive debugging options</small>
              </summary>
              {body}
            </details>
          ) : (
            <section className="profile-group" key={group}>
              <h4>{group}</h4>
              {body}
            </section>
          );
        },
      )}
      {dialogError && <small role="alert" className="error-note">{dialogError}</small>}
      {showUnavailable && Object.entries(cliOptionInfo).some(([flag]) => !available.includes(flag)) && (
        <details className="unsupported-profile-options">
          <summary>
            Unavailable options ({Object.keys(cliOptionInfo).filter((flag) => !available.includes(flag)).length})
          </summary>
          <p className="hint">
            This emulator's --help output does not advertise these options. They stay disabled for this build.
          </p>
          <ul>
            {Object.entries(cliOptionInfo)
              .filter(([flag]) => !available.includes(flag))
              .map(([flag, info]) => (
                <li key={flag}>
                  <label className="unsupported-option">
                    <span>{info.label}</span>
                    <input
                      type="text"
                      disabled
                      placeholder="Unavailable in this build"
                      aria-label={`${info.label} unavailable`}
                    />
                  </label>
                  <small className="hint">
                    Not advertised by the selected emulator build ({flag}).
                  </small>
                </li>
              ))}
          </ul>
        </details>
      )}
      {Object.keys(options).length + Object.keys(inheritedOptions).length > 0 && (
        <details className="launch-preview">
          <summary>{previewTitle}</summary>
          <p className="hint">
            Arguments are passed separately to the emulator process. The selected game path is supplied as --game.
          </p>
          <pre>
            {profileArgumentPreview(
              options,
              inheritedOptions,
              available,
              switchOptions,
            )}
          </pre>
        </details>
      )}
    </>
  );
}
