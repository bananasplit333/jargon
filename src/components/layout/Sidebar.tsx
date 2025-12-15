// src/components/layout/Sidebar.tsx
import React from 'react';
import {
  Home,
  Book,
  Layers,
  Type,
  FileText,
  Settings,
  HelpCircle,
  Users,
  Zap,
} from 'lucide-react';
import { NavLink } from "react-router"

const mainNav = [
  { name: 'Home', icon: Home, to: '/' },
  { name: 'Dictionary', icon: Book, to: '/dictionary' },
  { name: 'Snippets', icon: Layers, to: '/snippets' },
  { name: 'Style', icon: Type, to: '/style' },
  { name: 'Notes', icon: FileText, to: '/notes' },
];

const utilityNav = [
  { name: 'Invite your team', icon: Users },
  { name: 'Get a free month', icon: Zap },
  { name: 'Settings', icon: Settings },
  { name: 'Help', icon: HelpCircle },
];

interface NavItemProps {
  name: string;
  Icon: React.ElementType;
  to: string;
}

const NavItem: React.FC<NavItemProps> = ({ name, Icon, to }) => (
  <NavLink
    to={to}
    className={({ isActive }) =>
        `
        group flex items-center px-3 py-2 text-sm font-medium rounded-md transition-all duration-200
        ${
          isActive
            ? 'bg-gray-200 text-gray-900 dark:bg-gray-800 dark:text-white'
            : 'text-gray-600 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-white'
        }
        `
      }
    >
    {({ isActive }) => (
      <>
        <Icon
          className={`mr-3 h-5 w-5 flex-shrink-0 transition-colors duration-200 ${
            isActive
              ? 'text-gray-900 dark:text-white'
              : 'text-gray-400 group-hover:text-gray-500 dark:group-hover:text-gray-300'
          }`}
        />
        {name}
      </>
    )}
  </NavLink>
);

export const Sidebar: React.FC = () => {
  return (
    <div className="flex flex-col h-full border-r border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900">
      <div className="flex items-center h-16 px-6 justify-between">
        <div className="flex items-center gap-2 font-bold text-xl tracking-tight">
          <div className="w-6 h-6 bg-purple-600 rounded-md flex items-center justify-center">
            <span className="text-white text-xs">J</span>
          </div>
          <span>Jargon</span>
          <span className="ml-2 px-2 py-0.5 text-[10px] uppercase font-bold text-white bg-purple-500 rounded-full">
            Beta
          </span>
        </div>
      </div>

      <div className="flex-1 flex flex-col overflow-hidden px-4 gap-y-6 py-4">
        <nav className="space-y-1">
          {mainNav.map((item) => (
            <NavItem key={item.name} name={item.name} Icon={item.icon} to={item.to} />
          ))}
        </nav>

        <div className="mt-auto bg-gray-50 dark:bg-gray-800/50 rounded-xl p-4 border border-gray-100 dark:border-gray-700">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-gray-900 dark:text-white">Flow Pro Trial 👋</span>
            <button className="text-gray-400 hover:text-gray-600">×</button>
          </div>
          <div className="w-full bg-gray-200 rounded-full h-1.5 mb-2 dark:bg-gray-700">
            <div className="bg-purple-600 h-1.5 rounded-full" style={{ width: '0%' }}></div>
          </div>
          <p className="text-xs text-gray-500 dark:text-gray-400 mb-3">
            0 of 14 days used
          </p>
          <p className="text-xs text-gray-600 dark:text-gray-300 mb-4 leading-relaxed">
            Upgrade to Flow Pro to keep unlimited words and Pro features.
          </p>
          <button className="w-full py-2 px-3 bg-white dark:bg-gray-700 border border-gray-200 dark:border-gray-600 rounded-lg text-sm font-medium shadow-sm hover:bg-gray-50 transition-colors">
            Upgrade to Pro
          </button>
        </div>

        <nav className="space-y-1 pb-4">
          {utilityNav.map((item) => (
            <div
              key={item.name}
              className="group flex items-center px-3 py-2 text-sm font-medium rounded-md text-gray-600 hover:bg-gray-100"
            >
              <item.icon className="mr-3 h-5 w-5 text-gray-400" />
              {item.name}
            </div>
          ))}
        </nav>
      </div>
    </div>
  );
};
