use std::{collections::VecDeque, sync::mpsc, thread};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

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

fn capture_loop(
    device_id: &str,
    tx: mpsc::SyncSender<Vec<f32>>,
    samplerate: usize,
    chunksize: usize,
) -> Res<()> {
    let device = get_device(device_id)?;
    let mut audio_client = device.get_iaudioclient()?;
    let sample_type = &wasapi::SampleType::Float;
    let desired_format = wasapi::WaveFormat::new(32, 32, sample_type, samplerate, 2, None);
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
    let mut sample_queue =
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
    pub rx: mpsc::Receiver<Vec<f32>>,
}

impl Capturer {
    pub fn new(device_id: &str, samplerate: usize, chunksize: usize) -> Self {
        let device_id = device_id.to_owned();
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            capture_loop(&device_id, tx, samplerate, chunksize).unwrap();
        });
        Self { rx }
    }
}
