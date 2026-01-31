// src/components/layout/AppLayout.tsx
import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Routes, Route } from 'react-router-dom';
import { Menu, Minus, Square, X } from 'lucide-react';
import { Sidebar } from './Sidebar';
import { MainPage } from '../pages/MainPage';
import { SettingsPage } from '../pages/SettingsPage';

const COLLAPSE_BREAKPOINT = 960;
const DICTATION_DING_SRC = '/data/sounds/dictation-start.wav';

type DictationSoundState = {
	active: boolean;
	lastPlayedMs: number;
};

const getDictationSoundState = () => {
	const win = window as Window & { __jargonDictationSound?: DictationSoundState };
	if (!win.__jargonDictationSound) {
		win.__jargonDictationSound = { active: false, lastPlayedMs: 0 };
	}
	return win.__jargonDictationSound;
};

export const AppLayout: React.FC = () => {
	const [sidebarOpen, setSidebarOpen] = useState(true);
	const [isTauri, setIsTauri] = useState(false);
	const scrollContainerRef = useRef<HTMLDivElement | null>(null);
	const dingAudioRef = useRef<HTMLAudioElement | null>(null);

	useEffect(() => {
		if (typeof window === 'undefined') {
			return;
		}

		(async () => {
			try {
				const core = await import('@tauri-apps/api/core');
				setIsTauri(Boolean(core.isTauri?.() ?? true));
			} catch {
				setIsTauri(false);
			}
		})();

		const handleResize = () => {
			const compact = window.innerWidth < COLLAPSE_BREAKPOINT;
			setSidebarOpen(compact ? false : true);
		};

		handleResize();
		window.addEventListener('resize', handleResize);
		return () => window.removeEventListener('resize', handleResize);
	}, []);

	useEffect(() => {
		if (typeof window === 'undefined') {
			return;
		}

		let cancelled = false;
		let unlisten: null | (() => void) = null;
		dingAudioRef.current = new Audio(DICTATION_DING_SRC);
		dingAudioRef.current.preload = 'auto';
		dingAudioRef.current.onerror = (e) => console.error('[sound] audio load error:', e);
		dingAudioRef.current.oncanplaythrough = () => console.log('[sound] audio loaded and ready');
		const dictationState = getDictationSoundState();

		const playDing = async () => {
			const audio = dingAudioRef.current;
			if (!audio) {
				console.log('[sound] no audio element');
				return;
			}
			const now = Date.now();
			if (now - dictationState.lastPlayedMs < 200) {
				console.log('[sound] debounced - too soon');
				return;
			}
			dictationState.lastPlayedMs = now;
			audio.currentTime = 0;
			try {
				await audio.play();
			} catch (err) {
				console.error('[sound] playback error:', err);
			}
		};

		(async () => {
			try {
				const core = await import('@tauri-apps/api/core');
				const event = await import('@tauri-apps/api/event');
				const stopStartListening = await event.listen('stt:dictation-start', async () => {
					console.log('[sound] dictation-start event received');
					if (dictationState.active) {
						console.log('[sound] ignoring - already active');
						return;
					}
					dictationState.active = true;
					try {
						const enabled = await core.invoke<boolean>('sound_get_enabled');
						console.log('[sound] sound_get_enabled:', enabled);
						if (!enabled) {
							return;
						}
						console.log('[sound] playing ding...');
						await playDing();
						console.log('[sound] ding played successfully');
					} catch (err) {
						console.warn('Failed to play dictation sound', err);
					}
				});
				const stopStopListening = await event.listen('stt:dictation-stop', () => {
					dictationState.active = false;
				});
				if (cancelled) {
					stopStartListening();
					stopStopListening();
					return;
				}
				unlisten = () => {
					stopStartListening();
					stopStopListening();
				};
			} catch (err) {
				console.warn('Dictation sound listener not available in this environment', err);
			}
		})();

		return () => {
			cancelled = true;
			if (unlisten) {
				unlisten();
			}
			dingAudioRef.current = null;
		};
	}, []);

	useEffect(() => {
		const scrollContainer = scrollContainerRef.current;
		if (!scrollContainer) {
			return;
		}

		let timeoutId: number | undefined;
		const onScroll = () => {
			scrollContainer.setAttribute('data-scrolling', 'true');
			window.clearTimeout(timeoutId);
			timeoutId = window.setTimeout(() => {
				scrollContainer.removeAttribute('data-scrolling');
			}, 800);
		};

		scrollContainer.addEventListener('scroll', onScroll, { passive: true });
		return () => {
			scrollContainer.removeEventListener('scroll', onScroll);
			window.clearTimeout(timeoutId);
		};
	}, []);

	const toggleSidebar = useCallback(() => {
		setSidebarOpen((prev) => !prev);
	}, []);

	const minimize = useCallback(async () => {
		if (!isTauri) return;
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		await getCurrentWindow().minimize();
	}, [isTauri]);

	const toggleMaximize = useCallback(async () => {
		if (!isTauri) return;
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		await getCurrentWindow().toggleMaximize();
	}, [isTauri]);

	const close = useCallback(async () => {
		if (!isTauri) return;
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		await getCurrentWindow().close();
	}, [isTauri]);

	return (
		<div className="flex h-screen bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100 overflow-hidden">
			<aside
				className={`flex-shrink-0 transition-all duration-200 overflow-hidden ${sidebarOpen ? 'w-64' : 'w-0'
					}`}
			>
				{sidebarOpen && <Sidebar />}
			</aside>

			<main className="flex flex-col flex-1 min-w-0 overflow-hidden">
				<div className="flex items-center px-4 h-12 gap-3 border-b border-gray-200 dark:border-gray-800 bg-gray-50/80 dark:bg-gray-900/80 backdrop-blur">
					<button
						onClick={toggleSidebar}
						type="button"
						data-tauri-drag-region="false"
						className="h-9 w-9 flex items-center justify-center rounded-full border border-gray-300 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition"
						aria-label={sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
						aria-pressed={sidebarOpen}
					>
						<Menu className={`h-5 w-5 transition-transform ${sidebarOpen ? 'rotate-90' : ''}`} />
					</button>

					<div
						className="flex-1 h-full flex items-center justify-center select-none text-xs font-medium tracking-wide text-gray-500 dark:text-gray-400 cursor-grab active:cursor-grabbing"
						data-tauri-drag-region
						aria-label="Window drag region"
					>
						Jargon
					</div>

					{isTauri && (
						<div className="flex items-center gap-1" data-tauri-drag-region="false">
							<button
								type="button"
								aria-label="Minimize"
								onClick={minimize}
								className="h-9 w-9 flex items-center justify-center rounded-md hover:bg-gray-100 dark:hover:bg-gray-800 transition"
							>
								<Minus className="h-4 w-4" />
							</button>
							<button
								type="button"
								aria-label="Maximize"
								onClick={toggleMaximize}
								className="h-9 w-9 flex items-center justify-center rounded-md hover:bg-gray-100 dark:hover:bg-gray-800 transition"
							>
								<Square className="h-4 w-4" />
							</button>
							<button
								type="button"
								aria-label="Close"
								onClick={close}
								className="h-9 w-9 flex items-center justify-center rounded-md hover:bg-red-50 dark:hover:bg-red-900/30 transition"
							>
								<X className="h-4 w-4" />
							</button>
						</div>
					)}
				</div>
				<div
					ref={scrollContainerRef}
					className="flex-1 min-h-0 overflow-y-auto scrollbar-thin scrollbar-auto-hide scrollbar-thumb-gray-300 dark:scrollbar-thumb-gray-700 scrollbar-track-transparent"
				>
					<div className="container mx-auto px-6 py-6 max-w-5xl">
						<Routes>
							<Route path="/" element={<MainPage />} />
							<Route path="/dictionary" element={<div>Dictionary</div>} />
							<Route path="/settings" element={<div><SettingsPage /></div>} />
							<Route path="/style" element={<div>Style</div>} />
							<Route path="/notes" element={<div>Notes</div>} />
						</Routes>
					</div>
				</div>
			</main>
		</div>
	);
};
