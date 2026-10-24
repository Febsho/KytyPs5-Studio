import type { Game } from '../../types/studio';
import { isPubliclyRecognizedGame } from '../games/libraryModel';

type Props = {
  games: Game[];
  currentGameId: number;
  sourceGameId: number | null;
  loading: boolean;
  dirty: boolean;
  canRestore: boolean;
  onSourceChange: (gameId: number | null) => void;
  onCopy: (gameId: number) => void;
  onRestore: () => void;
};

export function ProfileActions({ games, currentGameId, sourceGameId, loading, dirty, canRestore, onSourceChange, onCopy, onRestore }: Props) {
  const sources = games.filter(game =>
    game.id !== currentGameId
    && game.file_status === 'Available'
    && isPubliclyRecognizedGame(game),
  );
  return <div className="profile-actions">
    <label>Copy profile from another game
      <select aria-label="Copy profile from another game" value={sourceGameId ?? ''} onChange={event => onSourceChange(event.target.value ? Number(event.target.value) : null)}>
        <option value="">Choose a game</option>
        {sources.map(game => <option key={game.id} value={game.id}>{game.title}</option>)}
      </select>
    </label>
    <button className="outline" disabled={loading || sourceGameId === null || !sources.some(game => game.id === sourceGameId)} onClick={() => sourceGameId !== null && onCopy(sourceGameId)}>Copy into editor</button>
    {dirty && <button className="outline" disabled={loading || !canRestore} onClick={onRestore}>Restore saved values</button>}
  </div>;
}
