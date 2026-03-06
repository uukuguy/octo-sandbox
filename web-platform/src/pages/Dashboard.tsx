import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { MessageSquare, Users, Bot } from 'lucide-react';
import { useAtom } from 'jotai';
import { sessionsAtom } from '../atoms';
import { sessionsApi } from '../api/sessions';
import { StatsCard } from '../components/dashboard/StatsCard';
import { RecentSessions } from '../components/dashboard/RecentSessions';

export function DashboardPage() {
  const [loading, setLoading] = useState(true);
  const [sessions, setSessions] = useAtom(sessionsAtom);
  const navigate = useNavigate();

  useEffect(() => {
    sessionsApi.list()
      .then(setSessions)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [setSessions]);

  const handleNewChat = async () => {
    try {
      const session = await sessionsApi.create();
      navigate(`/chat/${session.id}`);
    } catch (err) {
      console.error(err);
    }
  };

  if (loading) {
    return <div>Loading...</div>;
  }

  return (
    <div className="max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold mb-6">Dashboard</h1>

      <div className="grid grid-cols-3 gap-4 mb-8">
        <StatsCard title="Sessions" value={sessions.length} icon={<MessageSquare className="w-6 h-6" />} />
        <StatsCard title="Messages" value={sessions.length} icon={<Users className="w-6 h-6" />} />
        <StatsCard title="Agents" value={sessions.length} icon={<Bot className="w-6 h-6" />} />
      </div>

      <div className="mb-6">
        <h2 className="text-lg font-semibold mb-3">Recent Sessions</h2>
        <RecentSessions sessions={sessions} />
      </div>

      <button
        onClick={handleNewChat}
        className="w-full bg-primary text-white py-3 rounded-lg hover:opacity-90"
      >
        + New Chat
      </button>
    </div>
  );
}
