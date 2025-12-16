// src/components/settings/SettingsRow.tsx
import React from 'react';

interface SettingsRowProps {
  label: string;
  description?: React.ReactNode; // Made optional (?)
  children?: React.ReactNode;    // The right-side content (Button, Switch, etc.)
  isLast?: boolean;
}

export const SettingsRow: React.FC<SettingsRowProps> = ({ 
  label, 
  description, 
  children, 
  isLast = false 
}) => {
  return (
    <div className={`flex items-center justify-between py-5 ${!isLast ? 'border-b border-gray-100 dark:border-gray-700/50' : ''}`}>
      
      {/* Left Side: Label & Optional Description */}
      <div className="space-y-1">
        <h3 className={`text-sm font-medium ${description ? 'text-gray-900 dark:text-gray-100' : 'text-gray-900 dark:text-gray-100'}`}>
          {label}
        </h3>
        
        {/* Only render description if it exists */}
        {description && (
          <div className="text-sm text-gray-500 dark:text-gray-400">
            {description}
          </div>
        )}
      </div>

      {/* Right Side: The Control (Button, Switch, or nothing) */}
      <div className="flex-shrink-0 ml-4">
        {children}
      </div>
      
    </div>
  );
};