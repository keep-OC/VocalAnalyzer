use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use pitch_detection::detector::PitchDetector;

use crate::osc::OscSender;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

const SAMPLE_RATE: usize = 48_000;
const CHUNK_SIZE: usize = 1024;

pub fn get_devices() -> Res<Vec<wasapi::Device>> {
    let direction = &wasapi::Direction::Capture;
    let devices = wasapi::DeviceCollection::new(direction)?
        .into_iter()
        .map(|device| device.unwrap())
        .collect();
    Ok(devices)
}

fn get_device(device_id: &str) -> Res<wasapi::Device> {
    let device = get_devices()?
        .into_iter()
        .find(|device| device.get_id().unwrap() == device_id)
        .unwrap();
    Ok(device)
}

pub fn get_default_device() -> Res<wasapi::Device> {
    let direction = &wasapi::Direction::Capture;
    let device = wasapi::get_default_device(direction)?;
    Ok(device)
}

fn capture_loop(device_id: &str, tx: mpsc::SyncSender<Vec<f32>>, chunksize: usize) -> Res<()> {
    let device = get_device(device_id)?;
    let mut audio_client = device.get_iaudioclient()?;
    let sample_type = &wasapi::SampleType::Float;
    let desired_format = wasapi::WaveFormat::new(32, 32, sample_type, SAMPLE_RATE, 2, None);
    let blockalign = desired_format.get_blockalign();
    let (_def_time, min_time) = audio_client.get_device_period()?;
    let mode = wasapi::StreamMode::EventsShared {
        autoconvert: false,
        buffer_duration_hns: min_time,
    };
    let direction = &wasapi::Direction::Capture;
    audio_client.initialize_client(&desired_format, direction, &mode)?;
    let buffer_size = audio_client.get_buffer_size()?;
    let h_event = audio_client.set_get_eventhandle()?;
    let capture_client = audio_client.get_audiocaptureclient()?;
    let mut sample_queue: VecDeque<u8> =
        VecDeque::with_capacity(100 * blockalign as usize * (1024 + 2 * buffer_size as usize));
    audio_client.start_stream()?;
    loop {
        let mut stopped = false;
        while sample_queue.len() > (blockalign as usize * chunksize) {
            let mut chunk = vec![0f32; chunksize];
            for element in chunk.iter_mut() {
                let vl: Vec<u8> = sample_queue.drain(0..4).collect();
                let vr: Vec<u8> = sample_queue.drain(0..4).collect();
                let fl = f32::from_le_bytes(vl.try_into().unwrap());
                let fr = f32::from_le_bytes(vr.try_into().unwrap());
                *element = (fl + fr) / 2.0;
            }
            if tx.send(chunk).is_err() {
                stopped = true;
                break;
            }
        }
        capture_client.read_from_device_to_deque(&mut sample_queue)?;
        if stopped || h_event.wait_for_event(30_000).is_err() {
            audio_client.stop_stream()?;
            break;
        }
    }
    Ok(())
}

pub struct Capturer {
    rx: mpsc::Receiver<Vec<f32>>,
}

impl Capturer {
    pub fn new(device_id: &str) -> Self {
        let device_id = device_id.to_owned();
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            capture_loop(&device_id, tx, CHUNK_SIZE).unwrap();
        });
        Self { rx }
    }
}

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
}

impl Analyzer {
    pub fn new(device_id: &str) -> Self {
        let (stop_sender, stop) = mpsc::channel();
        let capturer = Capturer::new(device_id);
        let freq_history = Arc::new(Mutex::new(History::new(f32::NAN, 200)));
        let freq_history_clone = freq_history.clone();
        thread::spawn(move || {
            const BUFFER_SIZE: usize = CHUNK_SIZE * 4;
            let mut buffer = VecDeque::from([0.0; BUFFER_SIZE]);
            let mut detector = pitch_detection::detector::yin::YINDetector::new(BUFFER_SIZE, 0);
            let osc_sender = OscSender::new();
            while stop.try_recv().is_err() {
                let chunk = capturer.rx.recv().unwrap();
                buffer.drain(..CHUNK_SIZE);
                buffer.extend(chunk);
                let signal: Vec<f32> = buffer.iter().cloned().collect();
                let pitch = detector.get_pitch(&signal, SAMPLE_RATE, 0.1, 0.1);
                let frequency = pitch.map(|p| p.frequency);

                osc_sender.send_frequency(frequency.unwrap_or(0.0));

                let mut lock = freq_history_clone.lock().unwrap();
                lock.push(frequency.unwrap_or(f32::NAN));
            }
        });
        Self {
            stop_sender,
            freq_history,
        }
    }

    pub fn freq_history_logscale(&self) -> Vec<f32> {
        self.freq_history
            .lock()
            .unwrap()
            .values
            .iter()
            .map(|f| 69.0 + 12.0 * (f / 440.0).log2())
            .collect()
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        self.stop_sender.send(()).unwrap();
    }
}
