print("[python] Engine starting…", flush=True)
import sys
import argparse
import sherpa_onnx
import sounddevice as sd
from pynput import keyboard
import json
import pyautogui
import queue
import numpy as np
import threading
import time
import ctypes
from ctypes import wintypes

# --- CONFIGURATION (defaults; override via CLI args) ---
MODEL_DIR = "../data/parakeet_model"
MODEL_SAMPLE_RATE = 16000

# Accept either left/right Ctrl + left/right Shift as the hotkey
CTRL_KEYS = {keyboard.Key.ctrl_l, keyboard.Key.ctrl_r}
SHIFT_KEYS = {keyboard.Key.shift, keyboard.Key.shift_l, keyboard.Key.shift_r}
TYPE_INTO_ACTIVE_APP = True

# --- STATE ---
pressed = set()
recording = False
hotkey_active = False
audio_queue = queue.Queue()
audio_stream = None
input_sample_rate = MODEL_SAMPLE_RATE
lock = threading.Lock()
level_lock = threading.Lock()
latest_level = 0.0
level_thread = None
level_stop_event = threading.Event()

print("Initializing Parakeet (Sherpa-ONNX)...")

recognizer = sherpa_onnx.OfflineRecognizer.from_transducer(
    encoder=f"{MODEL_DIR}/encoder.int8.onnx",
    decoder=f"{MODEL_DIR}/decoder.int8.onnx",
    joiner=f"{MODEL_DIR}/joiner.int8.onnx",
    tokens=f"{MODEL_DIR}/tokens.txt",
    sample_rate=MODEL_SAMPLE_RATE,
    model_type="nemo_transducer",
    num_threads=4,
    provider="cpu",
)

print("onionsonsale!")

# --- AUDIO CALLBACK ---
def audio_callback(indata, frames, time, status):
    if status:
        print(status)
    try:
        rms = float(np.sqrt(np.mean(indata.astype(np.float32) ** 2)))
    except Exception:
        rms = 0.0
    # Gain + soft compression to make quiet voices visible.
    boosted = rms * 22.0
    level = 1.0 - np.exp(-boosted)
    level = float(min(1.0, max(0.0, level)))
    global latest_level
    with level_lock:
        latest_level = level
    audio_queue.put(indata.copy())


def get_input_sample_rate():
    try:
        device_info = sd.query_devices(kind="input")
        return int(device_info.get("default_samplerate", MODEL_SAMPLE_RATE))
    except Exception as exc:
        print(f"Warning: defaulting to {MODEL_SAMPLE_RATE} Hz; failed to read input sample rate: {exc}")
        return MODEL_SAMPLE_RATE


def resample_audio(audio, src_rate, target_rate):
    if src_rate == target_rate or audio.size == 0:
        return audio
    duration = audio.shape[0] / float(src_rate)
    target_length = max(1, int(duration * target_rate))
    src_times = np.linspace(0, duration, num=audio.shape[0], endpoint=False)
    target_times = np.linspace(0, duration, num=target_length, endpoint=False)
    return np.interp(target_times, src_times, audio).astype(np.float32)


_WIN_CLIPBOARD_API_INITIALIZED = False


def _init_win_clipboard_api(user32, kernel32) -> None:
    global _WIN_CLIPBOARD_API_INITIALIZED
    if _WIN_CLIPBOARD_API_INITIALIZED:
        return

    user32.OpenClipboard.argtypes = [wintypes.HWND]
    user32.OpenClipboard.restype = wintypes.BOOL

    user32.CloseClipboard.argtypes = []
    user32.CloseClipboard.restype = wintypes.BOOL

    user32.EmptyClipboard.argtypes = []
    user32.EmptyClipboard.restype = wintypes.BOOL

    user32.GetClipboardData.argtypes = [wintypes.UINT]
    user32.GetClipboardData.restype = wintypes.HANDLE

    user32.SetClipboardData.argtypes = [wintypes.UINT, wintypes.HANDLE]
    user32.SetClipboardData.restype = wintypes.HANDLE

    kernel32.GlobalLock.argtypes = [wintypes.HGLOBAL]
    kernel32.GlobalLock.restype = wintypes.LPVOID

    kernel32.GlobalUnlock.argtypes = [wintypes.HGLOBAL]
    kernel32.GlobalUnlock.restype = wintypes.BOOL

    kernel32.GlobalSize.argtypes = [wintypes.HGLOBAL]
    kernel32.GlobalSize.restype = ctypes.c_size_t

    kernel32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
    kernel32.GlobalAlloc.restype = wintypes.HGLOBAL

    kernel32.GlobalFree.argtypes = [wintypes.HGLOBAL]
    kernel32.GlobalFree.restype = wintypes.HGLOBAL

    _WIN_CLIPBOARD_API_INITIALIZED = True


def _win_clipboard_get_text():
    CF_UNICODETEXT = 13
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _init_win_clipboard_api(user32, kernel32)

    if not user32.OpenClipboard(0):
        return None
    try:
        handle = user32.GetClipboardData(CF_UNICODETEXT)
        if not handle:
            return None
        locked = kernel32.GlobalLock(handle)
        if not locked:
            return None
        try:
            size_bytes = int(kernel32.GlobalSize(handle) or 0)
            if size_bytes <= 1:
                return ""
            wchar_count = max(1, size_bytes // ctypes.sizeof(ctypes.c_wchar))
            # Read bounded length to avoid scanning past buffer.
            text = ctypes.wstring_at(locked, wchar_count)
            return text.rstrip("\x00")
        finally:
            kernel32.GlobalUnlock(handle)
    finally:
        user32.CloseClipboard()


def _win_clipboard_set_text(text: str) -> bool:
    CF_UNICODETEXT = 13
    GMEM_MOVEABLE = 0x0002

    user32 = ctypes.WinDLL("user32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _init_win_clipboard_api(user32, kernel32)

    if not user32.OpenClipboard(0):
        return False
    try:
        if not user32.EmptyClipboard():
            return False

        buf = ctypes.create_unicode_buffer(text)
        size_bytes = ctypes.sizeof(buf)

        hglob = kernel32.GlobalAlloc(GMEM_MOVEABLE, size_bytes)
        if not hglob:
            return False
        locked = kernel32.GlobalLock(hglob)
        if not locked:
            kernel32.GlobalFree(hglob)
            return False
        try:
            ctypes.memmove(locked, buf, size_bytes)
        finally:
            kernel32.GlobalUnlock(hglob)

        if not user32.SetClipboardData(CF_UNICODETEXT, hglob):
            kernel32.GlobalFree(hglob)
            return False
        return True
    finally:
        user32.CloseClipboard()


def paste_into_active_app(text: str) -> None:
    if sys.platform.startswith("win"):
        previous = None
        try:
            previous = _win_clipboard_get_text()
        except Exception:
            previous = None
        try:
            for _ in range(8):
                if _win_clipboard_set_text(text):
                    print("[python] Paste method: clipboard", flush=True)
                    pyautogui.hotkey("ctrl", "v")
                    time.sleep(0.06)
                    if previous is not None:
                        for _ in range(5):
                            if _win_clipboard_set_text(previous):
                                break
                            time.sleep(0.01)
                    return
                time.sleep(0.01)
        except Exception:
            pass

        print("[python] Paste method: fallback-typing", file=sys.stderr, flush=True)
    pyautogui.write(text)

# --- RECORDING CONTROL ---
def start_recording():
    global audio_stream, input_sample_rate
    with lock:
        audio_queue.queue.clear()
        input_sample_rate = get_input_sample_rate()
        try:
            audio_stream = sd.InputStream(
                samplerate=input_sample_rate,
                channels=1,
                callback=audio_callback,
            )
            audio_stream.start()
            start_level_emitter()
            return True
        except Exception as exc:
            audio_stream = None
            print(f"Unable to start recording (sample rate {input_sample_rate}): {exc}")
            return False

def stop_recording():
    global audio_stream
    with lock:
        if audio_stream:
            audio_stream.stop()
            audio_stream.close()
            audio_stream = None
    stop_level_emitter()
    print("Processing/.......")
    process_audio()


def start_level_emitter():
    global level_thread, latest_level
    stop_level_emitter()
    level_stop_event.clear()
    with level_lock:
        latest_level = 0.0

    def _emit_loop():
        smoothed = 0.0
        while not level_stop_event.is_set():
            with level_lock:
                target = latest_level

            if target > smoothed:
                smoothed = smoothed * 0.55 + target * 0.45
            else:
                smoothed = smoothed * 0.85 + target * 0.15

            sys.stdout.write(json.dumps({"type": "overlay_level", "level": smoothed}) + "\n")
            sys.stdout.flush()
            time.sleep(0.04)

    level_thread = threading.Thread(target=_emit_loop, daemon=True)
    level_thread.start()


def stop_level_emitter():
    global level_thread, latest_level
    level_stop_event.set()
    if level_thread is not None:
        level_thread.join(timeout=0.25)
        level_thread = None
    with level_lock:
        latest_level = 0.0
    sys.stdout.write(json.dumps({"type": "overlay_level", "level": 0.0}) + "\n")
    sys.stdout.flush()

# --- AUDIO PROCESSING ---
def process_audio():
    samples = []
    while not audio_queue.empty():
        samples.append(audio_queue.get())

    if not samples:
        return

    audio_data = np.concatenate(samples, axis=0).flatten()
    audio_data = resample_audio(audio_data, input_sample_rate, MODEL_SAMPLE_RATE)
    if input_sample_rate != MODEL_SAMPLE_RATE:
        print(f"Resampled from {input_sample_rate} Hz to {MODEL_SAMPLE_RATE} Hz")

    stream = recognizer.create_stream()
    stream.accept_waveform(MODEL_SAMPLE_RATE, audio_data)
    recognizer.decode_stream(stream)

    result = stream.result.text.strip()
    if result:
        sys.stdout.write(json.dumps({"type": "transcript", "text": result}) + "\n")
        sys.stdout.flush()
        if TYPE_INTO_ACTIVE_APP:
            try:
                paste_into_active_app(result + " ")
            except Exception as exc:
                print(f"[python] Warning: failed to paste into active app: {exc}", file=sys.stderr, flush=True)

# --- HOTKEY HANDLERS ---
def is_hotkey_pressed() -> bool:
    return (any(k in pressed for k in CTRL_KEYS) and any(k in pressed for k in SHIFT_KEYS))


def on_press(key):
    global recording, hotkey_active
    pressed.add(key)
    if not hotkey_active and is_hotkey_pressed():
        hotkey_active = True
        # Try to start recording, but expand overlay regardless of success
        if not recording and start_recording():
            recording = True
        sys.stdout.write(json.dumps({"type": "overlay", "hover": True}) + "\n")
        sys.stdout.flush()


def on_release(key):
    global recording, hotkey_active
    if key in pressed:
        pressed.remove(key)
    if hotkey_active and not is_hotkey_pressed():
        hotkey_active = False
        if recording:
            recording = False
            stop_recording()
        # Signal overlay collapse regardless
        sys.stdout.write(json.dumps({"type": "overlay", "hover": False}) + "\n")
        sys.stdout.flush()

# --- MAIN LOOP ---
def main():
    global MODEL_DIR, TYPE_INTO_ACTIVE_APP
    # Parse command-line arguments from Tauri
    parser = argparse.ArgumentParser(description="Speech-to-text engine")
    parser.add_argument("--hotkey", type=str, help="Hotkey combination (ignored for now; hardcoded Ctrl+Shift)")
    parser.add_argument("--model-dir", type=str, default=MODEL_DIR, help="Path to the ONNX model directory")
    parser.add_argument("--type-into-active-app", type=str, default="true", help="Type into active app (true/false)")
    args = parser.parse_args()
    
    MODEL_DIR = args.model_dir
    TYPE_INTO_ACTIVE_APP = args.type_into_active_app.lower() == "true"
    
    print(f"[python] Model dir: {MODEL_DIR}", flush=True)
    print(f"[python] Type into active app: {TYPE_INTO_ACTIVE_APP}", flush=True)
    
    # Start the hotkey listener and block so the process stays alive.
    listener = keyboard.Listener(on_press=on_press, on_release=on_release)
    listener.start()
    listener.join()  # wait forever until the program is killed


if __name__ == "__main__":
    main()
