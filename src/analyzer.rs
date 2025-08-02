use core::f32;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use linear_predictive_coding::calc_lpc_by_burg;
use pitch_detection::detector::PitchDetector;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Inv;

use crate::osc::OscSender;
use crate::sound_device;

pub const SAMPLE_RATE: usize = 48_000;
pub const CHUNK_SIZE: usize = 1024;
const BUFFER_SIZE: usize = CHUNK_SIZE * 4;
const FREQ_STEP: f32 = SAMPLE_RATE as f32 / BUFFER_SIZE as f32;

struct History<T: Clone> {
    values: VecDeque<T>,
}

impl<T: Clone> History<T> {
    fn new(value: T, capacity: usize) -> Self {
        let mut values = VecDeque::with_capacity(capacity);
        values.resize(capacity, value);
        Self { values }
    }
    fn push(&mut self, value: T) {
        if self.values.len() == self.values.capacity() {
            self.values.pop_front();
        }
        self.values.push_back(value);
    }
}

pub struct Analyzer {
    stop_sender: mpsc::Sender<()>,
    freq_history: Arc<Mutex<History<f32>>>,
    spectrum: Arc<Mutex<Vec<f32>>>,
    gains: Arc<Mutex<Vec<f32>>>,
    formant_spec: Arc<Mutex<Vec<f64>>>,
}

impl Analyzer {
    pub fn new(capturer: sound_device::Capturer) -> Self {
        let (stop_sender, stop) = mpsc::channel();
        let freq_history = Arc::new(Mutex::new(History::new(f32::NAN, 201)));
        let freq_history_clone = freq_history.clone();
        let spectrum = Arc::new(Mutex::new(vec![0.0; BUFFER_SIZE / 2]));
        let spectrum_clone = spectrum.clone();
        let gains = Arc::new(Mutex::new(vec![0.0; 20]));
        let gains_clone = gains.clone();
        let formant_spec = Arc::new(Mutex::new(vec![0.0; 512]));
        let formant_spec_clone = formant_spec.clone();
        thread::spawn(move || {
            let mut buffer = VecDeque::from([0.0; BUFFER_SIZE]);
            let mut detector = pitch_detection::detector::yin::YINDetector::new(BUFFER_SIZE, 0);
            let mut planner = rustfft::FftPlanner::new();
            let fft = planner.plan_fft_forward(BUFFER_SIZE);
            let osc_sender = OscSender::new();
            while stop.try_recv().is_err() {
                let chunk = capturer.rx.recv().unwrap();
                buffer.drain(..CHUNK_SIZE);
                buffer.extend(chunk);
                let signal: Vec<f32> = buffer.iter().cloned().collect();
                let pitch = detector.get_pitch(&signal, SAMPLE_RATE, 0.1, 0.1);
                let frequency = pitch.map(|p| p.frequency);
                {
                    let mut lock = freq_history_clone.lock().unwrap();
                    lock.push(frequency.unwrap_or(f32::NAN));
                }
                let window = apodize::hanning_iter(BUFFER_SIZE);
                let mut spec: Vec<Complex<f32>> = signal
                    .iter()
                    .zip(window)
                    .map(|(a, b)| Complex::from(a * b as f32))
                    .collect();
                fft.process(&mut spec);
                spectrum_clone
                    .lock()
                    .unwrap()
                    .iter_mut()
                    .enumerate()
                    .for_each(|(i, v)| *v = spec[i].norm_sqr());
                let gains: Vec<f32> = (1..=20)
                    .map(|k| {
                        frequency.map_or(0.0, |f0| {
                            let freq = f0 * k as f32;
                            gain_at_freq(&spec, &freq).clamp(0.0, 1.0)
                        })
                    })
                    .collect();
                gains_clone.lock().unwrap().copy_from_slice(&gains);
                let freq_normalized = frequency.map_or(-1.0, |freq| {
                    const E2: f32 = 40.0;
                    const G5: f32 = 79.0;
                    let midinote = freq_to_midi_note(&freq);
                    let normalize = |v, min, max| (v - min) / (max - min);
                    normalize(midinote, E2, G5).clamp(0.0, 1.0)
                });
                let array = ndarray::Array::from_iter(signal.iter().map(|&x| x as f64));
                let filter_coeffs = calc_lpc_by_burg(array.view(), 24).unwrap().to_vec();
                let formant_spec = calc_freq_responce(&filter_coeffs, 512);
                formant_spec_clone
                    .lock()
                    .unwrap()
                    .copy_from_slice(&formant_spec);
                osc_sender.send_frequency(freq_normalized, gains);
            }
        });
        Self {
            stop_sender,
            freq_history,
            spectrum,
            gains,
            formant_spec,
        }
    }

    pub fn freq_history_in_midi_note(&self) -> Vec<f32> {
        self.freq_history
            .lock()
            .unwrap()
            .values
            .iter()
            .map(freq_to_midi_note)
            .collect()
    }

    pub fn spectrum(&self) -> Vec<(f32, f32)> {
        self.spectrum
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, &power)| {
                let freq = FREQ_STEP * i as f32;
                let midi_note = freq_to_midi_note(&freq);
                let gain = power.ln();
                (midi_note, gain)
            })
            .collect()
    }

    pub fn gains(&self) -> Vec<f32> {
        self.gains.lock().unwrap().clone()
    }

    pub fn formant_spec(&self) -> Vec<f64> {
        self.formant_spec.lock().unwrap().clone()
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

fn gain_at_freq(spec: &Vec<Complex<f32>>, freq: &f32) -> f32 {
    let index = (freq / FREQ_STEP) as usize;
    let coeff = (freq % FREQ_STEP) / FREQ_STEP;
    let lerp = |a, b, t| a + (b - a) * t;

    let power = if index >= spec.len() {
        0.0
    } else if index + 1 >= spec.len() {
        spec[index].norm_sqr()
    } else {
        let [a, b] = [0, 1].map(|i| spec[index + i].norm_sqr());
        lerp(a, b, coeff)
    };
    power.ln() * 0.1
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
