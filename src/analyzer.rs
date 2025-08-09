use core::f32;
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::sync::{mpsc, Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;

use linear_predictive_coding::calc_lpc_by_burg;
use pitch_detection::detector;
use pitch_detection::detector::PitchDetector;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Inv;

use crate::osc::OscSender;
use crate::sound_device::{Capturer, Sound};
use crate::utils;

pub const CHUNK_SIZE: usize = 1024;
const BUFFER_SIZE: usize = CHUNK_SIZE * 4;
const LPC_DEPTH: usize = 20;
const FORMANT_SPEC_SIZE: usize = 512;

struct Feature {
    rms: f32,
    freq: Option<f32>,
    spectrum: Vec<(f32, f32)>,
    gains: Vec<f32>,
    formant_spec: Vec<(f64, f64)>,
    formant_peak: Vec<f64>,
}

struct FeatureAnalyzer {
    detector: detector::mcleod::McLeodDetector<f32>,
    fft: Arc<dyn rustfft::Fft<f32>>,
}

impl FeatureAnalyzer {
    fn new() -> Self {
        let detector = detector::mcleod::McLeodDetector::new(BUFFER_SIZE, BUFFER_SIZE / 2);
        let mut planner = rustfft::FftPlanner::new();
        let fft = planner.plan_fft_forward(BUFFER_SIZE);
        Self { detector, fft }
    }
    fn analyze(&mut self, sound: &Sound) -> Feature {
        let rms = calc_rms(sound);
        let freq = self.analyze_freq(sound);
        let spectrum = self.analyze_spectrum(sound);
        let gains: Vec<f32> = (1..=20)
            .map(|k| {
                freq.map_or(0.0, |f0| {
                    let freq = f0 * k as f32;
                    gain_at_freq(&spectrum, &freq).clamp(0.0, 1.0)
                })
            })
            .collect();
        let (formant_spec, formant_peak) = self.analyze_formant(sound);

        Feature {
            rms,
            freq,
            spectrum,
            gains,
            formant_spec,
            formant_peak,
        }
    }

    fn analyze_freq(&mut self, s: &Sound) -> Option<f32> {
        let pitch = self.detector.get_pitch(&s.samples, s.samplerate, 1.0, 0.7);
        pitch.map(|p| p.frequency)
    }

    fn analyze_spectrum(&self, s: &Sound) -> Vec<(f32, f32)> {
        let window = apodize::hanning_iter(BUFFER_SIZE);
        let mut spec: Vec<Complex<f32>> = s
            .samples
            .iter()
            .zip(window)
            .map(|(a, b)| Complex::from(a * b as f32))
            .collect();
        self.fft.process(&mut spec);
        let freq_step = s.samplerate as f32 / BUFFER_SIZE as f32;
        spec.into_iter()
            .take(BUFFER_SIZE / 2)
            .enumerate()
            .map(|(i, c)| (i as f32 * freq_step, c.norm()))
            .collect()
    }

    fn analyze_formant(&self, s: &Sound) -> (Vec<(f64, f64)>, Vec<f64>) {
        const CHUNK: usize = 2;
        const RESAMPLED_BUFFER_SIZE: usize = BUFFER_SIZE / CHUNK;
        let nyquist = s.samplerate / 2;
        let resampled_rate = s.samplerate / CHUNK;
        let resampled_nyquist = nyquist as f64 / CHUNK as f64;
        let mut buffer: Vec<f32> = s
            .samples
            .chunks(CHUNK)
            .map(|chunk| chunk.iter().sum())
            .collect();
        process_hpf(&mut buffer, s.samplerate, 50.0);
        process_window(&mut buffer, apodize::hanning_iter(RESAMPLED_BUFFER_SIZE));
        let array = ndarray::Array::from_iter(buffer.iter().map(|&x| x as f64));
        let filter_coeffs = calc_lpc_by_burg(array.view(), LPC_DEPTH).unwrap().to_vec();
        let spec = calc_freq_responce(&filter_coeffs, FORMANT_SPEC_SIZE, resampled_rate);
        let roots: Vec<Complex<f64>> = calc_poly_roots(&filter_coeffs);
        let mut freqs: Vec<f64> = roots
            .into_iter()
            .map(|r| r.arg() * resampled_nyquist / PI)
            .filter(|&freq| 100.0 < freq && freq < resampled_nyquist - 100.0)
            .collect();
        freqs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (spec, freqs)
    }
}

struct ResultStore {
    rms: f32,
    freq_history: VecDeque<f32>,
    spectrum: Vec<(f32, f32)>,
    gains: Vec<f32>,
    formant_spec: Vec<(f64, f64)>,
    formant_peak: Vec<f64>,
}

impl ResultStore {
    fn new() -> Self {
        Self {
            rms: 0.0,
            freq_history: VecDeque::from([f32::NAN; 201]),
            spectrum: vec![(0.0, 0.0); BUFFER_SIZE / 2],
            gains: vec![0.0; 20],
            formant_spec: vec![(0.0, 0.0); FORMANT_SPEC_SIZE],
            formant_peak: vec![],
        }
    }

    fn push(&mut self, f: &Feature) {
        self.rms = f.rms;
        self.freq_history.pop_front();
        self.freq_history.push_back(f.freq.unwrap_or(f32::NAN));
        self.spectrum.copy_from_slice(&f.spectrum);
        self.gains.copy_from_slice(&f.gains);
        self.formant_spec.copy_from_slice(&f.formant_spec);
        self.formant_peak.clone_from(&f.formant_peak);
    }
}

pub struct Results(Arc<RwLock<ResultStore>>);

impl Results {
    fn new() -> Self {
        Self(Arc::new(RwLock::new(ResultStore::new())))
    }

    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    fn read(&self) -> RwLockReadGuard<'_, ResultStore> {
        self.0.read().unwrap()
    }

    fn write(&self) -> RwLockWriteGuard<'_, ResultStore> {
        self.0.write().unwrap()
    }

    pub fn volume_db(&self) -> f32 {
        utils::to_db(self.read().rms)
    }

    pub fn freq_history_in_midi_note(&self) -> Vec<f32> {
        self.read()
            .freq_history
            .iter()
            .map(freq_to_midi_note)
            .collect()
    }

    pub fn spectrum(&self) -> Vec<(f32, f32)> {
        self.read().spectrum.clone()
    }

    pub fn spectrum_in_midi_note(&self) -> Vec<(f32, f32)> {
        self.read()
            .spectrum
            .iter()
            .map(|(freq, power)| {
                let midi_note = freq_to_midi_note(freq);
                let gain = 2.0 * power.ln();
                (midi_note, gain)
            })
            .collect()
    }

    pub fn gains(&self) -> Vec<f32> {
        self.read().gains.clone()
    }

    pub fn formant_spec(&self) -> Vec<(f64, f64)> {
        self.read().formant_spec.clone()
    }

    pub fn formant_peak(&self) -> Vec<f64> {
        self.read().formant_peak.clone()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AnalyzerOptions {
    pub gain: f32,
}

fn spawn_analyze_loop(
    capturer: Capturer,
    results: Results,
    options: Arc<RwLock<AnalyzerOptions>>,
    stop: mpsc::Receiver<()>,
) {
    thread::spawn(move || {
        let mut buffer = VecDeque::from([0.0; BUFFER_SIZE]);
        let osc_sender = OscSender::new();
        let mut feature_analyzer = FeatureAnalyzer::new();
        while stop.try_recv().is_err() {
            let sound = capturer.rx.recv().unwrap();
            buffer.drain(..CHUNK_SIZE);
            buffer.extend(sound.samples);
            let factor = utils::from_db(options.read().unwrap().gain);
            let sound = Sound {
                samplerate: sound.samplerate,
                samples: buffer.iter().map(|s| s * factor).collect(),
            };
            let feature = feature_analyzer.analyze(&sound);
            results.write().push(&feature);
            let freq_normalized = feature.freq.map_or(-1.0, normalize_freq);
            let formants = feature
                .formant_peak
                .iter()
                .take(4)
                .map(|&f| f.clamp(0.0, 8192.0) as f32 / 0x3FFF as f32)
                .collect();
            osc_sender.send_param(freq_normalized, feature.gains, formants);
        }
    });
}

pub struct Analyzer {
    stop_sender: mpsc::Sender<()>,
    pub results: Results,
    pub options: Arc<RwLock<AnalyzerOptions>>,
}

impl Analyzer {
    pub fn new(capturer: Capturer, options: AnalyzerOptions) -> Self {
        let (stop_sender, stop) = mpsc::channel();
        let results = Results::new();
        let options = Arc::new(RwLock::new(options));
        spawn_analyze_loop(capturer, results.clone(), options.clone(), stop);
        Self {
            stop_sender,
            results,
            options,
        }
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
    utils::normalize(midinote, E2, G5).clamp(0.0, 1.0)
}

fn gain_at_freq(spec: &[(f32, f32)], freq: &f32) -> f32 {
    let index = spec.iter().position(|(f, _)| *f >= *freq);
    if index.is_none() {
        return 0.0;
    }
    let index = index.unwrap();
    let (upper_freq, upper_gain) = spec[index];
    let (lower_freq, lower_gain) = if index >= 1 {
        spec[index - 1]
    } else {
        (0.0, 0.0)
    };
    let freq_step = upper_freq - lower_freq;
    let coeff = (freq % freq_step) / freq_step;
    let power = utils::lerp(lower_gain, upper_gain, coeff);
    power.ln() * 0.2
}

fn calc_rms(s: &Sound) -> f32 {
    let len = s.samples.len() as f32;
    let mean_square = s.samples.iter().map(|f| f * f / len).sum::<f32>();
    mean_square.sqrt()
}

fn calc_freq_responce(coeffs: &[f64], size: usize, samplerate: usize) -> Vec<(f64, f64)> {
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
            let freq = samplerate as f64 / (2 * size) as f64 * i as f64;
            let z = Complex::from_polar(1.0, omega);
            (freq, a(z).norm().inv())
        })
        .collect()
}

fn calc_poly_roots(coeffs: &Vec<f64>) -> Vec<Complex<f64>> {
    let mut poly = [1.0; LPC_DEPTH + 1];
    poly.iter_mut()
        .skip(1)
        .zip(coeffs)
        .for_each(|(p, c)| *p = *c);
    let roots = rpoly::rpoly(&poly);
    if roots.is_err() {
        return vec![];
    }
    roots
        .unwrap()
        .into_iter()
        .map(|r| Complex { re: r.re, im: r.im })
        .collect()
}

fn process_hpf(s: &mut [f32], samplerate: usize, cutoff_freq: f32) {
    let alpha = (-2.0 * PI as f32 * cutoff_freq / samplerate as f32).exp();
    for i in (2..s.len()).rev() {
        s[i] -= alpha * s[i - 1];
    }
}

fn process_window<I: Iterator<Item = f64>>(s: &mut [f32], window: I) {
    s.iter_mut().zip(window).for_each(|(x, w)| *x *= w as f32);
}
