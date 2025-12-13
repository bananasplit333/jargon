import sherpa_onnx
import sounddevice as sd
from pynput import keyboard
import pyautogui
import queue
import numpy as np
import threading

# --- CONFIGURATION ---
MODEL_DIR = "./parakeet_model"
MODEL_SAMPLE_RATE = 16000
HOTKEY = {keyboard.Key.ctrl_l, keyboard.Key.shift}

# --- STATE ---
pressed = set()
recording = False
audio_queue = queue.Queue()
audio_stream = None
input_sample_rate = MODEL_SAMPLE_RATE
lock = threading.Lock()

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

print("Ready! Hold CTRL + SHIFT to record.")

# --- AUDIO CALLBACK ---
def audio_callback(indata, frames, time, status):
    if status:
        print(status)
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
            print(f"Recording at {input_sample_rate} Hz...")
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
    print("Processing...")
    process_audio()

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
        print(f"Typing: {result}")
        pyautogui.write(result + " ")

# --- HOTKEY HANDLERS ---
def on_press(key):
    global recording
    if key in HOTKEY:
        pressed.add(key)
        if not recording and HOTKEY.issubset(pressed):
            if start_recording():
                recording = True

def on_release(key):
    global recording
    if key in pressed:
        pressed.remove(key)
    if recording and not HOTKEY.issubset(pressed):
        recording = False
        stop_recording()

# --- MAIN LOOP ---
def main():
    # Start the hotkey listener and block so the process stays alive.
    listener = keyboard.Listener(on_press=on_press, on_release=on_release)
    listener.start()
    listener.join()  # wait forever until the program is killed


if __name__ == "__main__":
    main()
