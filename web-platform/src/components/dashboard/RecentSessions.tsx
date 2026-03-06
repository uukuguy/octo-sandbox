import { useNavigate } from 'react-router-dom';
import { Session } from '../../api/types';

interface RecentSessionsProps {
  sessions: Session[];
}

export function RecentSessions({ sessions }: RecentSessionsProps) {
  const navigate = useNavigate();

  if (sessions.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500">
        No sessions yet. Start a new chat!
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {sessions.slice(0, 5).map((session) => (
        <button
          key={session.id}
          onClick={() => navigate(`/chat/${session.id}`)}
          className="w-full text-left p-3 rounded-lg hover:bg-gray-50 border"
        >
          <div className="font-medium">{session.name || 'Untitled'}</div>
          <div className="text-sm text-gray-500">
            {new Date(session.updated_at).toLocaleDateString()}
          </div>
        </button>
      ))}
    </div>
  );
}
