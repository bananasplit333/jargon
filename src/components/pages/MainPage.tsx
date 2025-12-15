// src/components/history/HistoryContent.tsx
import React from 'react';
import { Info } from 'lucide-react';

export const MainPage: React.FC = () => {
  return (
    <div className="max-w-3xl mx-auto space-y-8">
      
      {/* 1. Welcome Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold text-gray-900 dark:text-white">
          Welcome back, Jae
        </h1>
        {/* Stats Pill */}
        <div className="flex items-center gap-4 bg-white dark:bg-gray-800 px-4 py-2 rounded-full shadow-sm text-xs font-medium border border-gray-100 dark:border-gray-700 text-gray-600 dark:text-gray-300">
          <span className="flex items-center gap-1">👋 0 week</span>
          <span className="text-gray-300">|</span>
          <span className="flex items-center gap-1">🚀 145 words</span>
          <span className="text-gray-300">|</span>
          <span className="flex items-center gap-1">🏆 159 WPM</span>
        </div>
      </div>

      {/* 2. Hero Banner (Yellow Box) */}
      <div className="bg-[#FFFBEB] dark:bg-yellow-900/20 border border-yellow-100 dark:border-yellow-900/50 rounded-2xl p-8 relative overflow-hidden">
        <div className="relative z-10 space-y-4">
          <h2 className="text-3xl font-serif text-gray-900 dark:text-yellow-50">
            Hold down <span className="font-bold">Ctrl+Win</span> to dictate in any app
          </h2>
          <p className="text-gray-700 dark:text-yellow-100/80 max-w-xl leading-relaxed">
            Flow works in all your apps. Try it in <b>email</b>, <b>messages</b>, <b>docs</b> or <b>anywhere else</b>. 
            Use ctrl + win + space for hands-free mode.
          </p>
          <button className="mt-4 px-5 py-2.5 bg-gray-900 hover:bg-black text-white text-sm font-medium rounded-lg transition-colors shadow-lg">
            See how it works
          </button>
        </div>
      </div>

      {/* 3. Timeline / History Feed */}
      <div className="space-y-4">
        <h3 className="text-xs font-bold text-gray-400 uppercase tracking-wider">Today</h3>
        
        {/* Card Container */}
        <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 shadow-sm overflow-hidden">
          {/* Item 1 */}
          <div className="flex gap-6 p-5 border-b border-gray-100 dark:border-gray-700/50 hover:bg-gray-50 transition-colors">
            <span className="text-xs font-medium text-gray-400 w-16 pt-1">04:24 PM</span>
            <div className="flex-1 flex items-start gap-2 text-gray-400 italic">
              <span>Audio is silent.</span>
              <Info className="w-4 h-4 mt-0.5" />
            </div>
          </div>

          {/* Item 2 */}
          <div className="flex gap-6 p-5 border-b border-gray-100 dark:border-gray-700/50 hover:bg-gray-50 transition-colors">
            <span className="text-xs font-medium text-gray-400 w-16 pt-1">04:21 PM</span>
            <p className="flex-1 text-gray-700 dark:text-gray-300 leading-relaxed">
             penispienwiernepoirnepneppenispenis......
            </p>
          </div>

          {/* Item 3 */}
          <div className="flex gap-6 p-5 hover:bg-gray-50 transition-colors">
            <span className="text-xs font-medium text-gray-400 w-16 pt-1">04:20 PM</span>
            <p className="flex-1 text-gray-700 dark:text-gray-300 leading-relaxed">
              who are some famous app developers, and how did they actually start designing their process?
            </p>
          </div>
        </div>
      </div>

    </div>
  );
};