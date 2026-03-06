import { useAtom } from 'jotai';
import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useSetAtom } from 'jotai';
import { Plus, Trash2 } from 'lucide-react';
import { sessionsAtom, currentSessionIdAtom } from '../atoms';
import { sessionsApi } from '../api/sessions';
import { Session } from '../api/types';

export function SessionsPage() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);
  const setSessionsAtom = useSetAtom(sessionsAtom);
  const [currentSessionId, setCurrentSessionId] = useAtom(currentSessionIdAtom);
  const navigate = useNavigate();

  useEffect(() => {
    sessionsApi.list()
      .then((data) => {
        setSessions(data);
        setSessionsAtom(data);
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [setSessionsAtom]);

  const handleCreate = async () => {
    try {
      const session = await sessionsApi.create();
      setSessions((prev) => [session, ...prev]);
      setCurrentSessionId(session.id);
      navigate(`/chat/${session.id}`);
    } catch (err) {
      console.error(err);
    }
  };

  const handleDelete = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    if (!confirm('Delete this session?')) return;

    try {
      await sessionsApi.delete(sessionId);
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
      setSessionsAtom((prev) => prev.filter((s) => s.id !== sessionId));
      // Clear current session if deleting the active one
      if (currentSessionId === sessionId) {
        setCurrentSessionId(null);
      }
    } catch (err) {
      console.error(err);
    }
  };

  if (loading) {
    return <div>Loading...</div>;
  }

  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">Sessions</h1>
        <button
          onClick={handleCreate}
          className="flex items-center gap-2 bg-primary text-white px-4 py-2 rounded-lg"
        >
          <Plus className="w-5 h-5" />
          New
        </button>
      </div>

      {sessions.length === 0 ? (
        <div className="text-center py-8 text-gray-500">
          No sessions yet
        </div>
      ) : (
        <div className="space-y-2">
          {sessions.map((session) => (
            <button
              key={session.id}
              onClick={() => {
                setCurrentSessionId(session.id);
                navigate(`/chat/${session.id}`);
              }}
              className="w-full text-left p-4 rounded-lg border hover:bg-gray-50 flex items-center justify-between"
            >
              <div>
                <div className="font-medium">
                  {session.name || 'Untitled'}
                </div>
                <div className="text-sm text-gray-500">
                  {new Date(session.updated_at).toLocaleString()}
                </div>
              </div>
              <button
                onClick={(e) => handleDelete(e, session.id)}
                className="p-2 text-gray-400 hover:text-red-500"
              >
                <Trash2 className="w-5 h-5" />
              </button>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
