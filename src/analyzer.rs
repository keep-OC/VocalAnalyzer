use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use pitch_detection::detector::PitchDetector;

use crate::osc::OscSender;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

const CHUNK_SIZE: usize = 2048;

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

fn capture_loop(device_id: &str, tx: mpsc::SyncSender<Vec<f32>>, chunksize: usize) -> Res<()> {
    let device = get_device(device_id)?;
    let mut audio_client = device.get_iaudioclient()?;
    let sample_type = &wasapi::SampleType::Float;
    let desired_format = wasapi::WaveFormat::new(32, 32, sample_type, 48000, 2, Some(1));
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
            if let Err(_) = tx.send(chunk) {
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

pub struct Analyzer {
    stop_sender: mpsc::Sender<()>,
    pub detected_piches: Arc<Mutex<VecDeque<f32>>>,
}

impl Analyzer {
    pub fn new(device_id: &str) -> Self {
        let (stop_sender, stop) = mpsc::channel();
        let capturer = Capturer::new(device_id);
        let detected_piches = Arc::new(Mutex::new(VecDeque::from([f32::NAN; 100])));
        let clone = detected_piches.clone();
        let osc_sender = OscSender::new();
        thread::spawn(move || loop {
            if let Ok(()) = stop.try_recv() {
                break;
            }
            if let Ok(v) = capturer.rx.try_recv() {
                let mut detector = pitch_detection::detector::yin::YINDetector::new(CHUNK_SIZE, 0);
                let pitch = detector.get_pitch(&v, 48_000, 0.1, 0.1);
                let frequency = pitch.map(|p| p.frequency);
                osc_sender.send_frequency(frequency.unwrap_or(0.0));
                let mut lock = clone.lock().unwrap();
                lock.pop_front();
                lock.push_back(frequency.unwrap_or(f32::NAN));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        Self {
            stop_sender,
            detected_piches,
        }
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        self.stop_sender.send(()).unwrap();
    }
}
