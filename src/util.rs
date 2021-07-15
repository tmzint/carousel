use nalgebra::{Isometry2, Point2, Vector2};
use std::ops::{Deref, DerefMut};

pub type HashMap<K, T> = std::collections::HashMap<K, T, ahash::RandomState>;
pub type HashSet<T> = std::collections::HashSet<T, ahash::RandomState>;
pub type IndexMap<K, T> = indexmap::map::IndexMap<K, T, ahash::RandomState>;
pub type IndexSet<T> = indexmap::set::IndexSet<T, ahash::RandomState>;

#[macro_export]
macro_rules! fn_expr {
    ($return_type:ty : $body:expr) => {
        (|| -> $return_type { $body })()
    };
    ($body:expr) => {
        (|| $body)()
    };
}

#[macro_export]
macro_rules! hashmap {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            let mut _map = crate::util::HashMap::default();
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}

#[macro_export]
macro_rules! indexmap {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            let mut _map = crate::util::IndexMap::default();
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}

#[macro_export]
macro_rules! hashset {
    ($($value:expr),* $(,)?) => {
        {
            let mut _set = crate::util::HashSet::default();
            $(
                let _ = _set.insert($value);
            )*
            _set
        }
    };
}

#[macro_export]
macro_rules! indexset {
    ($($value:expr),* $(,)?) => {
        {
            let mut _set = crate::util::IndexSet::default();
            $(
                let _ = _set.insert($value);
            )*
            _set
        }
    };
}

#[macro_export]
macro_rules! some_or_return {
    ($body:expr, $return_fn:expr) => {
        match $body {
            Some(r) => r,
            None => {
                return $return_fn();
            }
        }
    };
    ($body:expr) => {
        match $body {
            Some(r) => r,
            None => {
                return;
            }
        }
    };
}

#[macro_export]
macro_rules! ok_or_return {
    ($body:expr, $return_fn:expr) => {
        match $body {
            Ok(r) => r,
            Err(_) => {
                return $return_fn();
            }
        }
    };
    ($body:expr) => {
        match $body {
            Ok(r) => r,
            Err(_) => {
                return;
            }
        }
    };
}

#[macro_export]
macro_rules! some_or_continue {
    ($body:expr) => {
        match $body {
            Some(r) => r,
            None => {
                continue;
            }
        }
    };
}

#[macro_export]
macro_rules! ok_or_continue {
    ($body:expr) => {
        match $body {
            Ok(r) => r,
            Err(_) => {
                continue;
            }
        }
    };
}

#[macro_export]
macro_rules! some_or_break {
    ($body:expr) => {
        match $body {
            Some(r) => r,
            None => {
                break;
            }
        }
    };
}

#[macro_export]
macro_rules! ok_or_break {
    ($body:expr) => {
        match $body {
            Ok(r) => r,
            Err(_) => {
                break;
            }
        }
    };
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Copy, Clone)]
pub enum OrderWindow<T> {
    Start,
    Value(T),
    End,
}

impl<T> OrderWindow<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Self::Value(value)
    }

    #[inline]
    pub fn as_option(&self) -> Option<&T> {
        match self {
            OrderWindow::Start => None,
            OrderWindow::Value(t) => Some(t),
            OrderWindow::End => None,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct Counted<T> {
    t: T,
    count: u64,
}

impl<T> Counted<T> {
    #[inline]
    pub fn one(t: T) -> Self {
        Self { t, count: 1 }
    }

    #[inline]
    pub fn inc(&mut self) -> u64 {
        self.count += 1;
        self.count
    }

    #[inline]
    pub fn dec(&mut self) -> u64 {
        self.count -= 1;
        self.count
    }
}

impl<T> Deref for Counted<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.t
    }
}

impl<T> DerefMut for Counted<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.t
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Bounds {
    pub size: Vector2<f32>,
    pub isometry: Isometry2<f32>,
}

pub trait Bounded {
    fn bounds(&self) -> Bounds;

    #[inline]
    fn contains(&self, point: Point2<f32>) -> bool {
        let bounds = self.bounds();
        let half_extends = bounds.size / 2.0;

        let point = bounds.isometry.inverse() * point;
        // see https://math.stackexchange.com/questions/1805724/detect-if-point-is-within-rotated-rectangles-bounds
        let o = half_extends;
        let w = Vector2::new(half_extends.x, -half_extends.y);
        let h = Vector2::new(-half_extends.x, half_extends.y);

        let mut xu = w.x - o.x;
        let mut yu = w.y - o.y;
        let mut xv = h.x - o.x;
        let mut yv = h.y - o.y;
        let mut l = xu * yv - xv * yu;
        if l < 0.0 {
            l = -l;
            xu = -xu;
            yv = -yv;
        } else {
            xv = -xv;
            yu = -yu;
        }

        let u = (point.x - o.x) * yv + (point.y - o.y) * xv;
        if u < 0.0 || u > l {
            return false;
        }

        let v = (point.x - o.x) * yu + (point.y - o.x) * xu;
        !(v < 0.0 || v > l)
    }
}
