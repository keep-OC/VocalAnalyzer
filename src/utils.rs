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

pub trait Elipsis {
    fn elipsis(&mut self, len: usize);
}

impl Elipsis for String {
    fn elipsis(&mut self, len: usize) {
        if self.len() > len {
            let boundary = (0..len)
                .rev()
                .find(|&i| self.is_char_boundary(i))
                .unwrap_or(0);
            self.replace_range(boundary.., "…");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_elipsis() {
        let mut s = "Hello, world!".to_string();
        s.elipsis(5);
        assert_eq!(s, "Hell…");

        let mut s2 = "Short".to_string();
        s2.elipsis(5);
        assert_eq!(s2, "Short");

        // "あいうえお".len() == 15
        let mut s3 = "あいうえお".to_string();
        s3.elipsis(5);
        assert_eq!(s3, "あ…");
    }
}
