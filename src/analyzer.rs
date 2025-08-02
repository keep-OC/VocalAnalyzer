use core::f32;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use linear_predictive_coding::calc_lpc_by_burg;
use pitch_detection::detector::yin::YINDetector;
use pitch_detection::detector::PitchDetector;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Inv;

use crate::osc::OscSender;
use crate::sound_device;

pub const SAMPLE_RATE: usize = 48_000;
pub const CHUNK_SIZE: usize = 1024;
const BUFFER_SIZE: usize = CHUNK_SIZE * 4;
const FREQ_STEP: f32 = SAMPLE_RATE as f32 / BUFFER_SIZE as f32;

struct Feature {
    freq: Option<f32>,
    spectrum: Vec<f32>,
    gains: Vec<f32>,
    formant_spec: Vec<f64>,
}

struct FeatureAnalyzer {
    detector: YINDetector<f32>,
    fft: Arc<dyn rustfft::Fft<f32>>,
}
impl FeatureAnalyzer {
    fn new() -> Self {
        let detector = pitch_detection::detector::yin::YINDetector::new(BUFFER_SIZE, 0);
        let mut planner = rustfft::FftPlanner::new();
        let fft = planner.plan_fft_forward(BUFFER_SIZE);
        Self { detector, fft }
    }
    fn analyze(&mut self, samples: &[f32]) -> Feature {
        let freq = self.analyze_freq(samples);
        let spectrum = self.analyze_spectrum(samples);
        let gains: Vec<f32> = (1..=20)
            .map(|k| {
                freq.map_or(0.0, |f0| {
                    let freq = f0 * k as f32;
                    gain_at_freq(&spectrum, &freq).clamp(0.0, 1.0)
                })
            })
            .collect();
        let formant_spec = self.analyze_formant(samples);
        Feature {
            freq,
            spectrum,
            gains,
            formant_spec,
        }
    }

    fn analyze_freq(&mut self, samples: &[f32]) -> Option<f32> {
        let pitch = self.detector.get_pitch(samples, SAMPLE_RATE, 0.1, 0.1);
        pitch.map(|p| p.frequency)
    }

    fn analyze_spectrum(&self, samples: &[f32]) -> Vec<f32> {
        let window = apodize::hanning_iter(BUFFER_SIZE);
        let mut spec: Vec<Complex<f32>> = samples
            .iter()
            .zip(window)
            .map(|(a, b)| Complex::from(a * b as f32))
            .collect();
        self.fft.process(&mut spec);
        spec.into_iter()
            .take(BUFFER_SIZE / 2)
            .map(|c| c.norm())
            .collect()
    }

    fn analyze_formant(&self, samples: &[f32]) -> Vec<f64> {
        let array = ndarray::Array::from_iter(samples.iter().map(|&x| x as f64));
        let filter_coeffs = calc_lpc_by_burg(array.view(), 24).unwrap().to_vec();
        calc_freq_responce(&filter_coeffs, 512)
    }
}

struct AnalyzedFeatures {
    freq_history: VecDeque<f32>,
    spectrum: Vec<f32>,
    gains: Vec<f32>,
    formant_spec: Vec<f64>,
}

impl AnalyzedFeatures {
    fn new() -> Self {
        Self {
            freq_history: VecDeque::from([f32::NAN; 201]),
            spectrum: vec![0.0; BUFFER_SIZE / 2],
            gains: vec![0.0; 20],
            formant_spec: vec![0.0; 512],
        }
    }
    fn push(&mut self, f: &Feature) {
        self.freq_history.pop_front();
        self.freq_history.push_back(f.freq.unwrap_or(f32::NAN));
        self.spectrum.copy_from_slice(&f.spectrum);
        self.gains.copy_from_slice(&f.gains);
        self.formant_spec.copy_from_slice(&f.formant_spec);
    }
}

pub struct Analyzer {
    stop_sender: mpsc::Sender<()>,
    data: Arc<Mutex<AnalyzedFeatures>>,
}

impl Analyzer {
    pub fn new(capturer: sound_device::Capturer) -> Self {
        let (stop_sender, stop) = mpsc::channel();
        let data = Arc::new(Mutex::new(AnalyzedFeatures::new()));
        let data_clone = data.clone();
        thread::spawn(move || {
            let mut buffer = VecDeque::from([0.0; BUFFER_SIZE]);
            let osc_sender = OscSender::new();
            let mut feature_analyzer = FeatureAnalyzer::new();
            while stop.try_recv().is_err() {
                let chunk = capturer.rx.recv().unwrap();
                buffer.drain(..CHUNK_SIZE);
                buffer.extend(chunk);
                let samples: Vec<f32> = buffer.iter().cloned().collect();
                let feature = feature_analyzer.analyze(&samples);
                data_clone.lock().unwrap().push(&feature);
                let freq_normalized = feature.freq.map_or(-1.0, normalize_freq);
                osc_sender.send_frequency(freq_normalized, feature.gains);
            }
        });
        Self { stop_sender, data }
    }

    pub fn freq_history_in_midi_note(&self) -> Vec<f32> {
        self.data
            .lock()
            .unwrap()
            .freq_history
            .iter()
            .map(freq_to_midi_note)
            .collect()
    }

    pub fn spectrum(&self) -> Vec<(f32, f32)> {
        self.data
            .lock()
            .unwrap()
            .spectrum
            .iter()
            .enumerate()
            .map(|(i, &power)| {
                let freq = FREQ_STEP * i as f32;
                let midi_note = freq_to_midi_note(&freq);
                let gain = 2.0 * power.ln();
                (midi_note, gain)
            })
            .collect()
    }

    pub fn gains(&self) -> Vec<f32> {
        self.data.lock().unwrap().gains.clone()
    }

    pub fn formant_spec(&self) -> Vec<f64> {
        self.data.lock().unwrap().formant_spec.clone()
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        self.stop_sender.send(()).unwrap();
    }
}

fn freq_to_midi_note(freq: &f32) -> f32 {
    if *freq < 1.0 {
        return 0.0;
    }
    69.0 + 12.0 * (freq / 440.0).log2()
}

fn normalize_freq(freq: f32) -> f32 {
    const E2: f32 = 40.0;
    const G5: f32 = 79.0;
    let midinote = freq_to_midi_note(&freq);
    let normalize = |v, min, max| (v - min) / (max - min);
    normalize(midinote, E2, G5).clamp(0.0, 1.0)
}

fn gain_at_freq(spec: &Vec<f32>, freq: &f32) -> f32 {
    let index = (freq / FREQ_STEP) as usize;
    let coeff = (freq % FREQ_STEP) / FREQ_STEP;
    let lerp = |a, b, t| a + (b - a) * t;

    let power = if index >= spec.len() {
        0.0
    } else if index + 1 >= spec.len() {
        spec[index]
    } else {
        lerp(spec[index], spec[index + 1], coeff)
    };
    power.ln() * 0.2
}

fn calc_freq_responce(coeffs: &Vec<f64>, size: usize) -> Vec<f64> {
    let one = Complex::new(1.0, 0.0);
    let a = |z: Complex<f64>| {
        one + coeffs
            .iter()
            .enumerate()
            .map(|(i, a)| a * z.powi(-(1 + i as i32)))
            .sum::<Complex<f64>>()
    };
    (0..size)
        .map(|i| {
            let omega = i as f64 * std::f64::consts::PI / size as f64;
            let z = Complex::from_polar(1.0, omega);
            a(z).norm().inv().ln()
        })
        .collect()
}
