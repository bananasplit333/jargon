import React, { useCallback, useEffect, useState } from 'react';

// --- 1. Reusable UI Components ---

// The Universal Row (Handles both Buttons and Toggles)
interface SettingsRowProps {
  label: string;
  description?: React.ReactNode;
  children?: React.ReactNode;
  isLast?: boolean;
}

const SettingsRow: React.FC<SettingsRowProps> = ({ 
  label, 
  description, 
  children, 
  isLast = false 
}) => {
  return (
    <div className={`flex items-center justify-between py-5 ${!isLast ? 'border-b border-gray-100 dark:border-gray-700/50' : ''}`}>
      <div className="space-y-1 pr-4">
        <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100">
          {label}
        </h3>
        {description && (
          <div className="text-sm text-gray-500 dark:text-gray-400">
            {description}
          </div>
        )}
      </div>
      <div className="flex-shrink-0">
        {children}
      </div>
    </div>
  );
};

// The Toggle Switch (Green/Gray Animation)
const ToggleSwitch: React.FC<{ checked: boolean; onChange: (val: boolean) => void }> = ({ checked, onChange }) => (
  <button
    onClick={() => onChange(!checked)}
    className={`
      relative inline-flex h-7 w-12 items-center rounded-full transition-colors duration-200 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-2
      ${checked ? 'bg-green-500' : 'bg-gray-200 dark:bg-gray-700'}
    `}
  >
    <span
      className={`
        inline-block h-5 w-5 transform rounded-full bg-white shadow transition duration-200 ease-in-out
        ${checked ? 'translate-x-6' : 'translate-x-1'}
      `}
    />
  </button>
);

// The "Change" Button
const ChangeButton = ({ onClick }: { onClick?: () => void }) => (
  <button onClick={onClick} className="px-6 py-2 bg-gray-100 hover:bg-gray-200 dark:bg-gray-800 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-200 text-sm font-medium rounded-lg transition-colors duration-200">
    Change
  </button>
);


// --- 2. Page Sections ---

const GeneralSection = () => (
  <div className="space-y-4">
    <h2 className="text-2xl font-semibold text-gray-900 dark:text-white">General</h2>
    <div className="bg-white dark:bg-gray-900 rounded-xl border border-gray-200 dark:border-gray-800 shadow-sm px-6">
      <SettingsRow 
        label="Keyboard shortcuts" 
        description={<span className="flex items-center gap-1">Hold <strong className="text-gray-700 dark:text-gray-300 font-semibold">Ctrl + Shift</strong> and speak. <a href="#" className="ml-1 hover:underline">Learn more →</a></span>}
      >
        <ChangeButton />
      </SettingsRow>
      <SettingsRow label="Microphone" description="Auto-detect">
        <ChangeButton />
      </SettingsRow>
      <SettingsRow label="Languages" description="English" isLast={true}>
        <ChangeButton />
      </SettingsRow>
    </div>
  </div>
);

const SystemSection = () => {
  // State for toggles
  const [launchAtLogin, setLaunchAtLogin] = useState(true);
  const [showBar, setShowBar] = useState(true);
  const [showInDock, setShowInDock] = useState(true);
  const [soundEffects, setSoundEffects] = useState(true);
  const [muteMusic, setMuteMusic] = useState(true);
  const [isTauri, setIsTauri] = useState(false);

  useEffect(() => {
    let active = true;
    (async () => {
      try {
        const core = await import('@tauri-apps/api/core');
        const tauri = Boolean(core.isTauri?.() ?? true);
        if (!active) {
          return;
        }
        setIsTauri(tauri);
        if (!tauri) {
          return;
        }
        const enabled = await core.invoke<boolean>('sound_get_enabled');
        if (active) {
          setSoundEffects(Boolean(enabled));
        }
      } catch {
        if (active) {
          setIsTauri(false);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  const handleSoundEffectsChange = useCallback(
    (next: boolean) => {
      setSoundEffects(next);
      if (!isTauri) {
        return;
      }
      void (async () => {
        try {
          const core = await import('@tauri-apps/api/core');
          await core.invoke('sound_set_enabled', { enabled: next });
        } catch (err) {
          console.warn('Failed to update sound effects setting', err);
        }
      })();
    },
    [isTauri],
  );

  return (
    <div className="space-y-2">
      <h2 className="text-2xl font-semibold text-gray-900 dark:text-white">System</h2>

      {/* App Settings Sub-Group */}
      <div className="space-y-3">
        <h3 className="text-sm font-medium text-gray-500 dark:text-gray-400 pl-1">App settings</h3>
        <div className="bg-white dark:bg-gray-900 rounded-xl border border-gray-200 dark:border-gray-800 shadow-sm px-6">
          <SettingsRow label="Launch app at login">
            <ToggleSwitch checked={launchAtLogin} onChange={setLaunchAtLogin} />
          </SettingsRow>
          <SettingsRow label="Show Flow bar at all times">
            <ToggleSwitch checked={showBar} onChange={setShowBar} />
          </SettingsRow>
          <SettingsRow label="Show app in dock" isLast={true}>
            <ToggleSwitch checked={showInDock} onChange={setShowInDock} />
          </SettingsRow>
        </div>
      </div>

      {/* Sound Sub-Group */}
      <div className="space-y-3">
        <h3 className="text-sm font-medium text-gray-500 dark:text-gray-400 pl-1">Sound</h3>
        <div className="bg-white dark:bg-gray-900 rounded-xl border border-gray-200 dark:border-gray-800 shadow-sm px-6">
          <SettingsRow label="Dictation sound effects">
            <ToggleSwitch checked={soundEffects} onChange={handleSoundEffectsChange} />
          </SettingsRow>
          <SettingsRow label="Mute music while dictating" isLast={true}>
            <ToggleSwitch checked={muteMusic} onChange={setMuteMusic} />
          </SettingsRow>
        </div>
      </div>
    </div>
  );
};


// --- 3. Main Page Component ---

export const SettingsPage: React.FC = () => {
  return (
    <div className="max-w-3xl mx-auto space-y-8 pb-20">
      <GeneralSection />
      <div className="border-t border-gray-200 dark:border-gray-800 my-8" /> {/* Divider */}
      <SystemSection />
    </div>
  );
};
