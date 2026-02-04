import numpy as np

def resample_audio(audio: np.ndarray, src_rate: int, target_rate: int) -> np.ndarray:
    if src_rate == target_rate or audio.size == 0:
        return audio
    duration = audio.shape[0] / float(src_rate)
    target_length = max(1, int(duration * target_rate))
    src_times = np.linspace(0, duration, num=audio.shape[0], endpoint=False)
    target_times = np.linspace(0, duration, num=target_length, endpoint=False)
    return np.interp(target_times, src_times, audio).astype(np.float32)
