use rustfft::num_traits::Num;

pub fn normalize<T: Num + Copy>(v: T, min: T, max: T) -> T {
    (v - min) / (max - min)
}

pub fn lerp<T: Num + Copy>(a: T, b: T, t: T) -> T {
    a + (b - a) * t
}
