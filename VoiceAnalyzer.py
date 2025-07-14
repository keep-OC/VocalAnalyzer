import tkinter as tk
from tkinter import ttk, messagebox
import numpy as np
import sounddevice as sd
from pythonosc.udp_client import SimpleUDPClient
import threading, collections, time, sys
import librosa

###############################################################################
# --- ユーザ設定 ---------------------------------------------------------------
OSC_IP   = "127.0.0.1"
OSC_PORT = 9000
BUFFER_SECONDS = 0.5        # リングバッファ長 (秒)
FPS = 60                    # 解析・送信レート (>=60)
HARMONICS = 20
DEBUG = True
# -----------------------------------------------------------------------------

# --- 送信先パラメータ名 ------------------------------------------------------- ### CHANGED
PARAM_FT_L  = "/avatar/parameters/FT_L"
PARAM_FT_H  = "/avatar/parameters/FT_H"
PARAM_GAIN  = [f"/avatar/parameters/G{i}" for i in range(1, HARMONICS + 1)]
# -----------------------------------------------------------------------------


def default_f0_estimator(wave, sr):
    # """ごく簡易なオートコリレーション基音推定（差し替え前提）"""
    # wave = wave - np.mean(wave)
    # corr = np.correlate(wave, wave, mode="full")[len(wave)-1:]
    # d = np.diff(corr)
    # idx = np.flatnonzero(d > 0)
    # if idx.size == 0:
    #     return 0.0
    # peak = np.argmax(corr[idx[0]:]) + idx[0]
    # return 0.0 if peak == 0 else sr / peak
    try:
        f0_series = librosa.yin(
            wave,
            fmin=80,            # 成人男性でも通常カバーできる最低周波数
            fmax=1000,
            sr=sr,
            frame_length=len(wave)
        )
        f0 = f0_series[0]
        return float(f0) if not np.isnan(f0) else 0.0
    except Exception:
        return 0.0


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


# class VoiceAnalyzer(threading.Thread):
#     def __init__(self, ring, sr, osc_client, f0_func):
#         super().__init__(daemon=True)
#         self.ring, self.sr, self.osc, self.f0_func = ring, sr, osc_client, f0_func
#         self.running = threading.Event(); self.running.set()

#     def run(self):
#         hop = 1.0 / FPS
#         next_ts = time.perf_counter()

#         # デバッグ用：送信カウント
#         send_count = 0
#         debug_timer = time.perf_counter()

#         while self.running.is_set():
#             now = time.perf_counter()
#             if now < next_ts:
#                 time.sleep(next_ts - now)
#                 continue
#             next_ts += hop

#             samples = self.ring.get()
#             if samples.size < 1024:
#                 continue

#             # --- FFT & 基音 --------------------------------------------------
#             spec  = np.abs(np.fft.rfft(samples))
#             freqs = np.fft.rfftfreq(samples.size, 1 / self.sr)

#             f0 = float(self.f0_func(samples, self.sr))  # [Hz]
#             f0_q = max(0, min(65535, int(round(f0))))   # 16-bit 量子化

#             ft_l = 1 / (f0_q & 0x7F) if (f0_q & 0x7F) > 0 else 0
#             ft_h = 1 / ((f0_q >> 7) & 0x7F) if ((f0_q >> 7) & 0x7F) > 0 else 0

#             self.osc.send_message(PARAM_FT_L, ft_l)
#             self.osc.send_message(PARAM_FT_H, ft_h)

#             if f0 > 0:
#                 amp_scale = samples.size
#                 for i in range(1, HARMONICS + 1):
#                     target = f0 * i
#                     idx    = int(np.argmin(np.abs(freqs - target)))
#                     amp    = spec[idx] / amp_scale
#                     amp = amp / 0.05
#                     self.osc.send_message(PARAM_GAIN[i-1], float(amp))

#             # --- 送信回数カウント & 出力 -------------------------------------
#             send_count += 1
#             if DEBUG:
#                 if now - debug_timer >= 1.0:
#                     print(f"[DEBUG] {send_count} sends/sec", file=sys.stderr, flush=True)
#                     send_count = 0
#                     debug_timer = now

class VoiceAnalyzer(threading.Thread):
    def __init__(self, ring, sr, osc_client, f0_func):
        super().__init__(daemon=True)
        self.ring, self.sr, self.osc, self.f0_func = ring, sr, osc_client, f0_func
        self.running = threading.Event(); self.running.set()

    def run(self):
        # send_count = 0
        # debug_timer = time.perf_counter()

        # while self.running.is_set():
        #     samples = self.ring.get()
        #     if samples.size < 1024:
        #         time.sleep(0.001)  # バッファが足りない時は軽く待つ
        #         continue
        
        send_count = 0
        debug_timer = time.perf_counter()

        interval = 1.0 / FPS
        next_time = time.perf_counter()

        while self.running.is_set():
            now = time.perf_counter()
            if now < next_time:
                time.sleep(next_time - now)
                continue
            next_time += interval

            samples = self.ring.get()
            if samples.size < 1024:
                continue  # or sleep(0.001)

            # FFT & 基音
            spec  = np.abs(np.fft.rfft(samples))
            freqs = np.fft.rfftfreq(samples.size, 1 / self.sr)
            f0 = float(self.f0_func(samples, self.sr))
            f0_q = max(0, min(65535, int(round(f0))))

            ft_l = 1 / (f0_q & 0x7F) if (f0_q & 0x7F) > 0 else -1.0
            ft_h = 1 / ((f0_q >> 7) & 0x7F) if ((f0_q >> 7) & 0x7F) > 0 else -1.0
            self.osc.send_message(PARAM_FT_L, ft_l)
            self.osc.send_message(PARAM_FT_H, ft_h)

            # if f0 > 0:
            #     amp_scale = samples.size
            #     for i in range(1, HARMONICS + 1):
            #         target = f0 * i
            #         idx = int(np.argmin(np.abs(freqs - target)))
            #         amp = spec[idx] / amp_scale
            #         amp = amp / 0.05
            #         self.osc.send_message(PARAM_GAIN[i-1], float(amp))

            # if f0 > 0:
            #     amp_scale = samples.size
            #     freq_tolerance = 5.0  # Hz
            #     for i in range(1, HARMONICS + 1):
            #         target = f0 * i
            #         # 範囲内のビンを取得
            #         mask = (freqs >= target - freq_tolerance) & (freqs <= target + freq_tolerance)
            #         if not np.any(mask):
            #             amp = 0.0
            #             peak_freq = 0.0
            #         else:
            #             peak_idx = np.argmax(spec[mask])
            #             peak_freq = freqs[mask][peak_idx]
            #             amp = spec[mask][peak_idx] / amp_scale
            #         amp = amp / 0.05  # スケーリング（任意）

            #         if DEBUG and i == 1:
            #             print(f"[DEBUG] Harmonic {i}: Peak @ {peak_freq:.1f} Hz, Amp = {amp:.3f}", file=sys.stderr)

            #         self.osc.send_message(PARAM_GAIN[i-1], float(amp))

            if f0 > 0:
                amp_scale = samples.size
                for i in range(1, HARMONICS + 1):
                    target = f0 * i
                    idx = np.argmin(np.abs(freqs - target))
                    peak_freq = freqs[idx]
                    amp = spec[idx] / amp_scale
                    amp = amp / 0.05  # スケーリング

                   # Harmonic デバッグ表示
                    if self.running.is_set() and DEBUG and i == 1:
                        print(f"[DEBUG] Harmonic {i}: Target {target:.2f} Hz → Closest bin {peak_freq:.2f} Hz, Amp = {amp:.3f}, FT_L={ft_l}, FT_H={ft_h}", file=sys.stderr)

                    self.osc.send_message(PARAM_GAIN[i - 1], float(amp))


            send_count += 1
            now = time.perf_counter()
            # 送信回数表示
            if self.running.is_set() and DEBUG and now - debug_timer >= 1.0:
                print(f"[DEBUG] {send_count} sends/sec", file=sys.stderr, flush=True)
                send_count = 0
                debug_timer = now
                        
        self.running.clear()

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
        if self.analyzer:
            if self.analyzer.is_alive():
                self.analyzer.stop()
                self.analyzer.join()
            self.analyzer = None

        if self.stream:
            self.stream.stop()
            self.stream.close()
            self.stream = None

    def on_close(self):
        self.stop()
        self.destroy()


if __name__ == "__main__":
    App().mainloop()
