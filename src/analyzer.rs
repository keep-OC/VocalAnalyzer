use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;

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

pub struct Analyzer {
    rx: mpsc::Receiver<Vec<u8>>,
    _waveformat: wasapi::WaveFormat,
    _worker: Option<thread::JoinHandle<()>>,
}

impl Analyzer {
    pub fn new(device_id: &str) -> Self {
        let device_id = device_id.to_owned();
        let device = get_device(&device_id).unwrap();
        let waveformat = device.get_iaudioclient().unwrap().get_mixformat().unwrap();
        let channels = waveformat.get_nchannels() as usize;
        let (tx, rx) = mpsc::sync_channel(channels);
        let worker = thread::spawn(move || {
            let device = get_device(&device_id).unwrap();
            capture_loop(device, tx, 4096).unwrap();
        });
        println!("{:?}", waveformat);
        Self {
            rx,
            _waveformat: waveformat,
            _worker: worker.into(),
        }
    }
    pub fn periodic(&self) {
        if let Ok(v) = self.rx.try_recv() {
            let samples = v.iter().take(8).collect::<Vec<_>>();
            println!("{}: {:?}...", v.iter().count(), samples);
        }
    }
}
