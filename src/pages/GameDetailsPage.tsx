import type { ReactNode, Dispatch, SetStateAction } from "react";
import { useEffect, useState } from "react";
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
  OfficialCompatibilityDetails,
  OfficialCompatibilityIndex,
} from "../types/studio";
import { date, formatBytes, short } from "../utils/format";
import { trophyGrade, trophyGrades } from "../utils/trophies";
import { compatibilityOsLabel, compatibilityStatusLabel, normalizeTitleId, officialCompatibilityForGame, recommendVerifiedBuild } from "../utils/compatibility";

type Tab = "Overview" | "Settings" | "Compatibility" | "Logs" | "Trophies" | "Cheats";
export type GameDetailsPageProps = {
  game: Game;
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
  onDownloadOnlineCheat?: (gameId:number, filename:string) => Promise<string>;
  onImportCustomPatch?: (gameId:number) => Promise<string | null>;
  onSaveCustomPatch?: (gameId:number, content:string) => Promise<string>;
  onSelectCustomPatch?: (path:string) => void;
};

export function GameDetailsPage({
  game: currentGame,
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
  onDownloadOnlineCheat,
  onImportCustomPatch,
  onSaveCustomPatch,
  onSelectCustomPatch,
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
  const [customPatchPath, setCustomPatchPath] = useState("");
  useEffect(() => {
    setCustomPatchDraft(JSON.stringify({
      id: currentGame.title_id || "TITLE_ID_REQUIRED",
      name: currentGame.title,
      version: currentGame.game_version || "1.0.0",
      mods: [],
    }, null, 2));
    setCustomPatchError("");
    setCustomPatchPath("");
  }, [currentGame.id, currentGame.title_id, currentGame.game_version, currentGame.title]);
  useEffect(() => {
    if ((gameDetailsTab !== "Trophies" && gameDetailsTab !== "Cheats") || !onInspectContent) return;
    let current = true;
    void onInspectContent(currentGame.id).then((result) => { if (current) { setContent(result); setContentError(""); } }).catch((error) => { if (current) setContentError(String(error)); });
    return () => { current = false; };
  }, [gameDetailsTab, currentGame.id, onInspectContent]);
  useEffect(() => {
    if (gameDetailsTab !== "Cheats" || !onFetchOnlineCheats) return;
    let current = true;
    setRemoteCheats(null);
    setRemoteCheatError("");
    setDownloadedCheatPath("");
    void onFetchOnlineCheats(currentGame.id)
      .then((items) => { if (current) setRemoteCheats(items); })
      .catch((error) => { if (current) setRemoteCheatError(String(error)); });
    return () => { current = false; };
  }, [gameDetailsTab, currentGame.id, onFetchOnlineCheats]);
  const importCustomPatch = async () => {
    if (!onImportCustomPatch) return;
    setCustomPatchBusy(true);
    setCustomPatchError("");
    try {
      const path = await onImportCustomPatch(currentGame.id);
      if (!path) return;
      setCustomPatchPath(path);
      onSelectCustomPatch?.(path);
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
      onSelectCustomPatch?.(path);
    } catch (error) {
      setCustomPatchError(String(error));
    } finally {
      setCustomPatchBusy(false);
    }
  };
  const recommendation = recommendVerifiedBuild(
    currentGame,
    officialCompatibilityDetails ?? null,
    builds,
    repos,
  );
  const exactLinuxReports = officialCompatibilityDetails?.reports.filter(
    (report) => normalizeTitleId(report.titleId) === normalizeTitleId(currentGame.title_id) && report.os.trim().toLowerCase() === "linux",
  ) ?? [];
  return (
    <section className="page game-details-page">
      <div className="game-details-header">
        <button className="outline" onClick={onBack}>
          ← Library
        </button>
        <div>
          <p className="eyebrow">GAME DETAILS</p>
          <h1>{currentGame.title}</h1>
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
        {(["Overview", "Settings", "Compatibility", "Logs", "Trophies", "Cheats"] as const).map(
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
      {gameDetailsTab === "Overview" && (
        <div className="details-overview">
          <div className="panel">
            <h2>Game overview</h2>
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
            <div className="buttonrow">
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
                      {repos.find((r) => r.id === b.repo_id)?.name} ·{" "}
                      {short(b.commit_sha)} · {b.build_type}
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
          <div>
            <h2>Game settings</h2>
            <p>
              Only options advertised by this selected emulator build can be
              saved. Empty values inherit global defaults or KytyPS5 defaults.
            </p>
          </div>
          {gameProfileEditor}
        </div>
      )}
      {(gameDetailsTab === "Trophies" || gameDetailsTab === "Cheats") && <div className="panel game-content-panel">
        {gameDetailsTab === "Trophies" ? <>
          <h2>Trophies</h2>
          <p>All trophies from the game package, grouped by grade. Earned status comes from this emulator build’s local trophy progress.</p>
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
          <h2>Cheats</h2>
          <p>Search local mods and matching PS5 cheat files. Downloading saves a file to Studio’s data directory; it does not enable or apply the cheats.</p>
          <label className="cheat-search">Search cheats<input value={cheatQuery} onChange={(e)=>setCheatQuery(e.target.value)} placeholder="Search by cheat name" /></label>
          {contentError && <p className="error-note">{contentError}</p>}
          {content?.cheat_error && <p className="error-note">{content.cheat_error}</p>}
          {content?.cheat_source && <p className="hint">Source: {content.cheat_source}</p>}
          {content?.cheats.filter((c)=>c.name.toLowerCase().includes(cheatQuery.trim().toLowerCase())).map((c,i)=><div className="kv cheat-row" key={`${c.name}-${i}`}><strong>{c.name}</strong><span>{c.enabled ? "Enabled in file" : "Disabled in file"} · {c.title_id || "No title ID"} · v{c.version || "?"}</span></div>)}
          {content && !content.cheats.length && !content.cheat_error && <p>No configured game patch file was found. Set one under Game settings.</p>}
          <section className="custom-patch-editor">
            <div className="buttonrow">
              <h3>Custom game patch</h3>
              <button className="outline" disabled={!onImportCustomPatch || customPatchBusy} onClick={() => void importCustomPatch()}>
                {customPatchBusy ? "Working…" : "Import JSON patch"}
              </button>
            </div>
            <p className="hint">
              Import a GoldHEN-style JSON file or edit a patch below. The title ID must match this game. ZIP archives are not applied directly.
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
            {customPatchPath && <p className="hint">Patch saved and selected for this game: <code>{customPatchPath}</code>. Open Game Settings and save the profile to apply it when launching.</p>}
          </section>
          <h3>Online PS5 cheat repository</h3>
          <p className="hint">Source: TeeKay87/HEN-Cheats-Collection · files are matched by title ID and ordered with the closest game version first.</p>
          {remoteCheatError && <p className="error-note">{remoteCheatError}</p>}
          {!remoteCheats && !remoteCheatError && <p>Searching repository…</p>}
          {remoteCheats?.length === 0 && <p>No matching PS5 cheat file was found for this title ID.</p>}
          {remoteCheats?.map((file) => {
            const matchingMods = file.mods.filter((mod) => mod.name.toLowerCase().includes(cheatQuery.trim().toLowerCase()));
            if (cheatQuery.trim() && !matchingMods.length) return null;
            return <article className="remote-cheat-file" key={file.filename}>
              <div className="remote-cheat-heading"><div><strong>{file.title}</strong><small>{file.filename} · version {file.version || "unknown"} · {file.mods.length} cheats</small></div><button className="outline" disabled={!onDownloadOnlineCheat || downloadingCheat === file.filename} onClick={() => {
                if (!onDownloadOnlineCheat) return;
                setDownloadingCheat(file.filename);
                void onDownloadOnlineCheat(currentGame.id, file.filename).then(setDownloadedCheatPath).catch((error) => setRemoteCheatError(String(error))).finally(() => setDownloadingCheat(""));
              }}>{downloadingCheat === file.filename ? "Downloading…" : "Download file"}</button></div>
              {matchingMods.map((mod, index) => <div className="kv cheat-row" key={`${mod.name}-${index}`}><strong>{mod.name}</strong><span>{mod.type}</span></div>)}
              <a href={file.source} target="_blank" rel="noreferrer">View source file</a>
            </article>;
          })}
          {downloadedCheatPath && <p className="hint">Saved locally: <code>{downloadedCheatPath}</code>. Select it in this game’s Settings under Game patch file to use it.</p>}
        </>}
      </div>}
      {gameDetailsTab === "Compatibility" && (
        <div className="panel compatibility-panel">
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
                <div><span className="eyebrow">SMART FORK &amp; BUILD ADVISOR</span><h3>Linux-tested build</h3></div>
                {recommendation && <strong className="official-status">{compatibilityStatusLabel(recommendation.status)}</strong>}
              </div>
              {!currentGame.title_id || !officialCompatibilityDetails ? <p className="hint">Waiting for an exact-title compatibility report.</p> : !recommendation ? <>
                <strong className="smart-advisor-empty">Not enough data</strong>
                <p className="hint">A recommendation requires an exact PPSA Linux report that names one tested source commit, plus a matching verified local build or checkout. GitHub activity, stars, PRs, and local process exits do not count as proof.</p>
                {exactLinuxReports.length > 0 && <p className="hint">{exactLinuxReports.length} exact-title Linux {exactLinuxReports.length === 1 ? "report is" : "reports are"} available; none can be tied unambiguously to a verified local source commit.</p>}
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
                  <span>{summary?.preferred_os === "linux" ? "Linux test prioritized" : Object.keys(platformStatuses).length ? "Best reported result · no Linux test" : "No official test report"}</span>
                  {summary?.latest_tested && <span>Latest test {summary.latest_tested}</span>}
                </div>
                <div className="official-platform-results">
                  {(["linux", "windows", "macos"] as const).map((os) => <div className={`official-platform-result ${os === "linux" ? "preferred" : ""}`} key={os}>
                    <span>{compatibilityOsLabel(os)}</span>
                    <strong>{compatibilityStatusLabel(platformStatuses[os])}</strong>
                    {summary?.report_counts[os] !== undefined && <small>{summary.report_counts[os]} {summary.report_counts[os] === 1 ? "report" : "reports"}</small>}
                  </div>)}
                </div>
                {officialCompatibilityError && <p className="hint">Detailed report unavailable: {officialCompatibilityError}</p>}
                {!officialCompatibilityDetails && !officialCompatibilityError && <p className="hint">Loading official test details…</p>}
                {officialCompatibilityDetails?.reports?.some((report) => normalizeTitleId(report.titleId) === normalizeTitleId(currentGame.title_id)) ? <div className="official-report-list">
                  {[...officialCompatibilityDetails.reports].filter((report) => normalizeTitleId(report.titleId) === normalizeTitleId(currentGame.title_id)).sort((a, b) => Number(b.os === "linux") - Number(a.os === "linux")).map((report, index) => <article className="official-report" key={`${report.slug}-${index}`}>
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
                <a className="official-database-link" href="https://kytyps5.github.io/compatibility/" target="_blank" rel="noreferrer">Open official compatibility database</a>
              </>;
            })()}
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
