import sherpa_onnx
import sounddevice as sd
import keyboard
import pyautogui
import queue
import numpy as np

# --- CONFIGURATION ---
MODEL_DIR = "./parakeet_model"  
HOTKEY = "ctrl+shift"
SAMPLE_RATE = 16000

print("Initializing Parakeet (Sherpa-ONNX)...")

recognizer = sherpa_onnx.OfflineRecognizer.from_transducer(
    encoder=f"{MODEL_DIR}/encoder.int8.onnx",
    decoder=f"{MODEL_DIR}/decoder.int8.onnx",
    joiner=f"{MODEL_DIR}/joiner.int8.onnx",
    tokens=f"{MODEL_DIR}/tokens.txt",
    sample_rate=SAMPLE_RATE,
    model_type="nemo_transducer",
    num_threads=4,
    provider="cpu",
)

print(recognizer)

print(f"Ready! Hold '{HOTKEY}' to record.")

audio_queue = queue.Queue()

def callback(indata, frames, time, status):
    """Capture audio and push to queue"""
    audio_queue.put(indata.copy())

while True:
    keyboard.wait(HOTKEY)
    print("Recording...")
    
    # Start recording
    with sd.InputStream(samplerate=SAMPLE_RATE, channels=1, callback=callback):
        while keyboard.is_pressed(HOTKEY):
            sd.sleep(50)
    
    # Process Audio
    print("Processing...")
    samples = []
    while not audio_queue.empty():
        samples.append(audio_queue.get())
    
    if not samples:
        continue

    # Flatten audio for the model
    audio_data = np.concatenate(samples, axis=0).flatten()
    
    # Inference 
    stream = recognizer.create_stream()
    stream.accept_waveform(SAMPLE_RATE, audio_data)
    recognizer.decode_stream(stream)
    result = stream.result.text

    if result:
        print(f"Typing: {result}")
        pyautogui.write(result + " ")
