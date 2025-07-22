import tkinter as tk
from tkinter import ttk, messagebox
import numpy as np
import sounddevice as sd
from pythonosc.udp_client import SimpleUDPClient
import threading, collections, time, sys
import librosa
import parselmouth
import math 

###############################################################################
# --- ユーザ設定 ---------------------------------------------------------------
OSC_IP   = "127.0.0.1"
OSC_PORT = 9000
BUFFER_SECONDS = 0.5        # リングバッファ長 (秒)
FPS = 60                    # 解析・送信レート (>=60)
HARMONICS = 20
DEBUG = False
# -----------------------------------------------------------------------------

# --- 送信先パラメータ名 ------------------------------------------------------- ### CHANGED
PARAM_FT_L  = "/avatar/parameters/FT_L"
PARAM_FT_H  = "/avatar/parameters/FT_H"
PARAM_GAIN  = [f"/avatar/parameters/G{i}" for i in range(1, HARMONICS + 1)]
PARAM_FORMANT = {
    i: (f"/avatar/parameters/F{i}_L", f"/avatar/parameters/F{i}_H") for i in range(1, 5)
}
# -----------------------------------------------------------------------------

def estimate_formants_parselmouth(wave, sr, n_formants=4):
    try:
        snd = parselmouth.Sound(wave, sr)
        duration = snd.duration
        formant = snd.to_formant_burg(time_step=0.01)  # Burgアルゴリズム
        formants = []
        for i in range(1, n_formants + 1):
            f = formant.get_value_at_time(i, duration / 2)  # 0.01秒時点での第iフォルマント
            formants.append(f if f is not None else 0.0)
        return formants
    except Exception:
        return [0.0] * n_formants

def default_f0_estimator(wave, sr):
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

def log_scale(x, A, B):
    return (np.log(x) - np.log(A)) / (np.log(B) - np.log(A))

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
    def __init__(self, ring, sr, osc_client, f0_func, amp_ref=0.05):
        super().__init__(daemon=True)
        self.ring = ring
        self.sr = sr
        self.osc = osc_client
        self.f0_func = f0_func
        self.amp_ref = amp_ref  # 最大値基準
        self.max_ratio = 0.0  # ゲインの最大比（インジケーター用）
        self.running = threading.Event()
        self.running.set()

    def set_amp_ref(self, new_ref):
        self.amp_ref = max(0.001, float(new_ref))  # 0.0 を防止

    def get_max_ratio(self):
        return min(1.0, self.max_ratio)

    def run(self):
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
            if samples.size < 2048:
                continue  # or sleep(0.001)

            # FFT & 基音
            E2_freq = 82.407
            G5_freq = 783.991
            spec  = np.abs(np.fft.rfft(samples))
            freqs = np.fft.rfftfreq(samples.size, 1 / self.sr)
            f0 = float(self.f0_func(samples, self.sr))
            f0_q = log_scale(f0, E2_freq, G5_freq) * 16383
            f0_q = max(0, min(16383, int(round(f0_q))))
            # f0_q = max(0, min(65535, int(round(f0))))

            ft_l = 1 / (f0_q & 0x7F) if (f0_q & 0x7F) > 0 else -1.0
            ft_h = 1 / ((f0_q >> 7) & 0x7F) if ((f0_q >> 7) & 0x7F) > 0 else -1.0
            self.osc.send_message(PARAM_FT_L, ft_l)
            self.osc.send_message(PARAM_FT_H, ft_h)

            self.max_ratio = 0.0
            if f0 > 0:
                amp_scale = samples.size
                for i in range(1, HARMONICS + 1):
                    target = f0 * i
                    idx = np.argmin(np.abs(freqs - target))
                    peak_freq = freqs[idx]
                    amp = spec[idx] / amp_scale
                    amp = min(1.0, amp / self.amp_ref)  # ← 上限を外部から指定し、1.0 を超えないようにする
                    self.max_ratio = max(self.max_ratio, amp)  # 滑らかに更新

                   # Harmonic デバッグ表示
                    if self.running.is_set() and DEBUG and i == 1:
                        print(f"[DEBUG] Harmonic {i}: Target {target:.2f} Hz → Closest bin {peak_freq:.2f} Hz, Amp = {amp:.3f}, FT_L={ft_l}, FT_H={ft_h}", file=sys.stderr)

                    self.osc.send_message(PARAM_GAIN[i - 1], float(amp))
                
                # フォルマント送信（Parselmouth 使用）
                formants = estimate_formants_parselmouth(samples, self.sr)
                restored = []
                for i, freq in enumerate(formants[:4], 1):
                    if not math.isnan(freq) and freq > 0:
                        # freq_q = np.log2(freq) / np.log2(64) - np.log2(8192) / np.log2(64)
                        # freq_q = log_scale(freq, 1, 8191) * 16383
                        # freq_q = max(0, min(16383, int(round(freq_q))))
                        freq_q = max(0, min(8192, int(round(freq))))
                        inv_l = 1 / (freq_q & 0x7F) if (freq_q & 0x7F) > 0 else -1.0
                        inv_h = 1 / ((freq_q >> 7) & 0x7F) if ((freq_q >> 7) & 0x7F) > 0 else -1.0
                        param_l, param_h = PARAM_FORMANT[i]
                        self.osc.send_message(param_l, inv_l)
                        self.osc.send_message(param_h, inv_h)

                #         # 復元用デバッグ値
                #         restored_freq = 0.0
                #         if inv_l > 0:
                #             restored_freq += 1.0 / inv_l
                #         if inv_h > 0:
                #             restored_freq += (1.0 / inv_h) * 128.0
                #         restored.append(restored_freq)
                #     else:
                #         # NaN や 0 Hz 以下の場合は -1.0 を送る（≒無効値）
                #         param_l, param_h = PARAM_FORMANT[i]
                #         self.osc.send_message(param_l, -1.0)
                #         self.osc.send_message(param_h, -1.0)

                # if self.running.is_set():
                #     while len(restored) < 4:
                #         restored.append(0.0)  # 足りないぶんは 0.0 で埋める
                #     debug_str = ", ".join(f"F{i}={restored[i - 1]:.1f}Hz" for i in range(1, 5))
                #     print(f"[DEBUG] F1~F4 restored: " +
                #         ", ".join(f"F{i}={restored[i-1]:.1f}Hz" for i in range(1, 5)),
                #         file=sys.stderr)
                # if DEBUG:
                    # print(f"[DEBUG] sound duration: {duration:.4f}s", file=sys.stderr)

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

        # スライダー
        self.slider = tk.Scale(self, from_=0.001, to=0.2, resolution=0.001,
                            orient="horizontal", label="Gain Sensitivity (amp_ref)",
                            command=self.on_slider_change)
        self.slider.set(0.05)
        self.slider.grid(row=2, column=0, padx=10, pady=10)

        # 数値入力欄
        self.amp_entry_var = tk.StringVar(value="0.05")
        self.amp_entry = ttk.Entry(self, textvariable=self.amp_entry_var, width=6)
        self.amp_entry.grid(row=2, column=1, padx=10)
        self.amp_entry.bind("<Return>", self.on_entry_change)
        self.amp_entry.bind("<FocusOut>", self.on_entry_change)  # フォーカスが外れたときにも反映

        # インジケーター用 Canvas
        self.canvas = tk.Canvas(self, width=100, height=150, bg="black")
        self.canvas.grid(row=3, column=0, columnspan=2, padx=10, pady=10)
        self.indicator = self.canvas.create_rectangle(40, 150, 60, 150, fill="yellow")

        # インジケーター更新タイマー
        self.after(100, self.update_indicator)

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
        self.analyzer = VoiceAnalyzer(self.ring, sr, osc, default_f0_estimator, amp_ref=0.05)
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

    def on_slider_change(self, val):
        try:
            f = float(val)
            if self.analyzer:
                self.analyzer.set_amp_ref(f)
            self.amp_entry_var.set(f"{f:.3f}")
        except ValueError:
            pass

    def on_entry_change(self, event):
        try:
            val = float(self.amp_entry_var.get())
            if val <= 0.0:
                raise ValueError
            val = round(val, 5)
            if self.analyzer:
                self.analyzer.set_amp_ref(val)
            self.slider.set(val)
        except ValueError:
            # 不正な入力（0 や文字列）は前の有効な値に戻す
            self.amp_entry_var.set(f"{self.slider.get():.3f}")

    def update_indicator(self):
        if self.analyzer:
            ratio = self.analyzer.get_max_ratio()
            top = int(150 - 150 * ratio)
            self.canvas.coords(self.indicator, 40, top, 60, 150)
        self.after(100, self.update_indicator)


if __name__ == "__main__":
    App().mainloop()
