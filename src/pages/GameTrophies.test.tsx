// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GameDetailsPage } from "./GameDetailsPage";

const progress=[
  {id:"1:1000:2",title_id:"CUSA00001",build_id:1,build_name:"Build One",build_path:"/games/b1",user_id:"1000",service_label:"2",path:"/games/b1/_Trophies/CUSA00001/trophies_1000_2.json",read_status:"readable",earned:2,earned_ids:["1","2"],error:null},
  {id:"2:1001:2",title_id:"CUSA00001",build_id:2,build_name:"Build Two",build_path:"/games/b2",user_id:"1001",service_label:"2",path:"/games/b2/_Trophies/CUSA00001/trophies_1001_2.json",read_status:"unreadable",earned:null,earned_ids:[],error:"Invalid trophy JSON"},
  {id:"2:1000:3",title_id:"CUSA00001",build_id:2,build_name:"Build Two",build_path:"/games/b2",user_id:"1000",service_label:"3",path:"/games/b2/_Trophies/CUSA00001/trophies_1000_3.json",read_status:"readable",earned:1,earned_ids:["3"],error:null},
];
vi.mock("@tauri-apps/api/core",()=>({invoke:vi.fn(async(command:string)=>command==="list_trophy_progress"?progress:command==="list_saves"?[{id:"s",title_id:"CUSA00001",user_id:"1000",slot:"slot0",build_id:1,build_name:"Build One",game_version:"1.00",path:"/games/b1/_SaveData/[users]/1000/CUSA00001/slot0",size_bytes:10,modified_at:null,layout:"guest-slot:slot0"}]:[])}));
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock("@tauri-apps/plugin-dialog",()=>({confirm:vi.fn(async()=>true),open:vi.fn(),save:vi.fn()}));

afterEach(cleanup);
describe("Trophy progress panel",()=>{
 it("shows local files separately by build, account and service and keeps unreadable progress unknown",async()=>{
  render(<GameDetailsPage game={{id:42,path:"/g",title:"Example",favorite:false,build_id:1,play_seconds:0,last_played:null,title_id:"CUSA00001",game_version:"1",icon_path:null,install_size:null,file_status:"Available"}} tab="Trophies" setTab={vi.fn()} profileDirty={false} builds={[]} repos={[]} gameProfileEditor={<div/>} gameCompatibility={null} manualOutcome="" setManualOutcome={vi.fn()} manualNotes="" setManualNotes={vi.fn()} compatibilityObservations={[]} sessions={[]} onBack={vi.fn()} onLaunch={vi.fn()} onOpenGameFolder={vi.fn()} onRefreshMetadata={vi.fn()} onRemoveEntry={vi.fn()} onSetPreferredBuild={vi.fn()} onRecordObservation={vi.fn()} onOpenSessionLog={vi.fn()} onInspectContent={async()=>({trophies:[],trophy_unlock_state:"unavailable",patches:[],patch_source:null,patch_error:null})}/>);
  expect(await screen.findByText("Build One · User 1000 · Service 2")).toBeTruthy();
  expect(screen.getByText("Build Two · User 1001 · Service 2")).toBeTruthy();
  expect(screen.getByText("Build Two · User 1000 · Service 3")).toBeTruthy();
  expect(screen.getByText("Progress file could not be read; earned count unavailable")).toBeTruthy();
 });
 it("offers combined backup only for a save with a known user ID",async()=>{
  render(<GameDetailsPage game={{id:42,path:"/g",title:"Example",favorite:false,build_id:1,play_seconds:0,last_played:null,title_id:"CUSA00001",game_version:"1",icon_path:null,install_size:null,file_status:"Available"}} tab="Save Data" setTab={vi.fn()} profileDirty={false} builds={[]} repos={[]} gameProfileEditor={<div/>} gameCompatibility={null} manualOutcome="" setManualOutcome={vi.fn()} manualNotes="" setManualNotes={vi.fn()} compatibilityObservations={[]} sessions={[]} onBack={vi.fn()} onLaunch={vi.fn()} onOpenGameFolder={vi.fn()} onRefreshMetadata={vi.fn()} onRemoveEntry={vi.fn()} onSetPreferredBuild={vi.fn()} onRecordObservation={vi.fn()} onOpenSessionLog={vi.fn()} onInspectContent={async()=>({trophies:[],trophy_unlock_state:"unavailable",patches:[],patch_source:null,patch_error:null})}/>);
  const button=await screen.findByRole("button",{name:"Save + Trophies"});expect(button).toBeTruthy();expect((button as HTMLButtonElement).disabled).toBe(false);
 });
});
