// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MainNavigation } from './MainNavigation';

afterEach(cleanup);
const items=[{id:'home',label:'Home'},{id:'emulators',label:'Emulators'},{id:'forks',label:'Fork Manager'},{id:'community',label:'Community Builds'},{id:'build',label:'Build Studio'},{id:'diagnostics',label:'Diagnostics'}];

describe('MainNavigation',()=>{
  it('keeps every section available through the visible More menu',()=>{
    const onNavigate=vi.fn();
    render(<MainNavigation items={items} active="build" onNavigate={onNavigate}/>);
    const more=screen.getByText('More');
    expect(more.getAttribute('tabindex')).toBe('0');
    fireEvent.click(more);
    const build=within(screen.getByLabelText('More sections')).getByRole('button',{name:'Build Studio'});
    expect(build.getAttribute('aria-current')).toBe('page');
    fireEvent.click(build);
    expect(onNavigate).toHaveBeenCalledWith('build');
    expect(build.closest('details')?.hasAttribute('open')).toBe(false);
  });
});
