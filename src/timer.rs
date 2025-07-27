use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub struct Timer {
    stop: mpsc::Sender<()>,
    timer_thread: Option<thread::JoinHandle<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

const UPDATE_FREQUENCY_HZ: u64 = 60;
const INTERVAL: Duration = Duration::from_micros(1_000_000 / UPDATE_FREQUENCY_HZ);

impl Timer {
    pub fn new(device_id: &String) -> Self {
        let (stop, stop_receiver) = mpsc::channel();
        let (interval, interval_receiver) = mpsc::channel();
        let timer_thread = thread::spawn(move || loop {
            let stopped = stop_receiver.recv_timeout(INTERVAL).is_ok();
            if stopped {
                drop(interval);
                break;
            }
            interval.send(()).unwrap();
        })
        .into();
        let message = device_id.to_owned();
        let worker = thread::spawn(move || {
            let mut count = 0;
            for _ in interval_receiver {
                println!("{}, {}", message, count);
                count += 1;
            }
        })
        .into();
        Self {
            stop,
            timer_thread,
            worker,
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.stop.send(()).unwrap();
        if let Some(h) = self.timer_thread.take() {
            h.join().unwrap();
        }
        if let Some(h) = self.worker.take() {
            h.join().unwrap();
        }
    }
}
