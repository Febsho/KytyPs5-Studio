// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProfileActions } from './ProfileActions';

afterEach(cleanup);
const game=(id:number,title:string,file_status:'Available'|'Missing'='Available')=>({id,title,path:`/games/${id}`,favorite:false,build_id:null,play_seconds:0,last_played:null,title_id:`PPSA${String(id).padStart(5,'0')}`,game_version:null,icon_path:null,install_size:null,file_status});

describe('ProfileActions',()=>{
  it('copies from another registered game and restores saved values when dirty',()=>{
    const onCopy=vi.fn(),onRestore=vi.fn(),onSourceChange=vi.fn();
    render(<ProfileActions games={[game(1,'One'),game(2,'Two')]} currentGameId={1} sourceGameId={null} loading={false} dirty={true} canRestore={true} onSourceChange={onSourceChange} onCopy={onCopy} onRestore={onRestore}/>);
    fireEvent.change(screen.getByRole('combobox',{name:'Copy profile from another game'}),{target:{value:'2'}});
    expect(onSourceChange).toHaveBeenCalledWith(2);
    fireEvent.click(screen.getByRole('button',{name:'Restore saved values'}));
    expect(onRestore).toHaveBeenCalledOnce();
  });
  it('does not offer the current game as a copy source and disables empty selection',()=>{
    render(<ProfileActions games={[game(1,'One')]} currentGameId={1} sourceGameId={null} loading={false} dirty={false} canRestore={false} onSourceChange={vi.fn()} onCopy={vi.fn()} onRestore={vi.fn()}/>);
    expect(screen.getByRole('combobox',{name:'Copy profile from another game'}).querySelectorAll('option')).toHaveLength(1);
    expect((screen.getByRole('button',{name:'Copy into editor'}) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole('button',{name:'Restore saved values'})).toBeNull();
  });
  it('omits removed games and generic executable entries from profile sources',()=>{
    render(<ProfileActions games={[game(1,'One'),game(2,'ASTRO BOT'),game(3,'Removed','Missing'),game(4,'app')]} currentGameId={1} sourceGameId={null} loading={false} dirty={false} canRestore={false} onSourceChange={vi.fn()} onCopy={vi.fn()} onRestore={vi.fn()}/>);
    const options = Array.from(screen.getByRole('combobox',{name:'Copy profile from another game'}).querySelectorAll('option')).map(option => option.textContent);
    expect(options).toEqual(['Choose a game', 'ASTRO BOT']);
  });
});
