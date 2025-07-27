use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;

use pitch_detection::detector::PitchDetector;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn get_device(device_id: &str) -> Res<wasapi::Device> {
    let direction = &wasapi::Direction::Capture;
    let device_collection = wasapi::DeviceCollection::new(direction)?;
    let device = device_collection
        .into_iter()
        .map(|device| device.unwrap())
        .find(|device| device.get_id().unwrap() == device_id)
        .unwrap();
    Ok(device)
}

fn capture_loop(
    device: wasapi::Device,
    tx: mpsc::SyncSender<Vec<u8>>,
    chunksize: usize,
) -> Res<()> {
    let mut audio_client = device.get_iaudioclient()?;
    let mixformat = audio_client.get_mixformat()?;
    let blockalign = mixformat.get_blockalign();
    let (_def_time, min_time) = audio_client.get_device_period()?;
    let mode = wasapi::StreamMode::EventsShared {
        autoconvert: false,
        buffer_duration_hns: min_time,
    };
    let direction = &wasapi::Direction::Capture;
    audio_client.initialize_client(&mixformat, direction, &mode)?;
    let buffer_size = audio_client.get_buffer_size()?;
    let h_event = audio_client.set_get_eventhandle()?;
    let capture_client = audio_client.get_audiocaptureclient()?;
    let mut sample_queue: VecDeque<u8> =
        VecDeque::with_capacity(100 * blockalign as usize * (1024 + 2 * buffer_size as usize));
    audio_client.start_stream()?;
    loop {
        let mut stopped = false;
        while sample_queue.len() > (blockalign as usize * chunksize) {
            let mut chunk = vec![0u8; blockalign as usize * chunksize];
            for element in chunk.iter_mut() {
                *element = sample_queue.pop_front().unwrap();
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
    rx: mpsc::Receiver<Vec<u8>>,
    waveformat: wasapi::WaveFormat,
}

impl Capturer {
    pub fn new(device_id: &str) -> Self {
        let device_id = device_id.to_owned();
        let device = get_device(&device_id).unwrap();
        let waveformat = device.get_iaudioclient().unwrap().get_mixformat().unwrap();
        let channels = waveformat.get_nchannels() as usize;
        let (tx, rx) = mpsc::sync_channel(channels);
        thread::spawn(move || {
            let device = get_device(&device_id).unwrap();
            capture_loop(device, tx, 4096).unwrap();
        });
        println!("{:?}", waveformat);
        Self { rx, waveformat }
    }
}

pub struct Analyzer {
    stop_sender: mpsc::Sender<()>,
}

impl Analyzer {
    pub fn new(device_id: &str) -> Self {
        let (stop_sender, stop) = mpsc::channel();
        let capturer = Capturer::new(device_id);
        let sample_rate = capturer.waveformat.get_samplespersec() as usize;
        thread::spawn(move || loop {
            if let Ok(()) = stop.try_recv() {
                break;
            }
            if let Ok(v) = capturer.rx.try_recv() {
                let samples: Vec<f32> = v
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                    .collect();
                let right: Vec<f32> = samples.chunks_exact(2).map(|chunk| chunk[0]).collect();
                let mut detector = pitch_detection::detector::yin::YINDetector::new(4096, 0);
                let pitch = detector.get_pitch(&right, sample_rate, 0.0, 0.0);
                if let Some(pitch) = pitch {
                    println!("{:?}", pitch.frequency);
                } else {
                    println!("pitch not detected");
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        });
        Self { stop_sender }
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        self.stop_sender.send(()).unwrap();
    }
}
