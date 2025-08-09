use rustfft::num_traits::{Float, Num};

pub fn normalize<T: Num + Copy>(v: T, min: T, max: T) -> T {
    (v - min) / (max - min)
}

pub fn lerp<T: Num + Copy>(a: T, b: T, t: T) -> T {
    a + (b - a) * t
}

pub fn to_db<T: Float>(v: T) -> T {
    v.log10() * T::from(20.0).unwrap()
}

pub fn from_db<T: Float>(db: T) -> T {
    T::from(20.0).unwrap().powf(db / T::from(20.0).unwrap())
}
