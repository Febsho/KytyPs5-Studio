// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cliOptionInfo, ProfileOptionFields, profileArgumentPreview } from './ProfileOptionFields';

afterEach(cleanup);

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));

describe('ProfileOptionFields', () => {
  it('has a user-facing editor for every current upstream Linux game option', () => {
    const upstreamLinuxOptions = [
      '--game-patch', '--addcont-dir', '--screen-width', '--screen-height',
      '--user-name', '--user-id', '--mic', '--controller-color',
      '--controller-volume', '--controller-vibration', '--keyboard-player',
      '--present-mode', '--gpu', '--fullscreen', '--hdr', '--hide-cursor',
      '--no-splash', '--vr', '--amd-cpu', '--vblank-frequency',
      '--console-language', '--vulkan-validation', '--network-connected',
      '--gpu-assisted-validation', '--shader-validation', '--tessellation',
      '--shader-optimization-type', '--shader-log-direction',
      '--shader-log-folder', '--command-buffer-dump',
      '--command-buffer-dump-folder', '--graphics-debug-dump',
      '--printf-direction', '--printf-output-file', '--profile',
      '--spirv-debug-printf', '--readback-linear-images',
      '--sync-raw-image-buffers', '--trophy-notifications', '--trophy-duration',
      '--perf-overlay', '--playgo-hack', '--skip-notice-screen', '--keymap', '--rd',
    ];
    expect(Object.keys(cliOptionInfo).sort()).toEqual(upstreamLinuxOptions.sort());
  });

  it('renders supported options and explains unavailable settings as disabled', () => {
    render(<ProfileOptionFields available={['--hdr']} options={{}} onChange={vi.fn()} />);
    expect(screen.getByText('Request HDR10 output')).toBeTruthy();
    fireEvent.click(screen.getByText(/Unavailable options/));
    const unavailable = screen.getByRole('textbox', { name: 'Vulkan validation unavailable' });
    expect((unavailable as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByText(/does not advertise these options/)).toBeTruthy();
  });

  it('saves a per-game boolean override and can reset it to its inherited value', () => {
    const onChange = vi.fn();
    const { rerender } = render(<ProfileOptionFields available={['--hdr']} options={{}} inheritedOptions={{ '--hdr': 'true' }} onChange={onChange} />);
    const toggle = screen.getByRole('checkbox', { name: 'Enabled' });
    expect((toggle as HTMLInputElement).checked).toBe(true);
    fireEvent.click(toggle);
    expect(onChange).toHaveBeenCalledWith({ '--hdr': 'false' });

    onChange.mockClear();
    rerender(<ProfileOptionFields available={['--hdr']} options={{ '--hdr': 'false' }} inheritedOptions={{ '--hdr': 'true' }} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Reset override' }));
    expect(onChange).toHaveBeenCalledWith({});
  });

  it('labels clearing a global value as resetting to the emulator default', () => {
    const onChange = vi.fn();
    render(<ProfileOptionFields available={['--hdr']} options={{ '--hdr': 'true' }} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Reset to emulator default' }));
    expect(onChange).toHaveBeenCalledWith({});
  });

  it('previews repeated keymap entries as separate CLI arguments', () => {
    render(<ProfileOptionFields available={['--keymap']} options={{ '--keymap': 'Cross=K\nCircle=J' }} onChange={vi.fn()} />);
    fireEvent.click(screen.getByText('CLI argument preview'));
    expect(screen.getByText(/Arguments are passed separately/)).toBeTruthy();
    expect(profileArgumentPreview({ '--keymap': 'Cross=K\nCircle=J' }, {}, ['--keymap'])).toBe(JSON.stringify(['--keymap', 'Cross=K', '--keymap', 'Circle=J'], null, 2));
  });

  it('edits future fork options advertised by the selected executable', () => {
    const onChange = vi.fn();
    render(
      <ProfileOptionFields
        available={['--fork-mode', '--fork-trace']}
        switchOptions={['--fork-trace']}
        options={{ '--fork-mode': 'safe', '--fork-trace': 'true' }}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText('Advanced diagnostics'));
    expect(screen.getByText('--fork-mode')).toBeTruthy();
    expect(screen.getByText('--fork-trace')).toBeTruthy();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Enabled' }));
    expect(onChange).toHaveBeenCalledWith({
      '--fork-mode': 'safe',
      '--fork-trace': 'false',
    });
    expect(
      JSON.parse(
        profileArgumentPreview(
          { '--fork-mode': 'safe', '--fork-trace': 'true' },
          {},
          ['--fork-mode', '--fork-trace'],
          ['--fork-trace'],
        ),
      ),
    ).toEqual(['--fork-mode', 'safe', '--fork-trace']);
  });

  it('preserves spaces inside values and previews only supported arguments', () => {
    const preview = profileArgumentPreview(
      { '--mic': 'USB microphone', '--hdr': 'true', '--removed-fork-option': 'x' },
      { '--screen-width': '1920' },
      ['--mic', '--hdr', '--screen-width'],
    );
    expect(JSON.parse(preview)).toEqual([
      '--hdr',
      '--mic',
      'USB microphone',
      '--screen-width',
      '1920',
    ]);
  });

  it('explains how to configure Kyty microphone capture', () => {
    render(
      <ProfileOptionFields
        available={['--mic']}
        options={{}}
        onChange={vi.fn()}
      />,
    );
    expect(
      screen.getByPlaceholderText('Device name (empty disables capture)'),
    ).toBeTruthy();
    expect(
      screen.getByText(/recognized by KytyPS5's audio backend/i),
    ).toBeTruthy();
  });

  it('adds, edits and removes controller mappings in the visual editor', () => {
    const onChange = vi.fn();
    const { rerender } = render(<ProfileOptionFields available={['--keymap']} options={{ '--keymap': 'Cross=K' }} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Add mapping' }));
    expect(onChange).toHaveBeenLastCalledWith({ '--keymap': 'Cross=K\n=' });

    rerender(<ProfileOptionFields available={['--keymap']} options={{ '--keymap': 'Cross=K\nCircle=J' }} onChange={onChange} />);
    fireEvent.change(screen.getByRole('textbox', { name: 'Controller control 2' }), { target: { value: 'Confirm' } });
    expect(onChange).toHaveBeenLastCalledWith({ '--keymap': 'Cross=K\nConfirm=J' });
    fireEvent.click(screen.getByRole('button', { name: 'Remove mapping 1' }));
    expect(onChange).toHaveBeenLastCalledWith({ '--keymap': 'Circle=J' });
  });

  it('edits only reported resolution fields and preserves inherited controls', () => {
    const onChange = vi.fn();
    render(<ProfileOptionFields available={['--screen-width', '--screen-height', '--perf-overlay', '--fullscreen', '--controller-volume']} options={{}} inheritedOptions={{ '--screen-width': '1920', '--screen-height': '1080', '--perf-overlay': 'true' }} onChange={onChange} />);
    expect(screen.queryByLabelText('Resolution preset')).toBeNull();
    expect((screen.getByRole('spinbutton', { name: 'Screen width' }) as HTMLInputElement).value).toBe('1920');
    expect((screen.getByRole('spinbutton', { name: 'Screen height' }) as HTMLInputElement).value).toBe('1080');
    expect((screen.getAllByRole('checkbox', { name: 'Enabled' })[0] as HTMLInputElement).checked).toBe(true);
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Screen width' }), { target: { value: '2560' } });
    expect(onChange).toHaveBeenCalledWith({ '--screen-width': '2560' });
    expect(screen.getByText('Controls')).toBeTruthy();
  });

  it('constrains GPU auto-selection and VBlank input to KytyPS5 supported values', () => {
    const { container } = render(
      <ProfileOptionFields
        available={['--gpu', '--vblank-frequency']}
        options={{}}
        onChange={vi.fn()}
      />,
    );
    const inputs = container.querySelectorAll<HTMLInputElement>(
      'input[type="number"]',
    );
    expect(inputs[0].min).toBe('-1');
    expect(inputs[1].min).toBe('30');
    expect(inputs[1].max).toBe('360');
    expect(screen.getByText(/KytyPS5 accepts 30–360 Hz/)).toBeTruthy();
  });

  it('shows automatic GPU selection and keeps an undetected saved index visible', () => {
    const { rerender } = render(
      <ProfileOptionFields
        available={['--gpu']}
        options={{ '--gpu': '-1' }}
        gpuDevices={[{ index: 0, name: 'Example Vulkan GPU' }]}
        onChange={vi.fn()}
      />,
    );
    const gpu = screen.getByRole('combobox', { name: 'Vulkan GPU' });
    expect(gpu).toHaveProperty('value', '-1');
    expect(screen.getByRole('option', { name: 'Automatic selection' })).toBeTruthy();

    rerender(
      <ProfileOptionFields
        available={['--gpu']}
        options={{ '--gpu': '3' }}
        gpuDevices={[{ index: 0, name: 'Example Vulkan GPU' }]}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByRole('combobox', { name: 'Vulkan GPU' })).toHaveProperty('value', '3');
    expect(screen.getByRole('option', { name: '3 · Not reported by Vulkan diagnostics' })).toBeTruthy();
  });
});
