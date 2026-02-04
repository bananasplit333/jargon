import os
import sys
import numpy as np

ROOT = os.path.dirname(os.path.dirname(__file__))
if ROOT not in sys.path:
    sys.path.append(ROOT)

from audio_utils import resample_audio


def test_resample_audio_identity():
    audio = np.array([0.0, 0.5, -0.5, 1.0], dtype=np.float32)
    out = resample_audio(audio, 16000, 16000)
    assert np.allclose(out, audio)


def test_resample_audio_downsample():
    audio = np.linspace(-1.0, 1.0, num=160, dtype=np.float32)
    out = resample_audio(audio, 16000, 8000)
    assert out.shape[0] == 80


def test_resample_audio_upsample():
    audio = np.linspace(-1.0, 1.0, num=80, dtype=np.float32)
    out = resample_audio(audio, 8000, 16000)
    assert out.shape[0] == 160
