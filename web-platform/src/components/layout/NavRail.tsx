import { NavLink } from 'react-router-dom';
import { Home, MessageSquare, History } from 'lucide-react';

const navItems = [
  { to: '/dashboard', icon: Home, label: 'Home' },
  { to: '/chat', icon: MessageSquare, label: 'Chat' },
  { to: '/sessions', icon: History, label: 'History' },
];

export function NavRail() {
  return (
    <nav className="w-16 bg-white border-r flex flex-col py-4">
      <div className="px-3 mb-6">
        <span className="text-xl font-bold">O</span>
      </div>
      <div className="flex-1 space-y-2">
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `flex flex-col items-center py-2 px-3 text-xs ${
                isActive ? 'text-primary' : 'text-gray-500'
              }`
            }
          >
            <item.icon className="w-5 h-5 mb-1" />
            <span>{item.label}</span>
          </NavLink>
        ))}
      </div>
    </nav>
  );
}
