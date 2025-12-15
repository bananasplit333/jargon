// src/components/layout/AppLayout.tsx
import React, { useCallback, useEffect, useState } from 'react';
import { Routes, Route } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import { Menu } from 'lucide-react';
import { Sidebar } from './Sidebar';
import { MainPage } from '../pages/MainPage';

const COLLAPSE_BREAKPOINT = 960;


export const AppLayout: React.FC = () => {
	const [sidebarOpen, setSidebarOpen] = useState(true);
	const [isCompact, setIsCompact] = useState(false);

	useEffect(() => {
		if (typeof window === 'undefined') {
			return;
		}

		const handleResize = () => {
			const compact = window.innerWidth < COLLAPSE_BREAKPOINT;
			setIsCompact(compact);
			setSidebarOpen(compact ? false : true);
		};

		handleResize();
		window.addEventListener('resize', handleResize);
		return () => window.removeEventListener('resize', handleResize);
	}, []);

	const toggleSidebar = useCallback(() => {
		setSidebarOpen((prev) => !prev);
	}, []);

	const openOverlay = useCallback(() => {
		if ((window as any).__TAURI__) {
			invoke('overlay_show', { show: true }).catch((err) => {
				console.error('overlay_show failed', err);
			});
		} else {
			console.warn('Overlay button available in desktop build.');
		}
	}, []);
	return (
		<div className="flex h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 overflow-hidden">
			<aside
				className={`flex-shrink-0 transition-all duration-200 overflow-hidden ${
					sidebarOpen ? 'w-64' : 'w-0'
				}`}
			>
				{sidebarOpen && <Sidebar />}
			</aside>

			<main className="flex-1 min-w-0 overflow-hidden">
				<div className="flex items-center px-6 pt-6 gap-3">
					<button
						onClick={toggleSidebar}
						type="button"
						className="h-10 w-10 flex items-center justify-center rounded-full border border-gray-300 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition"
						aria-label={sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
						aria-pressed={sidebarOpen}
					>
						<Menu className={`h-5 w-5 transition-transform ${sidebarOpen ? 'rotate-90' : ''}`} />
					</button>
				</div>
				<div className="container mx-auto px-6 py-6 max-w-5xl">
					<Routes>
						<Route path="/" element={<MainPage />} />
						<Route path="/dictionary" element={<div>Dictionary</div>} />
						<Route path="/snippets" element={<div>Snippets</div>} />
						<Route path="/style" element={<div>Style</div>} />
						<Route path="/notes" element={<div>Notes</div>} />
					</Routes>
				</div>
			</main>
		</div>
	);
};
