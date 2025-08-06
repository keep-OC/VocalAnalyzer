use std::{collections::VecDeque, sync::mpsc, thread};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn get_wasapi_devices() -> Res<Vec<wasapi::Device>> {
    let direction = &wasapi::Direction::Capture;
    let devices = wasapi::DeviceCollection::new(direction)?
        .into_iter()
        .map(|device| device.unwrap())
        .collect();
    Ok(devices)
}

fn get_wasapi_device(device_id: &str) -> Res<wasapi::Device> {
    let device = get_wasapi_devices()?
        .into_iter()
        .find(|device| device.get_id().unwrap() == device_id)
        .unwrap();
    Ok(device)
}

fn get_default_device_id() -> Res<String> {
    let direction = &wasapi::Direction::Capture;
    let device = wasapi::get_default_device(direction)?;
    let device_id = device.get_id()?;
    Ok(device_id)
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub samplerate: usize,
}

impl From<&wasapi::Device> for Device {
    fn from(device: &wasapi::Device) -> Self {
        let audio_client = device.get_iaudioclient().unwrap();
        let mixformat = audio_client.get_mixformat().unwrap();
        let samplerate = mixformat.get_samplespersec() as usize;
        Self {
            id: device.get_id().unwrap(),
            name: device.get_friendlyname().unwrap(),
            samplerate,
        }
    }
}

impl Device {
    pub fn capturer(&self, chunksize: usize) -> Capturer {
        Capturer::new(self.clone(), chunksize)
    }
}

pub struct DeviceList {
    pub devices: Vec<Device>,
    pub index: usize,
}

impl DeviceList {
    pub fn new() -> Self {
        let devices: Vec<Device> = get_wasapi_devices()
            .unwrap()
            .iter()
            .map(Into::into)
            .collect();
        let default_device_id = get_default_device_id().unwrap();
        let index = devices
            .iter()
            .position(|device| device.id == default_device_id)
            .unwrap();
        Self { devices, index }
    }
    pub fn device(&self) -> &Device {
        &self.devices[self.index]
    }
}

pub struct Sound {
    pub samples: Vec<f32>,
    pub samplerate: usize,
}

fn capture_loop(
    device: Device,
    tx: mpsc::SyncSender<Sound>,
    samplerate: usize,
    chunksize: usize,
) -> Res<()> {
    let device = get_wasapi_device(&device.id)?;
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
            let sound = Sound {
                samples: chunk,
                samplerate,
            };
            if tx.send(sound).is_err() {
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
    pub rx: mpsc::Receiver<Sound>,
}

impl Capturer {
    fn new(device: Device, chunksize: usize) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let samplerate = device.samplerate;
            capture_loop(device, tx, samplerate, chunksize).unwrap();
        });
        Self { rx }
    }
}
