import tkinter as tk
from tkinter import ttk
import numpy as np
import pyaudio
import threading
import time
from pythonosc.udp_client import SimpleUDPClient

class VoiceAnalyzerApp:
    def __init__(self, root):
        self.root = root
        self.root.title("Voice Analyzer (with Mic Selection)")

        self.client = SimpleUDPClient("127.0.0.1", 9000)

        self.RATE = 44100
        self.CHUNK = 1024
        self.N_HARMONICS = 6
        self.running = False

        self.p = pyaudio.PyAudio()
        self.stream = None

        # マイクデバイスの列挙
        self.input_devices = []
        for i in range(self.p.get_device_count()):
            dev = self.p.get_device_info_by_index(i)
            if dev.get("maxInputChannels") > 0:
                self.input_devices.append((i, dev.get("name")))

        # GUI
        self.device_var = tk.StringVar()
        self.device_box = ttk.Combobox(root, textvariable=self.device_var,
                                       values=[name for _, name in self.input_devices], state="readonly")
        self.device_box.pack(pady=5)
        if self.input_devices:
            self.device_box.current(0)

        self.start_button = ttk.Button(root, text="Start", command=self.start_stream)
        self.start_button.pack(pady=5)
        self.stop_button = ttk.Button(root, text="Stop", command=self.stop_stream)
        self.stop_button.pack(pady=5)

    def start_stream(self):
        if self.running:
            return
        self.running = True

        # 選択されたデバイスIDを取得
        selected_index = self.device_box.current()
        device_id = self.input_devices[selected_index][0]

        self.stream = self.p.open(format=pyaudio.paInt16,
                                  channels=1,
                                  rate=self.RATE,
                                  input=True,
                                  input_device_index=device_id,
                                  frames_per_buffer=self.CHUNK)

        self.thread = threading.Thread(target=self.process_loop)
        self.thread.daemon = True
        self.thread.start()

    def stop_stream(self):
        self.running = False
        if self.stream is not None:
            self.stream.stop_stream()
            self.stream.close()
            self.stream = None

    def process_loop(self):
        while self.running:
            try:
                data = self.stream.read(self.CHUNK, exception_on_overflow=False)
                audio = np.frombuffer(data, dtype=np.int16).astype(np.float32)
                audio /= np.max(np.abs(audio) + 1e-8)

                fft = np.fft.rfft(audio * np.hamming(len(audio)))
                magnitude = np.abs(fft)
                freqs = np.fft.rfftfreq(len(audio), d=1.0 / self.RATE)

                peak_idx = np.argmax(magnitude)
                f0 = freqs[peak_idx] if magnitude[peak_idx] > 0.01 else 0.0
                # print(f"F0 Detected: {f0:.2f} Hz")

                for i in range(self.N_HARMONICS):
                    harm_freq = f0 * (i + 1)
                    bin_index = int(harm_freq * len(freqs) / self.RATE)
                    gain = float(magnitude[bin_index]) / np.max(magnitude) if bin_index < len(magnitude) else 0.0

                    # print(f"Harmonic {i+1}: Gain={gain:.2f}  Freq={harm_freq:.2f} Hz")

                    self.client.send_message(f"/avatar/parameters/harmonic{i+1}", gain)
                    self.client.send_message(f"/avatar/parameters/harmonic{i+1}_freq", harm_freq)

            except Exception as e:
                print("[ERROR]:", e)

            time.sleep(0.05)

    def on_close(self):
        self.stop_stream()
        self.p.terminate()
        self.root.destroy()

if __name__ == "__main__":
    root = tk.Tk()
    app = VoiceAnalyzerApp(root)
    root.protocol("WM_DELETE_WINDOW", app.on_close)
    root.mainloop()
