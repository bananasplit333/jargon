// src/components/layout/AppLayout.tsx
import React from 'react';
import { Sidebar } from './Sidebar';
import { MainPage } from '../pages/MainPage';
import { Routes, Route } from 'react-router-dom';


export const AppLayout: React.FC = () => {
	return (
		<div className="flex h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 overflow-hidden">
			<aside className="w-64 flex-shrink-0">
				<Sidebar />
			</aside>

			<main className="flex-1 overflow-y-auto min-w-0">
				<div className="container mx-auto px-6 py-8 max-w-5xl">
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
