import { useEffect, useMemo, useState } from "react";
import type { Build, Game, GraphicsDiagnostics, ManualCompatibilityObservation, Repo, Session, System } from "../../types/studio";
import { compatibilityIssueBody, compatibilityIssueTitle, compatibilityStatusFromObservation, type CompatibilityIssueDraft, type CompatibilityIssueLink } from "../../utils/compatibilityReport";

type Props = {
  game: Game; builds: Build[]; repos: Repo[]; system: System | null; graphics: GraphicsDiagnostics | null;
  observations: ManualCompatibilityObservation[]; sessions: Session[]; githubTokenAvailable: boolean;
  onSearchIssues: (titleId: string) => Promise<CompatibilityIssueLink[]>;
  onReadLog: (session: Session) => Promise<string>;
  onSubmit: (title: string, body: string) => Promise<{ number: number; url: string }>;
  onOpenUrl: (url: string) => Promise<void>; onClose: () => void;
};

export function CompatibilityReportDialog(props: Props) {
  const { game, builds, repos, system, graphics, observations, sessions, githubTokenAvailable } = props;
  const latestObservation = observations[0];
  const latestSession = sessions.filter((session) => session.game_id === game.id).sort((a, b) => b.started_at - a.started_at)[0];
  const build = builds.find((item) => item.id === (latestObservation?.build_id ?? latestSession?.build_id ?? game.build_id)) ?? builds.find((item) => item.active);
  const repo = repos.find((item) => item.id === build?.repo_id);
  const [draft, setDraft] = useState<CompatibilityIssueDraft>(() => ({
    gameTitle: game.title, titleId: game.title_id ?? "", gameVersion: game.game_version ?? "", kytyVersion: build ? `${repo?.owner ?? "unknown"}/${repo?.name ?? "unknown"} @ ${build.commit_sha} (${build.build_type})` : "Unknown",
    status: compatibilityStatusFromObservation(latestObservation?.outcome ?? ""), details: latestObservation?.notes ?? "",
    steps: "", expected: "", os: system ? `${system.os} ${system.arch}` : "", cpu: system?.cpu ?? "",
    gpu: [system?.gpu, ...(graphics?.drivers ?? [])].filter(Boolean).join("; "),
    memory: system ? `${(system.memory_total_bytes / 1024 ** 3).toFixed(1)} GiB RAM; VRAM not reported` : "",
    patchConfig: latestObservation?.patch_config_json ?? "", extraNotes: "", relatedIssues: [], log: "",
  }));
  const [issues, setIssues] = useState<CompatibilityIssueLink[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [created, setCreated] = useState<{ number: number; url: string } | null>(null);
  const [includeLog, setIncludeLog] = useState(false);
  const [logLoaded, setLogLoaded] = useState(false);
  const title = useMemo(() => compatibilityIssueTitle(game.title, game.title_id ?? ""), [game.title, game.title_id]);

  useEffect(() => {
    if (!game.title_id) return;
    let cancelled = false;
    props.onSearchIssues(game.title_id).then((items) => { if (!cancelled) setIssues(items); }).catch((reason) => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, [game.title_id]);

  const update = (key: keyof CompatibilityIssueDraft, value: string) => setDraft((current) => ({ ...current, [key]: value }));
  const loadLog = async () => {
    if (!latestSession || logLoaded) return;
    setBusy(true); setError("");
    try { const log = await props.onReadLog(latestSession); setDraft((current) => ({ ...current, log: log.slice(-40_000) })); setLogLoaded(true); }
    catch (reason) { setError(`Could not read session log: ${String(reason)}`); }
    finally { setBusy(false); }
  };
  const submit = async () => {
    setBusy(true); setError("");
    try { setCreated(await props.onSubmit(title, compatibilityIssueBody(draft))); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };

  return <div className="compat-report-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) props.onClose(); }}>
    <section className="compat-report-dialog" role="dialog" aria-modal="true" aria-labelledby="compat-report-title">
      <header><div><small>PUBLIC GITHUB ISSUE</small><h2 id="compat-report-title">Game status report</h2></div><button onClick={props.onClose} aria-label="Close">×</button></header>
      {created ? <><p>Issue #{created.number} was created.</p><div className="compat-report-actions"><button onClick={() => void props.onOpenUrl(created.url)}>Open issue</button><button onClick={props.onClose}>Done</button></div></> : <>
        <p className="hint">Review everything before publishing. Host details, saved patch settings, selected issue links and any checked log become public.</p>
        {!githubTokenAvailable && <div role="alert"><p>Configure a GitHub token in Settings with Issues read and write access to KytyPS5/KytyPS5.</p><button type="button" onClick={() => void props.onOpenUrl("https://github.com/KytyPS5/KytyPS5/issues/new?template=kytyps5-game-emulation.yaml")}>Open official issue form</button></div>}
        <div className="compat-report-fields">
          <label>Official status<select value={draft.status} onChange={(event) => update("status", event.target.value)}><option value="">Select only if you tested it</option><option>Doesn't boot</option><option>Logo</option><option>Main menu</option><option>In game</option></select></label>
          <label>Result details<textarea value={draft.details} onChange={(event) => update("details", event.target.value)} /></label>
          <label>Steps to reproduce<textarea value={draft.steps} onChange={(event) => update("steps", event.target.value)} placeholder="Describe the steps you performed" /></label>
          <label>Expected behavior<textarea value={draft.expected} onChange={(event) => update("expected", event.target.value)} /></label>
          <label>OS / CPU / GPU / RAM<input value={[draft.os, draft.cpu, draft.gpu, draft.memory].filter(Boolean).join(" · ")} readOnly /></label>
          <label>Extra notes<textarea value={draft.extraNotes} onChange={(event) => update("extraNotes", event.target.value)} placeholder="Add details that do not fit the fields above" /></label>
        </div>
        <div className="compat-report-issues"><strong>Possible existing reports for {game.title_id}</strong>{issues.length ? issues.map((issue) => <label key={issue.number}><input type="checkbox" checked={draft.relatedIssues.some((item) => item.number === issue.number)} onChange={(event) => setDraft((current) => ({ ...current, relatedIssues: event.target.checked ? [...current.relatedIssues, issue] : current.relatedIssues.filter((item) => item.number !== issue.number) }))} /><span>#{issue.number} · {issue.title} ({issue.state})</span><button type="button" onClick={() => void props.onOpenUrl(`https://github.com/KytyPS5/KytyPS5/issues/${issue.number}`)}>View</button></label>) : <small>No matching issues found, or the search is still loading.</small>}</div>
        <div className="compat-report-log"><label><input type="checkbox" checked={includeLog} disabled={!logLoaded} onChange={(event) => setIncludeLog(event.target.checked)} /> Include latest session log in public issue</label>{latestSession ? <button type="button" disabled={busy || logLoaded} onClick={() => void loadLog()}>{logLoaded ? "Log loaded" : "Load latest session log"}</button> : <small>No session log available.</small>}<small>Log content may include local paths and system details. It is limited to the last 40,000 characters.</small></div>
        {error && <p role="alert">{error}</p>}
        <details className="compat-report-preview"><summary>Preview report body</summary><pre>{compatibilityIssueBody({ ...draft, log: includeLog ? draft.log : "" })}</pre></details>
        <div className="compat-report-actions"><button onClick={props.onClose}>Cancel</button><button className="primary" disabled={!githubTokenAvailable || busy || !draft.status || !draft.steps.trim() || !draft.expected.trim()} onClick={() => void submit()}>{busy ? "Working…" : "Create public issue"}</button></div>
      </>}
    </section>
  </div>;
}
