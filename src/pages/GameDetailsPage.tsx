import type { ReactNode, Dispatch, SetStateAction } from "react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { confirm as nativeConfirm, open as nativeOpen, save as nativeSave } from "@tauri-apps/plugin-dialog";
import { Play } from "lucide-react";
import type {
  Build,
  Game,
  GameContent,
  RemoteCheatFile,
  GameCompatibility,
  ManualCompatibilityObservation,
  Repo,
  Session,
  ShaderPrecompileSupport,
  ShaderPrecompileProgress,
  OfficialCompatibilityDetails,
  OfficialCompatibilityIndex,
} from "../types/studio";
import { date, emulatorBuildName, formatBytes, short } from "../utils/format";
import { trophyGrade, trophyGrades } from "../utils/trophies";
import { compatibilityOsLabel, compatibilityStatusLabel, currentCompatibilityOS, normalizeTitleId, officialCompatibilityForGame, recommendVerifiedBuild } from "../utils/compatibility";
import type { System, GraphicsDiagnostics } from "../types/studio";
import { CompatibilityReportDialog } from "../components/compatibility/CompatibilityReportDialog";
import type { CompatibilityIssueLink } from "../utils/compatibilityReport";

type Tab = "Overview" | "Settings" | "Compatibility" | "Logs" | "Trophies" | "Patches" | "Save Data";
type SaveEntry = { id:string; title_id:string; user_id:string|null; slot:string; build_id:number; build_name:string; game_version:string|null; path:string; size_bytes:number; modified_at:number|null; layout:string };
type SaveBackup = { path:string; title_id:string; user_id:string|null; source_build:string; source_build_id:number; source_fork:string; timestamp:number; checksum_sha256:string; label:string; includes_trophies:boolean; trophy_filenames:string[] };
type SaveDataProgress = { game_id:number; operation:string; phase:string; completed:number; total:number };
type TrophyProgress = { id:string; title_id:string; build_id:number; build_name:string; build_path:string; user_id:string; service_label:string; path:string; read_status:string; earned:number|null; earned_ids:string[]; error:string|null };
type TrophyBackup = {path:string;title_id:string;user_id:string;service_label:string;source_build:string;source_build_id:number;timestamp:number;sha256:string;filename:string};
export type GameDetailsPageProps = {
  game: Game;
  gameLogo?: string | null;
  gameBackground?: string | null;
  tab: Tab;
  setTab: Dispatch<SetStateAction<Tab>>;
  profileDirty: boolean;
  builds: Build[];
  repos: Repo[];
  gameProfileEditor: ReactNode;
  gameCompatibility: GameCompatibility | null;
  officialCompatibilityIndex?: OfficialCompatibilityIndex | null;
  officialCompatibilityDetails?: OfficialCompatibilityDetails | null;
  officialCompatibilityError?: string | null;
  manualOutcome: string;
  setManualOutcome: Dispatch<SetStateAction<string>>;
  manualNotes: string;
  setManualNotes: Dispatch<SetStateAction<string>>;
  compatibilityObservations: ManualCompatibilityObservation[];
  sessions: Session[];
  onBack: () => void;
  onLaunch: (gameId: number) => void;
  onLaunchBuildAB?: (gameId: number, buildId: number, settingsBuildId: number) => void | Promise<void>;
  activeSessions?: boolean;
  onAddToSteam?: (game: Game) => void;
  onOpenGameFolder: () => void;
  onRefreshMetadata: () => void;
  onRemoveEntry: () => void;
  onSetPreferredBuild: (buildId: number | null) => void;
  onActivateBuild?: (buildId: number) => void;
  onBuildRecommendedSource?: (repoId: number) => void;
  onRecordObservation: () => void;
  onOpenSessionLog: (session: Session) => void;
  onInspectContent?: (gameId:number) => Promise<GameContent>;
  onFetchOnlineCheats?: (gameId:number) => Promise<RemoteCheatFile[]>;
  shaderPrecompileSupport?: ShaderPrecompileSupport | null;
  onSetShaderPrecompileEnabled?: (enabled: boolean) => void;
  shaderPrecompileStatus?: string;
  shaderPrecompileBusy?: boolean;
  shaderPrecompileProgress?: ShaderPrecompileProgress | null;
  onPrecompileShaders?: (gameId: number) => Promise<void>;
  onStopPrecompileShaders?: () => Promise<void>;
  onDownloadOnlineCheat?: (gameId:number, filename:string) => Promise<string>;
  onImportCustomPatch?: (gameId:number) => Promise<{path:string;content:string} | null>;
  onSaveCustomPatch?: (gameId:number, content:string) => Promise<string>;
  onSetGamePatchModEnabled?: (gameId:number, modIndex:number, enabled:boolean) => Promise<{content:GameContent;patchJson:string}>;
  system?: System | null;
  graphics?: GraphicsDiagnostics | null;
  githubTokenAvailable?: boolean;
  onOpenOfficialDatabase?: () => void;
  onSearchCompatibilityIssues?: (titleId: string) => Promise<CompatibilityIssueLink[]>;
  onReadSessionLog?: (session: Session) => Promise<string>;
  onSubmitCompatibilityIssue?: (title: string, body: string) => Promise<{ number: number; url: string }>;
  onOpenExternalUrl?: (url: string) => Promise<void>;
};

export function GameDetailsPage({
  game: currentGame,
  gameLogo = null,
  gameBackground = null,
  tab: gameDetailsTab,
  setTab: setGameDetailsTab,
  profileDirty,
  builds,
  repos,
  gameProfileEditor,
  gameCompatibility,
  officialCompatibilityIndex,
  officialCompatibilityDetails,
  officialCompatibilityError,
  manualOutcome,
  setManualOutcome,
  manualNotes,
  setManualNotes,
  compatibilityObservations,
  sessions,
  onBack,
  onLaunch,
  onLaunchBuildAB,
  activeSessions = false,
  onAddToSteam,
  onOpenGameFolder,
  onRefreshMetadata,
  onRemoveEntry,
  onSetPreferredBuild,
  onActivateBuild,
  onBuildRecommendedSource,
  onRecordObservation,
  onOpenSessionLog,
  onInspectContent,
  onFetchOnlineCheats,
  shaderPrecompileSupport = null,
  onSetShaderPrecompileEnabled,
  shaderPrecompileStatus = "",
  shaderPrecompileBusy = false,
  shaderPrecompileProgress = null,
  onPrecompileShaders,
  onStopPrecompileShaders,
  onDownloadOnlineCheat,
  onImportCustomPatch,
  onSaveCustomPatch,
  onSetGamePatchModEnabled,
  system = null,
  graphics = null,
  githubTokenAvailable = false,
  onOpenOfficialDatabase,
  onSearchCompatibilityIssues,
  onReadSessionLog,
  onSubmitCompatibilityIssue,
  onOpenExternalUrl,
}: GameDetailsPageProps) {
  const [content, setContent] = useState<GameContent | null>(null);
  const [contentError, setContentError] = useState("");
  const [cheatQuery, setCheatQuery] = useState("");
  const [remoteCheats, setRemoteCheats] = useState<RemoteCheatFile[] | null>(null);
  const [remoteCheatError, setRemoteCheatError] = useState("");
  const [downloadingCheat, setDownloadingCheat] = useState("");
  const [downloadedCheatPath, setDownloadedCheatPath] = useState("");
  const [customPatchDraft, setCustomPatchDraft] = useState("");
  const [customPatchError, setCustomPatchError] = useState("");
  const [customPatchBusy, setCustomPatchBusy] = useState(false);
  const [compatibilityReportOpen, setCompatibilityReportOpen] = useState(false);
  const [patchToggleBusy, setPatchToggleBusy] = useState(false);
  const [patchToggleError, setPatchToggleError] = useState("");
  const [customPatchPath, setCustomPatchPath] = useState("");
  const [saves, setSaves] = useState<SaveEntry[]>([]);
  const [saveBusy, setSaveBusy] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [saveNotice, setSaveNotice] = useState("");
  const [backupHistory, setBackupHistory] = useState<SaveBackup[]>([]);
  const [autoBackup, setAutoBackup] = useState(false);
  const [backupDirectory, setBackupDirectory] = useState("");
  const [retentionDays, setRetentionDays] = useState("90");
  const [restorePointName, setRestorePointName] = useState("");
  const [saveProgress, setSaveProgress] = useState<SaveDataProgress | null>(null);
  const [trophyProgress, setTrophyProgress] = useState<TrophyProgress[]>([]);
  const [trophyBusy, setTrophyBusy] = useState(false);
  const [trophyError, setTrophyError] = useState("");
  const [trophyNotice, setTrophyNotice] = useState("");
  const [trophyBackups, setTrophyBackups] = useState<TrophyBackup[]>([]);
  const [abSettingsBuildId, setAbSettingsBuildId] = useState<number | null>(
    currentGame.build_id ?? builds.find((build) => build.active)?.id ?? builds[0]?.id ?? null,
  );
  const [abLaunchingBuildId, setAbLaunchingBuildId] = useState<number | null>(null);
  const gameSettingsBuild = builds.find((build) => build.id === currentGame.build_id) ??
    builds.find((build) => build.active) ?? builds[0] ?? null;
  const shaderPipelinePercent = shaderPrecompileProgress?.pipelines_total
    ? Math.min(100, Math.round(shaderPrecompileProgress.pipelines_compiled / shaderPrecompileProgress.pipelines_total * 100))
    : null;
  const shaderProgressView = shaderPrecompileProgress && (
    <div className="shader-precompile-progress" aria-live="polite">
      <div className="shader-precompile-progress-stats">
        {shaderPrecompileProgress.shaders_total !== null && <span><strong>{shaderPrecompileProgress.shaders_total.toLocaleString()}</strong> game shader programs</span>}
        <span><strong>{shaderPrecompileProgress.pipelines_compiled.toLocaleString()}{shaderPrecompileProgress.pipelines_total !== null ? ` / ${shaderPrecompileProgress.pipelines_total.toLocaleString()}` : ""}</strong> pipelines compiled</span>
        <span><strong>{shaderPrecompileProgress.live_workers.toLocaleString()}</strong> live compiler workers</span>
      </div>
      {shaderPipelinePercent !== null && <div className="shader-precompile-progress-track" role="progressbar" aria-label="Shader pipeline compilation" aria-valuemin={0} aria-valuemax={shaderPrecompileProgress.pipelines_total ?? 0} aria-valuenow={shaderPrecompileProgress.pipelines_compiled}><span style={{ width: `${shaderPipelinePercent}%` }} /></div>}
    </div>
  );
  useEffect(() => {
    setAbSettingsBuildId(
      currentGame.build_id ?? builds.find((build) => build.active)?.id ?? builds[0]?.id ?? null,
    );
  }, [currentGame.id, currentGame.build_id, builds]);
  useEffect(() => {
    setCustomPatchDraft(JSON.stringify({
      id: currentGame.title_id || "TITLE_ID_REQUIRED",
      name: currentGame.title,
      version: currentGame.game_version || "1.0.0",
      process: "eboot.bin",
      mods: [],
    }, null, 2));
    setCustomPatchError("");
    setCustomPatchPath("");
  }, [currentGame.id, currentGame.title_id, currentGame.game_version, currentGame.title]);
  useEffect(() => {
    if ((gameDetailsTab !== "Trophies" && gameDetailsTab !== "Patches") || !onInspectContent) return;
    let current = true;
    void onInspectContent(currentGame.id).then((result) => { if (current) { setContent(result); setContentError(""); } }).catch((error) => { if (current) setContentError(String(error)); });
    return () => { current = false; };
  }, [gameDetailsTab, currentGame.id, currentGame.build_id, onInspectContent]);
  const refreshTrophyProgress = async () => {
    setTrophyBusy(true); setTrophyError("");
    try { const [progress,backups]=await Promise.all([invoke<TrophyProgress[]>("list_trophy_progress", {gameId: currentGame.id}),invoke<TrophyBackup[]>("list_trophy_backups",{gameId:currentGame.id})]);setTrophyProgress(progress);setTrophyBackups(backups); }
    catch(error) { setTrophyError(String(error)); }
    finally { setTrophyBusy(false); }
  };
  useEffect(() => { if(gameDetailsTab === "Trophies") void refreshTrophyProgress(); }, [gameDetailsTab,currentGame.id]);
  const backupTrophy = async (entry:TrophyProgress) => { setTrophyBusy(true);setTrophyError("");setTrophyNotice("");try{const path=await invoke<string>("backup_trophy_progress",{gameId:currentGame.id,path:entry.path});setTrophyNotice(`Verified trophy backup created: ${path}`);}catch(error){setTrophyError(String(error));}finally{setTrophyBusy(false);void refreshTrophyProgress();} };
  useEffect(() => {
    if (gameDetailsTab !== "Patches" || !onFetchOnlineCheats) return;
    let current = true;
    setRemoteCheats(null);
    setRemoteCheatError("");
    setDownloadedCheatPath("");
    void onFetchOnlineCheats(currentGame.id)
      .then((items) => { if (current) setRemoteCheats(items); })
      .catch((error) => { if (current) setRemoteCheatError(String(error)); });
    return () => { current = false; };
  }, [gameDetailsTab, currentGame.id, onFetchOnlineCheats]);
  const detectSaves = async () => {
    setSaveBusy(true); setSaveError("");
    try { const [found,backups] = await Promise.all([invoke<SaveEntry[]>("list_saves", { gameId: currentGame.id }),invoke<SaveBackup[]>("list_save_backups", { gameId: currentGame.id })]); setSaves(found); setBackupHistory(backups); }
    catch (error) { setSaveError(String(error)); }
    finally { setSaveBusy(false); }
  };
  useEffect(() => { if (gameDetailsTab === "Save Data") void detectSaves(); }, [gameDetailsTab, currentGame.id]);
  useEffect(() => { if (gameDetailsTab !== "Save Data") return; void Promise.all([invoke<string | null>("get_setting",{key:"save_data_auto_backup"}),invoke<string | null>("get_setting",{key:"save_data_backup_directory"}),invoke<string | null>("get_setting",{key:"save_data_retention_days"})]).then(([automatic,directory,days])=>{setAutoBackup(automatic==="true");setBackupDirectory(directory??"");setRetentionDays(days??"90");}).catch((error)=>setSaveError(String(error))); },[gameDetailsTab]);
  useEffect(() => {
    if (gameDetailsTab !== "Save Data") return;
    let unlisten: (()=>void) | undefined;
    let disposed=false;
    void listen<SaveDataProgress>("save-data-progress",({payload})=>{
      if(payload.game_id===currentGame.id)setSaveProgress(payload);
    }).then((stop)=>{if(disposed)stop();else unlisten=stop;}).catch((error)=>setSaveError(String(error)));
    return ()=>{disposed=true;unlisten?.();};
  },[gameDetailsTab,currentGame.id]);
  const createSaveBackup = async (entry: SaveEntry, label?: string, includeTrophies = false) => {
    setSaveBusy(true); setSaveError(""); setSaveNotice("");
    try { const path = await invoke<string>(includeTrophies ? "backup_save_with_trophies" : "backup_save", { gameId: currentGame.id, savePath: entry.path, label: label ?? null }); setSaveNotice(`${includeTrophies ? "Combined save and trophy backup" : "Save backup"} created: ${path}`); const backups=await invoke<SaveBackup[]>("list_save_backups",{gameId:currentGame.id}); setBackupHistory(backups); }
    catch (error) { setSaveError(String(error)); }
    finally { setSaveBusy(false); }
  };
  const importCustomPatch = async () => {
    if (!onImportCustomPatch) return;
    setCustomPatchBusy(true);
    setCustomPatchError("");
    try {
      const imported = await onImportCustomPatch(currentGame.id);
      if (!imported) return;
      setCustomPatchPath(imported.path);
      setCustomPatchDraft(imported.content);
      if (onInspectContent) setContent(await onInspectContent(currentGame.id));
    } catch (error) {
      setCustomPatchError(String(error));
    } finally {
      setCustomPatchBusy(false);
    }
  };
  const saveCustomPatch = async () => {
    if (!onSaveCustomPatch) return;
    setCustomPatchBusy(true);
    setCustomPatchError("");
    try {
      const path = await onSaveCustomPatch(currentGame.id, customPatchDraft);
      setCustomPatchPath(path);
      if (onInspectContent) setContent(await onInspectContent(currentGame.id));
    } catch (error) {
      setCustomPatchError(String(error));
    } finally {
      setCustomPatchBusy(false);
    }
  };
  const setPatchEnabled = async (modIndex: number, enabled: boolean) => {
    if (!onSetGamePatchModEnabled) return;
    setPatchToggleBusy(true);
    setPatchToggleError("");
    try {
      const updated = await onSetGamePatchModEnabled(currentGame.id, modIndex, enabled);
      setContent(updated.content);
      setCustomPatchDraft(updated.patchJson);
    } catch (error) {
      setPatchToggleError(String(error));
    } finally {
      setPatchToggleBusy(false);
    }
  };
  const recommendation = recommendVerifiedBuild(
    currentGame,
    officialCompatibilityDetails ?? null,
    builds,
    repos,
  );
  const hostOS = currentCompatibilityOS();
  const exactHostReports = officialCompatibilityDetails?.reports.filter(
    (report) => normalizeTitleId(report.titleId) === normalizeTitleId(currentGame.title_id) && report.os.trim().toLowerCase() === hostOS,
  ) ?? [];
  return (
    <section className="page game-details-page">
      <div className="game-details-header">
        {gameBackground && <img className="game-details-background" src={gameBackground} alt="" />}
        <button className="outline" onClick={onBack}>
          ← Library
        </button>
        <div className="game-details-title">
          <p className="eyebrow">GAME DETAILS</p>
          {gameLogo ? (
            <>
              <h1 className="sr-only">{currentGame.title}</h1>
              <img className="game-details-logo" src={gameLogo} alt="" />
            </>
          ) : (
            <h1>{currentGame.title}</h1>
          )}
          <p className="path">{currentGame.path}</p>
        </div>
        <div className="buttonrow">
          <button
            className="outline"
            onClick={() => setGameDetailsTab("Settings")}
          >
            Settings
          </button>
          {onAddToSteam && (
            <button className="outline" onClick={() => onAddToSteam(currentGame)}>
              Add to Steam
            </button>
          )}
          <button
            className="primary"
            disabled={currentGame.file_status === "Missing"}
            title={
              currentGame.file_status === "Missing"
                ? "Registered game path is missing"
                : ""
            }
            onClick={() => onLaunch(currentGame.id)}
          >
            <Play size={17} /> Play game
          </button>
        </div>
      </div>
      <nav className="details-tabs" aria-label="Game details tabs">
        {(["Overview", "Settings", "Compatibility", "Logs", "Trophies", "Patches", "Save Data"] as const).map(
          (tab) => (
            <button
              key={tab}
              className={gameDetailsTab === tab ? "active" : ""}
              onClick={() => setGameDetailsTab(tab)}
            >
              {tab}
            </button>
          ),
        )}
      </nav>
      {gameDetailsTab === "Save Data" && <div className="panel save-data-panel">
        <div className="game-overview-heading"><div><span className="eyebrow">PER-GAME STORAGE</span><h2>Save Data</h2><p>On launch, Studio automatically uses the save from the build you played most recently. A different save already in the selected build is backed up before replacement. ZIP export and import remain available for manual transfers; Studio app updates keep your saves.</p></div><button className="outline" disabled={saveBusy || activeSessions} onClick={() => void detectSaves()}>{saveBusy ? "Working…" : "Detect saves"}</button></div>
        {activeSessions && <p className="hint">Close all emulator sessions before changing save data.</p>}
        <div className="save-data-settings"><label className="save-auto-backup"><input type="checkbox" checked={autoBackup} onChange={async(e)=>{const enabled=e.currentTarget.checked;setAutoBackup(enabled);try{await invoke("set_setting",{key:"save_data_auto_backup",value:String(enabled)});}catch(error){setAutoBackup(!enabled);setSaveError(String(error));}}}/> Create deduplicated backups after each completed game session</label><div className="buttonrow"><span className="path">{backupDirectory || "Studio app data / save-backups"}</span><button className="outline" disabled={saveBusy} onClick={async()=>{const selected=await nativeOpen({directory:true,multiple:false,title:"Choose save backup folder"});if(typeof selected!=="string")return;try{const path=await invoke<string>("set_save_backup_directory",{path:selected});setBackupDirectory(path);setSaveNotice(`Backup destination changed to ${path}`);}catch(error){setSaveError(String(error));}}}>Choose backup folder</button></div><div className="buttonrow"><label>Keep backups for <input aria-label="Backup retention days" type="number" min={1} max={3650} value={retentionDays} onChange={(e)=>setRetentionDays(e.currentTarget.value)} onBlur={()=>{const days=Math.max(1,Math.min(3650,Number(retentionDays)||90));setRetentionDays(String(days));void invoke("set_setting",{key:"save_data_retention_days",value:String(days)}).catch((error)=>setSaveError(String(error)));}}/> days</label><button className="outline" disabled={saveBusy || activeSessions} onClick={async()=>{const days=Math.max(1,Math.min(3650,Number(retentionDays)||90));if(!await nativeConfirm(`Remove verified backups older than ${days} days? The newest verified backup for each build, user and slot will be kept.`,{title:"Clean save backup history",kind:"warning",okLabel:"Clean old backups"}))return;setSaveBusy(true);try{const bytes=await invoke<number>("clean_save_backups",{gameId:currentGame.id,retentionDays:days});setSaveNotice(`Removed ${formatBytes(bytes)} of old verified backups.`);await detectSaves();}catch(error){setSaveError(String(error));}finally{setSaveBusy(false);}}}>Clean old backups…</button></div><label className="save-point-name">Restore point name <input value={restorePointName} maxLength={64} placeholder="e.g. Before boss fight" onChange={(e)=>setRestorePointName(e.currentTarget.value)}/></label></div>
        {saveError && <p className="error-text">{saveError}</p>}{saveNotice && <p className="hint">{saveNotice}</p>}
        {saveBusy && saveProgress && <div className="save-progress" aria-live="polite"><div className="save-progress-heading"><strong>{saveProgress.operation.replaceAll("-"," ")}</strong><span>{saveProgress.phase}</span></div>{saveProgress.total>1 && <div className="shader-precompile-progress-track" role="progressbar" aria-valuemin={0} aria-valuemax={saveProgress.total} aria-valuenow={saveProgress.completed}><span style={{width:`${Math.min(100,Math.round(saveProgress.completed/saveProgress.total*100))}%`}}/></div>}</div>}
        <div className="buttonrow"><button className="outline" disabled={saveBusy || activeSessions || !builds.length} onClick={async () => { const selected = await nativeOpen({ directory:true, multiple:false, title:"Select a save folder named for this game's title ID" }); if (typeof selected !== "string") return; if(!await nativeConfirm("Import this title save root? Detected matching saves are backed up before replacement.",{title:"Import save folder",kind:"warning",okLabel:"Import save"}))return; const buildId=currentGame.build_id ?? builds.find((b)=>b.active)?.id ?? builds[0]?.id; if (buildId === undefined) return; setSaveBusy(true); setSaveError(""); try { const path=await invoke<string>("import_save_directory",{gameId:currentGame.id,sourcePath:selected,buildId}); setSaveNotice(`Imported to ${path}`); await detectSaves(); } catch(e) { setSaveError(String(e)); } finally { setSaveBusy(false); } }}>Import save folder</button><button className="outline" disabled={saveBusy || activeSessions || !builds.length} onClick={async()=>{const archive=await nativeOpen({multiple:false,filters:[{name:"ZIP archive",extensions:["zip"]}]});if(typeof archive!=="string")return;if(!await nativeConfirm("Import this verified save slot into the selected build? An existing matching slot is backed up first.",{title:"Import save ZIP",kind:"warning",okLabel:"Import save"}))return;const buildId=currentGame.build_id??builds.find((b)=>b.active)?.id??builds[0]?.id;if(buildId===undefined)return;setSaveBusy(true);try{const path=await invoke<string>("import_save_archive",{gameId:currentGame.id,archivePath:archive,buildId});setSaveNotice(`Imported to ${path}`);await detectSaves();}catch(e){setSaveError(String(e));}finally{setSaveBusy(false);}}}>Import ZIP</button></div>
        {saves.length === 0 && <p>{saveBusy ? "Scanning installed builds…" : "No saves were detected in the verified layouts."}</p>}
        {saves.map((entry) => <section className="save-data-entry" key={entry.id}><div><strong>{entry.slot}</strong><p>{entry.title_id} · User {entry.user_id ?? "default/unknown"} · {entry.build_name}</p><p>{entry.path}</p><small>{formatBytes(entry.size_bytes)} · {entry.modified_at ? date(entry.modified_at) : "Modified time unavailable"} · {entry.layout}{entry.game_version ? ` · game ${entry.game_version}` : ""}</small></div><div className="buttonrow"><button className="outline" disabled={saveBusy || activeSessions} onClick={() => void createSaveBackup(entry,restorePointName.trim()||"manual")}>Create save backup</button><button className="outline" disabled={saveBusy || activeSessions || !entry.user_id} title={!entry.user_id?"Combined backups require a known save user ID":"Includes trophy files for the same user, build and title"} onClick={() => void createSaveBackup(entry,restorePointName.trim()||"manual",true)}>Save + Trophies</button><button className="outline" disabled={saveBusy} onClick={async()=>{const destination=await nativeSave({defaultPath:`${entry.title_id}-${entry.slot}.zip`,filters:[{name:"ZIP archive",extensions:["zip"]}]});if(!destination)return;setSaveBusy(true);try{const path=await invoke<string>("export_save_archive",{gameId:currentGame.id,savePath:entry.path,destination});setSaveNotice(`Exported ${path}`);}catch(e){setSaveError(String(e));}finally{setSaveBusy(false);}}}>Export ZIP</button><button className="outline" onClick={() => void invoke("open_save_folder",{path:entry.path}).catch((e)=>setSaveError(String(e)))}>Folder</button><button className="outline" disabled={saveBusy || activeSessions} onClick={async()=>{ if (!await nativeConfirm("Delete this save? Studio creates a verified backup first.", {title:"Delete save data",kind:"warning",okLabel:"Delete save"})) return; setSaveBusy(true); try { await invoke("delete_save",{gameId:currentGame.id,savePath:entry.path}); await detectSaves(); } catch(e) { setSaveError(String(e)); } finally { setSaveBusy(false); } }}>Delete…</button></div></section>)}
        {backupHistory.length > 0 && <div><h3>Backup history</h3>{backupHistory.map((record)=><section className="save-data-entry" key={record.path}><div><strong>{record.label} · {date(record.timestamp)} · {record.source_fork} / {record.source_build} · user {record.user_id ?? "default/unknown"}</strong><p>{record.path}</p><small>SHA-256 {record.checksum_sha256} · {record.includes_trophies?`includes ${record.trophy_filenames.join(", ")}`:"save data only"}</small></div><button className="outline" disabled={saveBusy || activeSessions} onClick={async()=>{const details=record.includes_trophies?` It will also restore these local trophy files for user ${record.user_id}: ${record.trophy_filenames.join(", ")}.`:"";if(!await nativeConfirm(`Restore this verified backup to its original build and save layout? Existing data is backed up first.${details}`,{title:"Restore save backup",kind:"warning",okLabel:"Restore backup"}))return;setSaveBusy(true);try{await invoke("restore_save",{gameId:currentGame.id,backupPath:record.path});setSaveNotice("Backup restored and verified.");await detectSaves();}catch(e){setSaveError(String(e));}finally{setSaveBusy(false);}}}>Restore</button></section>)}</div>}
        <p className="hint">Backups are stored at {backupDirectory || "Studio app data / save-backups"}, outside emulator build folders. Only checksum-verified backups are shown. Use “Save + Trophies” to include the matching user’s trophy JSON files for that same build; ordinary save backups exclude trophies. Cleanup keeps the last verified restore point per build, user and slot.</p>
      </div>}
      {gameDetailsTab === "Overview" && (
        <div className="details-overview">
          <div className="panel game-overview-panel">
            <div className="game-overview-heading">
              <div>
                <span className="eyebrow">LIBRARY DETAILS</span>
                <h2>Game overview</h2>
                <p>Installation, playtime and launch settings for this game.</p>
              </div>
              {gameSettingsBuild && <span className="overview-build-chip">{emulatorBuildName(repos.find((repo) => repo.id === gameSettingsBuild.repo_id), gameSettingsBuild)}</span>}
            </div>
            <div className="game-meta">
              {currentGame.title_id && <span>{currentGame.title_id}</span>}
              {currentGame.game_version && (
                <span>Version {currentGame.game_version}</span>
              )}
              {currentGame.install_size !== null && (
                <span>{formatBytes(currentGame.install_size)}</span>
              )}
              <span>
                {currentGame.file_status === "Missing"
                  ? "Files missing"
                  : "Files available"}
              </span>
              <span>
                {Math.round(currentGame.play_seconds / 60)} min played
              </span>
            </div>
            <p>
              Game files remain at their registered location. Studio does not
              copy or modify the original game data.
            </p>
            {onPrecompileShaders && (
              <div className="shader-precompile-panel">
                <div>
                  <strong>Precompile shaders for this game</strong>
                  <p>
                    {shaderPrecompileSupport?.full_precompile
                      ? "Run the selected emulator build’s shader tool now. Completed shards are kept if you stop; run it again later to continue the missing work."
                      : shaderPrecompileSupport?.background_precompile
                        ? "This fork records shaders while you play and compiles known shaders in the background on later launches. The first playthrough discovers them."
                      : shaderPrecompileSupport
                        ? "The selected emulator build does not include a full game shader precompile tool. Choose a build with the tool installed to enable this action."
                        : gameSettingsBuild
                          ? "Checking whether the selected emulator build includes a shader precompile tool…"
                          : "Install an emulator build to check shader precompile support."}
                  </p>
                  {gameSettingsBuild && <small>Build: {emulatorBuildName(repos.find((repo) => repo.id === gameSettingsBuild.repo_id), gameSettingsBuild)}</small>}
                  {shaderPrecompileSupport?.cache_directory && <small>Cache: {shaderPrecompileSupport.cache_directory}</small>}
                  {shaderPrecompileStatus && <small role="status">{shaderPrecompileStatus}</small>}
                  {shaderProgressView}
                </div>
                {shaderPrecompileBusy ? (
                  <button className="outline danger" onClick={() => void onStopPrecompileShaders?.()}>Stop precompile</button>
                ) : shaderPrecompileSupport?.background_precompile && !shaderPrecompileSupport.full_precompile ? (
                  <label className="shader-precompile-game-toggle">
                    <input
                      type="checkbox"
                      checked={shaderPrecompileSupport.background_enabled}
                      disabled={currentGame.file_status === "Missing" || activeSessions}
                      onChange={(event) => onSetShaderPrecompileEnabled?.(event.target.checked)}
                    />
                    Precompile known shaders on this game’s launches
                  </label>
                ) : (
                  <button
                    className="outline"
                    disabled={currentGame.file_status === "Missing" || activeSessions || !shaderPrecompileSupport?.full_precompile}
                    onClick={() => void onPrecompileShaders?.(currentGame.id)}
                  >
                    {shaderPrecompileSupport?.full_precompile ? "Precompile now" : "Precompile unavailable"}
                  </button>
                )}
              </div>
            )}
            <div className="buttonrow game-overview-actions">
              <button
                className="outline"
                disabled={currentGame.file_status === "Missing"}
                onClick={onOpenGameFolder}
              >
                Open game folder
              </button>
              <button
                className="outline"
                disabled={currentGame.file_status === "Missing"}
                onClick={onRefreshMetadata}
              >
                Refresh metadata
              </button>
              <button
                className="outline danger"
                onClick={onRemoveEntry}
              >
                Remove library entry
              </button>
              <label>
                Preferred emulator build
                <select
                  value={currentGame.build_id ?? ""}
                  onChange={(event) => {
                    if (
                      profileDirty &&
                      !window.confirm(
                        "This game profile has unsaved changes. Discard them and switch emulator builds?",
                      )
                    )
                      return;
                    onSetPreferredBuild(
                      event.target.value ? Number(event.target.value) : null,
                    );
                  }}
                >
                  <option value="">Use active build</option>
                  {builds.map((b) => (
                    <option value={b.id} key={b.id}>
                      {emulatorBuildName(repos.find((r) => r.id === b.repo_id), b)}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </div>
        </div>
      )}
      {gameDetailsTab === "Settings" && (
        <div className="game-profile-workspace">
          <div className="game-settings-heading">
            <div>
              <h2>Game settings</h2>
              <p>Choose this game’s emulator, then override its launch settings. Empty fields inherit that build’s global defaults.</p>
            </div>
            <label>
              Preferred emulator
              <select
                aria-label="Preferred emulator build"
                value={currentGame.build_id ?? ""}
                onChange={(event) => {
                  if (profileDirty && !window.confirm("This game profile has unsaved changes. Discard them and switch emulator builds?")) return;
                  onSetPreferredBuild(event.target.value ? Number(event.target.value) : null);
                }}
              >
                <option value="">{gameSettingsBuild ? `Use active build · ${emulatorBuildName(repos.find((repo) => repo.id === gameSettingsBuild.repo_id), gameSettingsBuild)}` : "Use active build"}</option>
                {builds.map((build) => (
                  <option value={build.id} key={build.id}>
                    {emulatorBuildName(repos.find((repo) => repo.id === build.repo_id), build)}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {onPrecompileShaders && (
            <section className="game-shader-precompile-card" aria-label="Game shader precompile">
              <div>
                <span className="eyebrow">PER-GAME ACTION</span>
                <h3>Precompile shaders</h3>
                <p>
                  {shaderPrecompileSupport?.full_precompile
                    ? "Build the shader cache for this game with the selected emulator fork. Completed shards are kept if you stop; run it again later to continue the missing work."
                    : shaderPrecompileSupport?.background_precompile
                      ? "This fork records shaders during play, then compiles those known shaders in the background on later launches."
                    : shaderPrecompileSupport
                      ? "The selected fork does not provide a full game shader precompile tool."
                      : "Checking precompile support for the selected emulator…"}
                </p>
                {shaderPrecompileStatus && <small role="status">{shaderPrecompileStatus}</small>}
                {shaderProgressView}
              </div>
              {shaderPrecompileBusy ? (
                <button className="outline danger" onClick={() => void onStopPrecompileShaders?.()}>Stop precompile</button>
              ) : shaderPrecompileSupport?.background_precompile && !shaderPrecompileSupport.full_precompile ? (
                <label className="shader-precompile-game-toggle">
                  <input
                    type="checkbox"
                    checked={shaderPrecompileSupport.background_enabled}
                    disabled={currentGame.file_status === "Missing" || activeSessions}
                    onChange={(event) => onSetShaderPrecompileEnabled?.(event.target.checked)}
                  />
                  Enable for this game
                </label>
              ) : (
                <button
                  className="primary"
                  disabled={currentGame.file_status === "Missing" || activeSessions || !shaderPrecompileSupport?.full_precompile}
                  onClick={() => void onPrecompileShaders?.(currentGame.id)}
                >
                  Precompile for this game
                </button>
              )}
            </section>
          )}
          {gameProfileEditor}
        </div>
      )}
      {(gameDetailsTab === "Trophies" || gameDetailsTab === "Patches") && <div className="panel game-content-panel">
        {gameDetailsTab === "Trophies" ? <>
          <h2>Trophies</h2>
          <p>Definitions come from the game package. Earned status is local emulator progress only; there is no PlayStation Network sync.</p>
          {trophyError && <p className="error-note">{trophyError}</p>}{trophyNotice && <p className="hint">{trophyNotice}</p>}
          <section className="trophy-progress-manager"><div className="buttonrow"><h3>Local progress by build, user and service</h3><button className="outline" disabled={trophyBusy} onClick={()=>void refreshTrophyProgress()}>{trophyBusy?"Reading…":"Refresh"}</button></div>
            {trophyProgress.length===0&&!trophyBusy&&<p>No local trophy progress files were found in installed build folders. Missing files do not mean zero trophies earned.</p>}
            {trophyProgress.map((entry)=><article className="save-data-entry" key={entry.id}><div><strong>{entry.build_name} · User {entry.user_id} · Service {entry.service_label}</strong><p>{entry.read_status==="readable"?`${entry.earned} earned IDs read`:"Progress file could not be read; earned count unavailable"}</p><small>{entry.path}</small>{entry.error&&<p className="error-note">{entry.error}</p>}</div><div className="buttonrow"><button className="outline" disabled={trophyBusy||activeSessions||entry.read_status!=="readable"} onClick={()=>void backupTrophy(entry)}>Backup Trophies</button><button className="outline" disabled={trophyBusy||activeSessions||entry.read_status!=="readable"} onClick={async()=>{const destination=await nativeSave({defaultPath:`trophies_${entry.user_id}_${entry.service_label}.json`,filters:[{name:"Trophy JSON",extensions:["json"]}]});if(!destination)return;setTrophyBusy(true);try{const path=await invoke<string>("export_trophy_progress",{gameId:currentGame.id,path:entry.path,destination});setTrophyNotice(`Exported ${path}`);}catch(error){setTrophyError(String(error));}finally{setTrophyBusy(false);}}}>Export</button><button className="outline" disabled={trophyBusy||activeSessions} onClick={()=>void invoke("open_trophy_folder",{gameId:currentGame.id,path:entry.path}).catch(error=>setTrophyError(String(error)))}>Open Trophy Folder</button></div></article>)}
            <button className="outline" disabled={trophyBusy||activeSessions} onClick={async()=>{const source=await nativeOpen({multiple:false,filters:[{name:"Trophy JSON",extensions:["json"]}]});if(typeof source!=="string")return;const target=builds.find(b=>b.id===currentGame.build_id)?.id??builds.find(b=>b.active)?.id;if(!target){setTrophyError("No registered build is available as an import target.");return;}if(!await nativeConfirm("Import this exact user and service file? Existing trophy progress will never be overwritten.",{title:"Import trophy progress",kind:"warning",okLabel:"Import JSON"}))return;setTrophyBusy(true);try{const path=await invoke<string>("import_trophy_progress",{gameId:currentGame.id,source,buildId:target});setTrophyNotice(`Imported ${path}`);await refreshTrophyProgress();}catch(error){setTrophyError(String(error));}finally{setTrophyBusy(false);}}}>Import Trophy JSON…</button>
          </section>
          <section className="trophy-progress-manager"><h3>Verified trophy backup history</h3>{trophyBackups.length===0?<p>No verified trophy backups yet.</p>:trophyBackups.map(record=><article className="save-data-entry" key={record.path}><div><strong>{record.source_build} · User {record.user_id} · Service {record.service_label}</strong><p>{date(record.timestamp)} · SHA-256 {record.sha256}</p><small>{record.path}</small></div><button className="outline" disabled={trophyBusy||activeSessions} onClick={async()=>{if(!await nativeConfirm(`Restore ${record.filename} for user ${record.user_id}, service ${record.service_label}, to build ${record.source_build}? Existing progress for this exact account and service is backed up first.`,{title:"Restore trophy progress",kind:"warning",okLabel:"Restore progress"}))return;setTrophyBusy(true);try{const path=await invoke<string>("restore_trophy_backup",{gameId:currentGame.id,backupPath:record.path});setTrophyNotice(`Restored ${path}`);await refreshTrophyProgress();}catch(error){setTrophyError(String(error));}finally{setTrophyBusy(false);}}}>Restore…</button></article>)}</section>
          {contentError && <p className="error-note">{contentError}</p>}
          {!content && !contentError && <p>Reading trophy package…</p>}
          {content?.trophies.length ? <>
            <div className="trophy-counts" aria-label="Trophy counts">
              {content.trophy_unlock_state === "available" && <span className="trophy-count trophy-earned"><strong>{content.trophies.filter((t) => t.unlocked === true).length}/{content.trophies.length}</strong>earned</span>}
              {content.trophy_unlock_state !== "available" && <span className="trophy-count trophy-other">Earned progress unavailable for this game/build</span>}
              {trophyGrades.map((grade) => <span className={`trophy-count trophy-${grade}`} key={grade}><strong>{content.trophies.filter((t) => trophyGrade(t.grade) === grade).length}</strong>{grade}</span>)}
              {content.trophies.some((t) => trophyGrade(t.grade) === "other") && <span className="trophy-count trophy-other"><strong>{content.trophies.filter((t) => trophyGrade(t.grade) === "other").length}</strong>other</span>}
            </div>
            {([...trophyGrades, "other"] as const).map((grade) => {
              const trophies = content.trophies.filter((t) => trophyGrade(t.grade) === grade);
              if (!trophies.length) return null;
              return <section className={`trophy-section trophy-${grade}`} key={grade}>
                <h3><span className="trophy-grade-icon" aria-hidden="true">{grade === "platinum" ? "✦" : "●"}</span>{grade === "other" ? "Other trophies" : `${grade[0].toUpperCase()}${grade.slice(1)} trophies`}<small>{trophies.length}</small></h3>
                <div className="trophy-grid">{trophies.map((t) => <article className={`trophy-item trophy-${grade}`} key={t.id}>
                  <span className="trophy-item-top"><span className="trophy-grade">{grade === "other" ? (t.grade || "Trophy") : grade}</span><span className={`trophy-earned-state ${t.unlocked === true ? "earned" : ""}`}>{t.unlocked === true ? "✓ Earned" : t.unlocked === false ? "Not earned" : "Status unavailable"}</span></span>
                  <strong>{t.name || (t.hidden ? "Hidden trophy" : `Trophy ${t.id}`)}</strong>
                  <small>{t.detail || (t.hidden ? "This trophy is hidden." : `Trophy ${t.id}`)}</small>
                </article>)}</div>
              </section>;
            })}
          </> : content && <p>No readable trophy definitions were found in the game's <code>sce_sys/trophy2</code> or <code>sce_sys/trophy</code> packages.</p>}
        </> : <>
          <h2>Patches</h2>
          <p>Import the patch matching this game version. Patches are installed in the selected emulator build's <code>_Patches</code> folder and applied on the next game launch.</p>
          <label className="cheat-search">Search patches<input value={cheatQuery} onChange={(e)=>setCheatQuery(e.target.value)} placeholder="Search by patch name" /></label>
          {contentError && <p className="error-note">{contentError}</p>}
          {content?.patch_error && <p className="error-note">{content.patch_error}</p>}
          {content?.patch_source && <p className="hint">Source: {content.patch_source}</p>}
          {patchToggleError && <p className="error-note">{patchToggleError}</p>}
          {content?.patches.map((patch, index) => {
            if (!patch.name.toLowerCase().includes(cheatQuery.trim().toLowerCase())) return null;
            return <label className="game-patch-toggle" key={`${patch.name}-${index}`}>
              <input type="checkbox" checked={patch.enabled} disabled={!onSetGamePatchModEnabled || patchToggleBusy} onChange={(event) => void setPatchEnabled(index, event.target.checked)} />
              <span><strong>{patch.name}</strong><small>{patch.enabled ? "On" : "Off"} · {patch.title_id || "No title ID"} · v{patch.version || "?"}</small></span>
            </label>;
          })}
          {content && !content.patches.length && !content.patch_error && <p>No patch file is installed for this game and emulator build yet.</p>}
          <section className="custom-patch-editor">
            <div className="buttonrow">
              <h3>Custom game patch</h3>
              <button className="outline" disabled={!onImportCustomPatch || customPatchBusy} onClick={() => void importCustomPatch()}>
              {customPatchBusy ? "Working…" : "Import patch JSON"}
              </button>
            </div>
            <p className="hint">
              Import validates the game ID and version, then installs the file as <code>_Patches/{currentGame.title_id || "TITLE_ID"}.json</code> beside the selected emulator. The JSON editor can create or edit this game's patch file.
            </p>
            <label className="cheat-search">
              Patch JSON
              <textarea className="patch-json-editor" rows={12} spellCheck={false} value={customPatchDraft} onChange={(event) => setCustomPatchDraft(event.target.value)} />
            </label>
            <div className="buttonrow">
              <button className="outline" disabled={!onSaveCustomPatch || customPatchBusy || !currentGame.title_id} onClick={() => void saveCustomPatch()}>
                {customPatchBusy ? "Saving…" : "Save custom patch"}
              </button>
              {!currentGame.title_id && <small className="hint">Refresh game metadata first to detect its title ID.</small>}
            </div>
            {customPatchError && <p className="error-note">{customPatchError}</p>}
            {customPatchPath && <p className="hint">Patch installed for this game and emulator: <code>{customPatchPath}</code>. It will be passed to KytyPS5 on the next launch.</p>}
          </section>
          <h3>Online PS5 patch repository</h3>
          <p className="hint">Source: TeeKay87/HEN-Cheats-Collection · files are matched by title ID and game version.</p>
          {remoteCheatError && <p className="error-note">{remoteCheatError}</p>}
          {!remoteCheats && !remoteCheatError && <p>Searching repository…</p>}
          {!currentGame.title_id && <p className="hint">No game title ID is available. Refresh game metadata to search for matching patches.</p>}
          {remoteCheats?.length === 0 && currentGame.title_id && <p>No matching PS5 patch file was found for title ID {currentGame.title_id}.</p>}
          {remoteCheats?.map((file) => {
            const matchingMods = file.mods.filter((mod) => mod.name.toLowerCase().includes(cheatQuery.trim().toLowerCase()));
            if (cheatQuery.trim() && !matchingMods.length) return null;
            const versionMismatch = Boolean(file.version && currentGame.game_version && file.version !== currentGame.game_version);
            return <article className="remote-cheat-file" key={file.filename}>
              <div className="remote-cheat-heading"><div><strong>{file.title}</strong><small>{file.filename} · version {file.version || "unknown"} · {file.mods.length} patches</small></div><button className="outline" disabled={!onDownloadOnlineCheat || downloadingCheat === file.filename || versionMismatch} onClick={() => {
                if (!onDownloadOnlineCheat) return;
                setDownloadingCheat(file.filename);
                void onDownloadOnlineCheat(currentGame.id, file.filename).then(async (path) => {
                  setDownloadedCheatPath(path);
                  setCustomPatchPath(path);
                  if (onInspectContent) {
                    try { setContent(await onInspectContent(currentGame.id)); setContentError(""); }
                    catch (error) { setContentError(String(error)); }
                  }
                }).catch((error) => setRemoteCheatError(String(error))).finally(() => setDownloadingCheat(""));
              }}>{downloadingCheat === file.filename ? "Installing…" : "Install patch"}</button></div>
              {versionMismatch && <small className="error-note">This patch is for version {file.version}; this game is {currentGame.game_version}. Choose the matching version.</small>}
              {matchingMods.map((mod, index) => <div className="kv cheat-row" key={`${mod.name}-${index}`}><strong>{mod.name}</strong><span>{mod.type}</span></div>)}
              <a href={file.source} target="_blank" rel="noreferrer">View source file</a>
            </article>;
          })}
          {downloadedCheatPath && <p className="hint">Patch installed for this game: <code>{downloadedCheatPath}</code>. It will be applied on the next launch.</p>}
        </>}
      </div>}
      {gameDetailsTab === "Compatibility" && (
        <div className="panel compatibility-panel">
          <section className="build-comparison-2">
            <div className="sectionhead">
              <div>
                <span className="eyebrow">LOCAL SESSION EVIDENCE</span>
                <h2>Build comparison</h2>
                <p>Compare observed results for this game across installed builds. No phase or performance value is inferred.</p>
              </div>
            </div>
            {builds.length > 1 ? (
              <>
                <div className="ab-test-controls">
                  <label>
                    Shared settings from
                    <select
                      aria-label="A/B test shared settings build"
                      value={abSettingsBuildId ?? ""}
                      onChange={(event) => setAbSettingsBuildId(event.target.value ? Number(event.target.value) : null)}
                    >
                      {builds.map((build) => {
                        const repo = repos.find((item) => item.id === build.repo_id);
                        return <option key={build.id} value={build.id}>
                          {emulatorBuildName(repo, build)}
                        </option>;
                      })}
                    </select>
                  </label>
                  <p className="hint">Each start uses this build’s saved defaults plus the game profile. Studio stops the test if the target build cannot accept the same options. Close the current session before starting the next run.</p>
                </div>
                <div className="comparison-table-wrap">
                  <table className="comparison-table build-results-table">
                    <thead><tr><th>Build</th><th>Attempts</th><th>Process started</th><th>Crashes</th><th>Non-zero exits</th><th>Reached phase</th><th>FPS</th><th>Frame time</th><th>A/B test</th></tr></thead>
                    <tbody>
                      {builds.map((build) => {
                        const repo = repos.find((item) => item.id === build.repo_id);
                        const runs = sessions.filter((session) => session.game_id === currentGame.id && session.build_id === build.id);
                        const crashes = runs.filter((session) => session.outcome === "crash").length;
                        const nonZeroExits = runs.filter((session) => session.outcome === "error" || (session.exit_code !== null && session.exit_code !== 0)).length;
                        const observation = compatibilityObservations.find((item) => item.build_id === build.id);
                        const phase = observation?.outcome.replaceAll("_", " ") || "Not recorded";
                        return (
                          <tr key={build.id}>
                            <th scope="row">
                              <strong>{emulatorBuildName(repo, build)}</strong>
                              <small>{build.build_type}{build.active ? " · Active" : ""}</small>
                            </th>
                            <td>{runs.length}</td>
                            <td>{runs.filter((session) => session.pid !== null).length} / {runs.length || 0}</td>
                            <td>{crashes}</td>
                            <td>{nonZeroExits}</td>
                            <td>{phase}</td>
                            <td>Not collected</td>
                            <td>Not collected</td>
                            <td>
                              <button
                                className="outline"
                                disabled={activeSessions || abLaunchingBuildId !== null || !onLaunchBuildAB || abSettingsBuildId === null || !build.executable_status.startsWith("Executable")}
                                title={activeSessions ? "Close the current emulator session first" : "Run this build using the shared settings snapshot"}
                                onClick={() => {
                                  if (!onLaunchBuildAB || abSettingsBuildId === null) return;
                                  setAbLaunchingBuildId(build.id);
                                  void Promise.resolve(onLaunchBuildAB(currentGame.id, build.id, abSettingsBuildId))
                                    .finally(() => setAbLaunchingBuildId(null));
                                }}
                              >
                                {abLaunchingBuildId === build.id ? "Starting…" : "Run this build"}
                              </button>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
                <small className="hint">FPS and frame times are unavailable because the current emulator telemetry does not report them. Phase values come only from saved manual observations.</small>
              </>
            ) : (
              <p className="hint">Install at least two emulator builds to compare or run an A/B test.</p>
            )}
          </section>
          <section className="official-compatibility-section">
            <div className="official-compatibility-heading">
              <div>
                <span className="eyebrow">OFFICIAL KYTYPS5 DATABASE</span>
                <h2>Community compatibility</h2>
              </div>
              {officialCompatibilityIndex?.updated_at && <small>Cached {date(officialCompatibilityIndex.updated_at)}</small>}
            </div>
            {!currentGame.title_id && <p className="hint">A PS5 title ID (PPSA) is required to look up this game.</p>}
            {currentGame.title_id && !officialCompatibilityIndex?.available && <p className="hint">{officialCompatibilityIndex?.error || "Compatibility data is unavailable."} No status has been inferred.</p>}
            <section className="smart-advisor" aria-label="Smart fork and build recommendation">
              <div className="smart-advisor-heading">
                <div><span className="eyebrow">SMART FORK &amp; BUILD ADVISOR</span><h3>{compatibilityOsLabel(hostOS)}-tested build</h3></div>
                {recommendation && <strong className="official-status">{compatibilityStatusLabel(recommendation.status)}</strong>}
              </div>
              {!currentGame.title_id || !officialCompatibilityDetails ? <p className="hint">Waiting for an exact-title compatibility report.</p> : !recommendation ? <>
                <strong className="smart-advisor-empty">Not enough data</strong>
                <p className="hint">A recommendation requires an exact PPSA {compatibilityOsLabel(hostOS)} report that names one tested source commit, plus a matching verified local build or checkout. GitHub activity, stars, PRs, and local process exits do not count as proof.</p>
                {exactHostReports.length > 0 && <p className="hint">{exactHostReports.length} exact-title {compatibilityOsLabel(hostOS)} {exactHostReports.length === 1 ? "report is" : "reports are"} available; none can be tied unambiguously to a verified local source commit.</p>}
              </> : <>
                <div className="smart-advisor-build">
                  <strong>{recommendation.repo.owner}/{recommendation.repo.name}</strong>
                  <span>{recommendation.kind === "installed" ? `${recommendation.build!.build_type} · ${recommendation.build!.commit_sha}` : `Source checkout · ${recommendation.testedSha}`}</span>
                  <span>{recommendation.explanation}</span>
                </div>
                <div className="smart-advisor-evidence">
                  <span>Tested {recommendation.report.testedDate || "date not reported"} · {compatibilityOsLabel(recommendation.report.os)}</span>
                  <span>Tested version: {recommendation.report.testedVersion}</span>
                  {recommendation.report.hardware && <span>Hardware: {recommendation.report.hardware}</span>}
                  <span>Performance: See report notes; Studio does not normalize metrics across tests.</span>
                  <span>Stability: No normalized stability measurement is published.</span>
                </div>
                {recommendation.report.notes?.trim() && <div className="official-report-notes"><strong>Test notes and known issues</strong><p>{recommendation.report.notes.trim()}</p></div>}
                <div className="buttonrow smart-advisor-actions">
                  {recommendation.kind === "installed" && recommendation.build && <>
                    <button className="outline" onClick={() => onSetPreferredBuild(recommendation.build!.id)} disabled={currentGame.build_id === recommendation.build.id}>Use for this game</button>
                    <button className="primary" onClick={() => onActivateBuild?.(recommendation.build!.id)} disabled={!onActivateBuild || recommendation.build.active}>{recommendation.build.active ? "Already active" : "Activate build"}</button>
                  </>}
                  {recommendation.kind === "build" && <button className="primary" onClick={() => onBuildRecommendedSource?.(recommendation.repo.id)} disabled={!onBuildRecommendedSource || !recommendation.repo.source_path}>Open Build Studio for this checkout</button>}
                  <a href={`${recommendation.repo.url.replace(/\/$/, "")}/commit/${recommendation.testedSha}`} target="_blank" rel="noreferrer">View tested commit</a>
                  {recommendation.report.source?.url && <a href={recommendation.report.source.url} target="_blank" rel="noreferrer">Open test report</a>}
                </div>
                {recommendation.build?.community_prs?.length ? <div className="smart-advisor-prs"><strong>Included community PRs</strong><span>Build provenance only; PR inclusion does not prove a game-specific fix.</span><div>{recommendation.build.community_prs.map((pr) => <a href={pr.url} target="_blank" rel="noreferrer" key={`${pr.number}-${pr.head_sha}`}>#{pr.number}</a>)}</div></div> : null}
              </>}
            </section>
            {currentGame.title_id && officialCompatibilityIndex?.available && (() => {
              const summary = officialCompatibilityForGame(currentGame, officialCompatibilityIndex);
              const platformStatuses = summary?.os ?? {};
              return <>
                <div className="official-compatibility-summary">
                  <strong className={`official-status status-${(summary?.status || "untested").toLowerCase().replaceAll(/[^a-z]+/g, "-")}`}>
                    {compatibilityStatusLabel(summary?.status)}
                  </strong>
                  <span>{summary?.preferred_os === hostOS ? `${compatibilityOsLabel(hostOS)} test prioritized` : Object.keys(platformStatuses).length ? `Best reported result · no ${compatibilityOsLabel(hostOS)} test` : "No official test report"}</span>
                  {summary?.latest_tested && <span>Latest test {summary.latest_tested}</span>}
                </div>
                <div className="official-platform-results">
                  {(["linux", "windows", "macos"] as const).map((os) => <div className={`official-platform-result ${os === summary?.preferred_os ? "preferred" : ""}`} key={os}>
                    <span>{compatibilityOsLabel(os)}</span>
                    <strong>{compatibilityStatusLabel(platformStatuses[os])}</strong>
                    {summary?.report_counts[os] !== undefined && <small>{summary.report_counts[os]} {summary.report_counts[os] === 1 ? "report" : "reports"}</small>}
                  </div>)}
                </div>
                {officialCompatibilityError && <p className="hint">Detailed report unavailable: {officialCompatibilityError}</p>}
                {!officialCompatibilityDetails && !officialCompatibilityError && <p className="hint">Loading official test details…</p>}
                {officialCompatibilityDetails?.reports?.some((report) => normalizeTitleId(report.titleId) === normalizeTitleId(currentGame.title_id)) ? <div className="official-report-list">
                  {[...officialCompatibilityDetails.reports].filter((report) => normalizeTitleId(report.titleId) === normalizeTitleId(currentGame.title_id)).sort((a, b) => Number(b.os.toLowerCase() === hostOS) - Number(a.os.toLowerCase() === hostOS)).map((report, index) => <article className="official-report" key={`${report.slug}-${index}`}>
                    <div className="official-report-heading">
                      <strong>{compatibilityOsLabel(report.os)} · {compatibilityStatusLabel(report.status)}</strong>
                      <span>{report.testedDate}</span>
                    </div>
                    <div className="official-report-meta">
                      <span>Emulator: {report.testedVersion || "Not specified"}</span>
                      {report.hardware && <span>Hardware: {report.hardware}</span>}
                    </div>
                    {report.notes?.trim() && <div className="official-report-notes"><strong>Test details and known issues</strong><p>{report.notes.trim()}</p></div>}
                    {report.source?.url && <a href={report.source.url} target="_blank" rel="noreferrer">{report.source.label || "Open source report"}</a>}
                  </article>)}
                </div> : officialCompatibilityDetails && <p className="hint">The official index has a status, but no detailed report is published for this exact title ID.</p>}
              </>;
            })()}
            <button className="official-database-link" type="button" onClick={() => onOpenOfficialDatabase?.()}>Open official compatibility database</button>
          </section>
          <h2>Compatibility evidence</h2>
          <div className="kv">
            <span>Supported path recognized</span>
            <strong>{gameCompatibility?.path_recognized ? "Yes" : "No"}</strong>
          </div>
          <div className="kv">
            <span>Emulator process starts</span>
            <strong>
              {gameCompatibility?.emulator_process_starts ?? "—"} /{" "}
              {gameCompatibility?.launch_attempts ?? "—"} attempts
            </strong>
          </div>
          <div className="kv">
            <span>Emulator reported title ID</span>
            <strong>
              {gameCompatibility?.emulator_reported_title_id
                ? "Observed"
                : "Not observed"}
            </strong>
          </div>
          <div className="kv">
            <span>Reached gameplay</span>
            <strong>{gameCompatibility?.gameplay_status || "Unknown"}</strong>
          </div>
          <div className="kv">
            <span>Confirmed playable</span>
            <strong>
              {gameCompatibility?.playable_status || "Unconfirmed"}
            </strong>
          </div>
          <p className="hint">
            Automatic process evidence is separate from these user-confirmed
            test notes. A clean process exit alone does not establish gameplay
            compatibility.
          </p>
          <h3>Your test observations</h3>
          {currentGame.title_id && onSearchCompatibilityIssues && onReadSessionLog && onSubmitCompatibilityIssue && onOpenExternalUrl && <button type="button" className="compat-report-open" onClick={() => setCompatibilityReportOpen(true)}>Prepare GitHub game status report</button>}
          <div className="formgrid">
            <div>
              <label htmlFor="compatibility-outcome">Outcome observed</label>
              <select
                id="compatibility-outcome"
                value={manualOutcome}
                onChange={(event) => setManualOutcome(event.target.value)}
              >
                <option value="launch_failed">Launch failed</option>
                <option value="reached_menu">Reached menu</option>
                <option value="reached_gameplay">Reached gameplay</option>
                <option value="playable">Playable</option>
                <option value="playable_with_issues">
                  Playable with issues
                </option>
              </select>
            </div>
            <div>
              <label htmlFor="compatibility-notes">Notes (optional)</label>
              <textarea
                id="compatibility-notes"
                rows={3}
                maxLength={2000}
                value={manualNotes}
                onChange={(event) => setManualNotes(event.target.value)}
                placeholder="Describe the test and any issues"
              />
            </div>
          </div>
          <button className="primary" onClick={onRecordObservation}>
            Record test observation
          </button>
          {compatibilityObservations.map((observation) => (
            <div className="compat-observation" key={observation.id}>
              <div className="kv">
                <span>
                  {observation.outcome.replaceAll("_", " ")} ·{" "}
                  {date(observation.observed_at)}
                </span>
                <strong>
                  {observation.repository} · {short(observation.build_commit)} ·{" "}
                  {observation.build_type}
                </strong>
              </div>
              {observation.notes && <p>{observation.notes}</p>}
              <details>
                <summary>Patch and compatibility settings</summary>
                <pre>{observation.patch_config_json}</pre>
              </details>
            </div>
          ))}
        </div>
      )}
      {compatibilityReportOpen && onSearchCompatibilityIssues && onReadSessionLog && onSubmitCompatibilityIssue && onOpenExternalUrl && <CompatibilityReportDialog
        game={currentGame} builds={builds} repos={repos} system={system} graphics={graphics}
        observations={compatibilityObservations} sessions={sessions} githubTokenAvailable={githubTokenAvailable}
        onSearchIssues={onSearchCompatibilityIssues} onReadLog={onReadSessionLog}
        onSubmit={onSubmitCompatibilityIssue} onOpenUrl={onOpenExternalUrl}
        onClose={() => setCompatibilityReportOpen(false)}
      />}
      {gameDetailsTab === "Logs" && (
        <div className="panel">
          <h2>Recent session logs</h2>
          {sessions
            .filter((session) => session.game_id === currentGame.id)
            .map((session) => (
              <div className="kv" key={session.id}>
                <span>
                  {date(session.started_at)} · {session.outcome || "Running"}
                </span>
                <button
                  className="outline"
                  disabled={!session.log_path}
                  onClick={() => onOpenSessionLog(session)}
                >
                  {session.log_path ? "Open log" : "Log cleared"}
                </button>
              </div>
            ))}
          {!sessions.some((session) => session.game_id === currentGame.id) && (
            <p>No session logs yet.</p>
          )}
        </div>
      )}
    </section>
  );
}
