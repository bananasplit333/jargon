// src/components/history/HistoryContent.tsx
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Check, Copy, Info } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import confetti from 'canvas-confetti';

type TranscriptEvent = {
  text: string;
};

type HistoryItem = {
  id: string;
  text: string;
  ts: number;
};

const HISTORY_STORAGE_KEY = 'jargon:transcriptHistory:v1';
const MAX_HISTORY_ITEMS = 200;

function createId() {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function loadHistory(): HistoryItem[] {
  if (typeof window === 'undefined') {
    return [];
  }
  try {
    const raw = window.localStorage.getItem(HISTORY_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .filter((item): item is HistoryItem => {
        return (
          typeof item === 'object' &&
          item !== null &&
          'id' in item &&
          'text' in item &&
          'ts' in item &&
          typeof (item as any).id === 'string' &&
          typeof (item as any).text === 'string' &&
          typeof (item as any).ts === 'number'
        );
      })
      .slice(0, MAX_HISTORY_ITEMS);
  } catch {
    return [];
  }
}

async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const textarea = document.createElement('textarea');
      textarea.value = text;
      textarea.style.position = 'fixed';
      textarea.style.top = '0';
      textarea.style.left = '0';
      textarea.style.opacity = '0';
      textarea.setAttribute('readonly', 'true');
      document.body.appendChild(textarea);
      textarea.select();
      textarea.setSelectionRange(0, textarea.value.length);
      const ok = document.execCommand('copy');
      document.body.removeChild(textarea);
      return ok;
    } catch {
      return false;
    }
  }
}

export const MainPage: React.FC = () => {
  const [history, setHistory] = useState<HistoryItem[]>(() => loadHistory());
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const clearCopiedTimer = useRef<number | null>(null);

  const timeFormatter = useMemo(() => {
    return new Intl.DateTimeFormat(undefined, {
      hour: '2-digit',
      minute: '2-digit',
    });
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    window.localStorage.setItem(
      HISTORY_STORAGE_KEY,
      JSON.stringify(history.slice(0, MAX_HISTORY_ITEMS)),
    );
  }, [history]);

  useEffect(() => {
    return () => {
      if (clearCopiedTimer.current !== null) {
        window.clearTimeout(clearCopiedTimer.current);
      }
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: null | (() => void) = null;

    listen<TranscriptEvent>('stt:transcript', (event) => {
      const text = event.payload?.text?.trim();
      if (!text) {
        return;
      }

      setHistory((prev) => [{ id: createId(), text, ts: Date.now() }, ...prev].slice(0, MAX_HISTORY_ITEMS));
    })
      .then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch((err) => {
        console.warn('stt:transcript listener not available in this environment', err);
      });

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  const onCopyHistoryItem = async (item: HistoryItem) => {
    const ok = await copyToClipboard(item.text);
    if (!ok) {
      return;
    }

    setCopiedId(item.id);
    if (clearCopiedTimer.current !== null) {
      window.clearTimeout(clearCopiedTimer.current);
    }
    clearCopiedTimer.current = window.setTimeout(() => setCopiedId(null), 1200);
  };

  return (
    <div className="max-w-3xl mx-auto space-y-8">
      
      {/* 1. Welcome Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold text-gray-900 dark:text-white">
          Welcome back. Ready to dictate?
        </h1>
      </div>

      {/* 2. Hero Banner (Yellow Box) */}
      <div className="bg-[#FFFBEB] dark:bg-yellow-900/20 border border-yellow-100 dark:border-yellow-900/50 rounded-2xl p-8 relative overflow-hidden">
        <div className="relative z-10 space-y-4">
          <h2 className="text-3xl font-serif text-gray-900 dark:text-yellow-50">
            Hold down <span className="font-bold">Ctrl+Shift</span> to dictate in any app
          </h2>
          <p className="text-gray-700 dark:text-yellow-100/80 max-w-xl leading-relaxed">
            placeholder text
          </p>
          <button className="mt-4 px-5 py-2.5 bg-gray-900 hover:bg-black text-white text-sm font-medium rounded-lg transition-colors shadow-lg"
            onClick={() => confetti()}
          >
            See how it works
          </button>
        </div>
      </div>

      {/* 3. Timeline / History Feed */}
      <div className="space-y-4">
        <h3 className="text-xs font-bold text-gray-400 uppercase tracking-wider">Today</h3>
        
        {/* Card Container */}
        <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 shadow-sm overflow-hidden">
          {history.length === 0 ? (
            <div className="flex gap-6 p-5 hover:bg-gray-50 dark:hover:bg-gray-700/30 transition-colors">
              <span className="text-xs font-medium text-gray-400 w-16 pt-1">
                {timeFormatter.format(Date.now())}
              </span>
              <div className="flex-1 flex items-start gap-2 text-gray-400 italic">
                <span>No transcripts yet. Dictate once to start your history.</span>
                <Info className="w-4 h-4 mt-0.5" />
              </div>
            </div>
          ) : (
            history.map((item, idx) => (
              <button
                key={item.id}
                type="button"
                title="Click to copy"
                onClick={() => onCopyHistoryItem(item)}
                className={
                  `group w-full text-left flex items-start gap-6 p-5 cursor-pointer transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-gray-300 dark:focus-visible:ring-gray-700 hover:bg-gray-50 dark:hover:bg-gray-700/30 ${
                    idx !== history.length - 1 ? 'border-b border-gray-100 dark:border-gray-700/50' : ''
                  }`
                }
              >
                <span className="text-xs font-medium text-gray-400 w-16 pt-1">
                  {timeFormatter.format(item.ts)}
                </span>
                <p className="flex-1 text-gray-700 dark:text-gray-300 leading-relaxed pr-2">{item.text}</p>
                <span className="flex items-center pt-1 text-gray-300 dark:text-gray-500 opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100 transition-opacity">
                  {copiedId === item.id ? (
                    <Check className="w-4 h-4 text-green-600 dark:text-green-400" />
                  ) : (
                    <Copy className="w-4 h-4" />
                  )}
                </span>
              </button>
            ))
          )}
          
        </div>
      </div>

    </div>
  );
};
