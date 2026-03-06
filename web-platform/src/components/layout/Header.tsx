import { useNavigate } from 'react-router-dom';
import { useAtomValue, useSetAtom } from 'jotai';
import { LogOut, User } from 'lucide-react';
import { userAtom } from '../../atoms';
import { authApi } from '../../api/auth';

export function Header() {
  const user = useAtomValue(userAtom);
  const setUser = useSetAtom(userAtom);
  const navigate = useNavigate();

  const handleLogout = () => {
    authApi.logout();
    setUser(null);
    navigate('/login');
  };

  return (
    <header className="h-14 bg-white border-b flex items-center justify-between px-4">
      <div className="text-sm text-gray-500">
        Welcome, <span className="font-medium">{user?.display_name || user?.email}</span>
      </div>
      <div className="flex items-center gap-3">
        <button className="p-2 hover:bg-gray-100 rounded-lg">
          <User className="w-5 h-5" />
        </button>
        <button
          onClick={handleLogout}
          className="p-2 hover:bg-gray-100 rounded-lg text-gray-500"
        >
          <LogOut className="w-5 h-5" />
        </button>
      </div>
    </header>
  );
}
