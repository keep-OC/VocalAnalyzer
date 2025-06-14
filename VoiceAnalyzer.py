import tkinter as tk
from tkinter import ttk, messagebox
import numpy as np
import sounddevice as sd
from pythonosc.udp_client import SimpleUDPClient
import threading, collections, time, sys

###############################################################################
# --- ユーザ設定 ---------------------------------------------------------------
OSC_IP   = "127.0.0.1"
OSC_PORT = 9000
BUFFER_SECONDS = 0.2        # リングバッファ長 (秒)
FPS = 90                    # 解析・送信レート (>=60)
HARMONICS = 10
DEBUG = True
# -----------------------------------------------------------------------------

# --- 送信先パラメータ名 ------------------------------------------------------- ### CHANGED
PARAM_F0_L  = "/avatar/parameters/F0_L"
PARAM_F0_H  = "/avatar/parameters/F0_H"
PARAM_GAIN  = [f"/avatar/parameters/G{i}" for i in range(1, HARMONICS + 1)]
# -----------------------------------------------------------------------------


def default_f0_estimator(wave, sr):
    """ごく簡易なオートコリレーション基音推定（差し替え前提）"""
    wave = wave - np.mean(wave)
    corr = np.correlate(wave, wave, mode="full")[len(wave)-1:]
    d = np.diff(corr)
    idx = np.flatnonzero(d > 0)
    if idx.size == 0:
        return 0.0
    peak = np.argmax(corr[idx[0]:]) + idx[0]
    return 0.0 if peak == 0 else sr / peak


class RingBuffer:
    def __init__(self, size):
        self.size = size
        self.buf = collections.deque(maxlen=size)
        self.lock = threading.Lock()

    def extend(self, data):
        with self.lock:
            self.buf.extend(data)

    def get(self):
      # コピーを極力抑えつつ NumPy 配列化（NumPy 2.0 対応） ### CHANGED
        with self.lock:
            return np.asarray(self.buf, dtype=np.float32)


class VoiceAnalyzer(threading.Thread):
    def __init__(self, ring, sr, osc_client, f0_func):
        super().__init__(daemon=True)
        self.ring, self.sr, self.osc, self.f0_func = ring, sr, osc_client, f0_func
        self.running = threading.Event(); self.running.set()

    def run(self):
        hop = 1.0 / FPS
        next_ts = time.perf_counter()

        while self.running.is_set():
            now = time.perf_counter()
            if now < next_ts:
                time.sleep(next_ts - now)
                continue
            next_ts += hop

            samples = self.ring.get()
            if samples.size < 1024:          # 十分たまるまで待つ
                continue

            # --- FFT & 基音 --------------------------------------------------
            spec  = np.abs(np.fft.rfft(samples))
            freqs = np.fft.rfftfreq(samples.size, 1 / self.sr)

            f0 = float(self.f0_func(samples, self.sr))  # [Hz]
            f0_q = max(0, min(65535, int(round(f0))))   # 16-bit 量子化 ### CHANGED

            # 基音を下位/上位 8bit に分割して送信 ### CHANGED
            self.osc.send_message(PARAM_F0_L, f0_q & 0xFF)
            self.osc.send_message(PARAM_F0_H, (f0_q >> 8) & 0xFF)

            # --- 倍音ゲイン --------------------------------------------------
            debug_parts = [f"f0={f0:7.2f}Hz"] if DEBUG else None
            if f0 > 0:
                spec_max = np.max(spec) or 1.0
                for i in range(1, HARMONICS + 1):
                    target = f0 * i
                    idx    = int(np.argmin(np.abs(freqs - target)))
                    amp    = spec[idx] / spec_max          # 0-1
                    gain_q = max(0, min(255, int(round(amp * 255))))  # Int 0-255 ### CHANGED
                    self.osc.send_message(PARAM_GAIN[i-1], gain_q)    # Int 送信 ### CHANGED
                    if DEBUG:
                        debug_parts.append(f"h{i}:{gain_q:3d}")

            # --- Console 出力 ----------------------------------------------
            if DEBUG:
                print(" | ".join(debug_parts), file=sys.stderr, flush=True)

    def stop(self):
        self.running.clear()


class App(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("VRChat OSC Voice Analyzer")
        self.resizable(False, False)
        self.protocol("WM_DELETE_WINDOW", self.on_close)

        # --- GUI -----------------------------------------------------------
        devices = [d['name'] for d in sd.query_devices() if d['max_input_channels'] > 0]
        self.combo = ttk.Combobox(self, values=devices, state='readonly', width=40)
        self.combo.grid(row=0, column=0, columnspan=2, padx=10, pady=10)
        if devices: self.combo.current(0)

        ttk.Button(self, text="Start", command=self.start).grid(row=1, column=0, padx=10, pady=10)
        ttk.Button(self, text="Stop",  command=self.stop).grid(row=1, column=1, padx=10, pady=10)

        # --- 内部状態 -------------------------------------------------------
        self.stream = None
        self.analyzer = None
        self.ring = None

    def start(self):
        try:
            dev_name = self.combo.get()
            dev_info = next(d for d in sd.query_devices() if d['name'] == dev_name)
        except StopIteration:
            messagebox.showerror("Device", "マイクを選択してください")
            return

        sr = int(dev_info["default_samplerate"])
        self.ring = RingBuffer(int(BUFFER_SECONDS * sr))

        # Sounddevice stream
        self.stream = sd.InputStream(
            device=dev_info['index'], samplerate=sr, channels=1, dtype='float32',
            callback=lambda indata, frames, t, status: self.ring.extend(indata[:, 0])
        )
        self.stream.start()

        # Analyzer thread
        osc = SimpleUDPClient(OSC_IP, OSC_PORT)
        self.analyzer = VoiceAnalyzer(self.ring, sr, osc, default_f0_estimator)
        self.analyzer.start()

    def stop(self):
        if self.stream:
            self.stream.stop(); self.stream.close(); self.stream = None
        if self.analyzer:
            self.analyzer.stop(); self.analyzer = None

    def on_close(self):
        self.stop()
        self.destroy()


if __name__ == "__main__":
    App().mainloop()
