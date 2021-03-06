use std::cmp::Ordering;
use std::convert::{From, TryFrom};
use std::fmt::{self, Display, Write};
use std::mem;
use std::ops::{
    Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign,
};

use num_bigint::BigInt;
use num_rational::{BigRational, Rational64};
use num_traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Num, One, Signed, Zero};

use integer::Integer;

mod integer;

const PRECISION: usize = 10;

#[derive(Clone, Eq, PartialEq, Ord)]
pub(crate) enum Number {
    Machine(Rational64),
    Big(BigRational),
}

impl Number {
    pub const fn new_machine(val: Rational64) -> Number {
        Number::Machine(val)
    }

    pub const fn new_big(val: BigRational) -> Number {
        Number::Big(val)
    }

    pub fn to_integer(&self) -> Integer {
        match self {
            Self::Machine(val) => Integer::Machine(val.to_integer()),
            Self::Big(val) => Integer::Big(val.to_integer()),
        }
    }

    pub fn machine_ratio<A: Into<i64>, B: Into<i64>>(a: A, b: B) -> Self {
        Number::new_machine(Rational64::new(a.into(), b.into()))
    }

    #[allow(dead_code)]
    pub fn big_ratio<A: Into<BigInt>, B: Into<BigInt>>(a: A, b: B) -> Self {
        Number::new_big(BigRational::new(a.into(), b.into()))
    }

    pub fn round(&self) -> Self {
        match self {
            Self::Machine(val) => Self::Machine(val.round()),
            Self::Big(val) => Self::Big(val.round()),
        }
    }

    pub fn ceil(&self) -> Self {
        match self {
            Self::Machine(val) => Self::Machine(val.ceil()),
            Self::Big(val) => Self::Big(val.ceil()),
        }
    }

    pub fn floor(&self) -> Self {
        match self {
            Self::Machine(val) => Self::Machine(val.floor()),
            Self::Big(val) => Self::Big(val.floor()),
        }
    }

    pub fn abs(&self) -> Self {
        match self {
            Self::Machine(val) => Self::Machine(val.abs()),
            Self::Big(val) => Self::Big(val.abs()),
        }
    }

    pub fn is_decimal(&self) -> bool {
        match self {
            Self::Machine(v) => !v.is_integer(),
            Self::Big(v) => !v.is_integer(),
        }
    }

    pub fn fract(&mut self) -> Number {
        match self {
            Self::Machine(v) => Number::new_machine(v.fract()),
            Self::Big(v) => Number::new_big(v.fract()),
        }
    }

    pub fn clamp<A: Into<Number> + Zero, B: Into<Number>>(self, min: A, max: B) -> Self {
        let max = max.into();
        if self > max {
            return max;
        }

        if min.is_zero() && self.is_negative() {
            return Number::zero();
        }

        let min = min.into();
        if self < min {
            return min;
        }

        self
    }
}

impl Default for Number {
    fn default() -> Self {
        Self::zero()
    }
}

impl Zero for Number {
    fn zero() -> Self {
        Number::new_machine(Rational64::from_integer(0))
    }

    fn is_zero(&self) -> bool {
        match self {
            Self::Machine(v) => v.is_zero(),
            Self::Big(v) => v.is_zero(),
        }
    }
}

impl One for Number {
    fn one() -> Self {
        Number::new_machine(Rational64::from_integer(1))
    }

    fn is_one(&self) -> bool {
        match self {
            Self::Machine(v) => v.is_one(),
            Self::Big(v) => v.is_one(),
        }
    }
}

impl Num for Number {
    type FromStrRadixErr = ();
    #[cold]
    fn from_str_radix(_: &str, _: u32) -> Result<Self, Self::FromStrRadixErr> {
        unreachable!()
    }
}

impl Signed for Number {
    fn abs(&self) -> Self {
        self.abs()
    }

    #[cold]
    fn abs_sub(&self, _: &Self) -> Self {
        unreachable!()
    }

    #[cold]
    fn signum(&self) -> Self {
        if self.is_zero() {
            Self::zero()
        } else if self.is_positive() {
            Self::one()
        } else {
            -Self::one()
        }
    }

    fn is_positive(&self) -> bool {
        match self {
            Self::Machine(v) => v.is_positive(),
            Self::Big(v) => v.is_positive(),
        }
    }

    fn is_negative(&self) -> bool {
        match self {
            Self::Machine(v) => v.is_negative(),
            Self::Big(v) => v.is_negative(),
        }
    }
}

macro_rules! from_integer {
    ($ty:ty) => {
        impl From<$ty> for Number {
            fn from(b: $ty) -> Self {
                if let Ok(v) = i64::try_from(b) {
                    Number::Machine(Rational64::from_integer(v))
                } else {
                    Number::Big(BigRational::from_integer(BigInt::from(b)))
                }
            }
        }
    };
}

macro_rules! from_smaller_integer {
    ($ty:ty) => {
        impl From<$ty> for Number {
            fn from(val: $ty) -> Self {
                Number::new_machine(Rational64::from_integer(val as i64))
            }
        }
    };
}

impl From<i64> for Number {
    fn from(val: i64) -> Self {
        Number::new_machine(Rational64::from_integer(val))
    }
}

// todo: implement std::convertTryFrom instead
impl From<f64> for Number {
    fn from(b: f64) -> Self {
        Number::Big(BigRational::from_float(b).unwrap())
    }
}

from_integer!(usize);
from_integer!(isize);
from_smaller_integer!(i32);
from_smaller_integer!(u32);
from_smaller_integer!(u8);

impl fmt::Debug for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Number {{ {} }}", self)
    }
}

impl Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut whole = self.to_integer().abs();
        let has_decimal = self.is_decimal();
        let mut frac = self.abs().fract();
        let mut dec = String::with_capacity(if has_decimal { PRECISION + 1 } else { 0 });
        if has_decimal {
            for _ in 0..(PRECISION - 1) {
                frac *= Self::from(10);
                write!(dec, "{}", frac.to_integer())?;
                frac = frac.fract();
                if frac.is_zero() {
                    break;
                }
            }
            if !frac.is_zero() {
                let end = (frac * Self::from(10)).round().to_integer();
                if end.is_ten() {
                    loop {
                        match dec.pop() {
                            Some('9') => continue,
                            Some(c) => {
                                dec.push(char::from(c as u8 + 1));
                                break;
                            }
                            None => {
                                whole += 1;
                                break;
                            }
                        }
                    }
                } else if end.is_zero() {
                    loop {
                        match dec.pop() {
                            Some('0') => continue,
                            Some(c) => {
                                dec.push(c);
                                break;
                            }
                            None => break,
                        }
                    }
                } else {
                    write!(dec, "{}", end)?;
                }
            }
        }

        if self.is_negative() && (!whole.is_zero() || !dec.is_empty()) {
            f.write_char('-')?;
        }
        write!(f, "{}", whole)?;
        if !dec.is_empty() {
            f.write_char('.')?;
            write!(f, "{}", dec)?;
        }
        Ok(())
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self {
            Self::Machine(val1) => match other {
                Self::Machine(val2) => val1.partial_cmp(val2),
                Self::Big(val2) => {
                    let tuple: (i64, i64) = (*val1).into();
                    BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1))
                        .partial_cmp(val2)
                }
            },
            Self::Big(val1) => match other {
                Self::Machine(val2) => {
                    let tuple: (i64, i64) = (*val2).into();
                    val1.partial_cmp(&BigRational::new_raw(
                        BigInt::from(tuple.0),
                        BigInt::from(tuple.1),
                    ))
                }
                Self::Big(val2) => val1.partial_cmp(val2),
            },
        }
    }
}

impl Add for Number {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        match self {
            Self::Machine(val1) => match other {
                Self::Machine(val2) => match val1.checked_add(&val2) {
                    Some(v) => Self::Machine(v),
                    None => {
                        let tuple1: (i64, i64) = val1.into();
                        let tuple2: (i64, i64) = val2.into();
                        Self::Big(
                            BigRational::new_raw(BigInt::from(tuple1.0), BigInt::from(tuple1.1))
                                + BigRational::new_raw(
                                    BigInt::from(tuple2.0),
                                    BigInt::from(tuple2.1),
                                ),
                        )
                    }
                },
                Self::Big(val2) => {
                    let tuple: (i64, i64) = val1.into();
                    Self::Big(
                        BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)) + val2,
                    )
                }
            },
            Self::Big(val1) => match other {
                Self::Big(val2) => Self::Big(val1 + val2),
                Self::Machine(val2) => {
                    let tuple: (i64, i64) = val2.into();
                    Self::Big(
                        val1 + BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)),
                    )
                }
            },
        }
    }
}

impl Add<&Self> for Number {
    type Output = Self;

    fn add(self, other: &Self) -> Self {
        match self {
            Self::Machine(val1) => match other {
                Self::Machine(val2) => match val1.checked_add(val2) {
                    Some(v) => Self::Machine(v),
                    None => {
                        let tuple1: (i64, i64) = val1.into();
                        let tuple2: (i64, i64) = (*val2).into();
                        Self::Big(
                            BigRational::new_raw(BigInt::from(tuple1.0), BigInt::from(tuple1.1))
                                + BigRational::new_raw(
                                    BigInt::from(tuple2.0),
                                    BigInt::from(tuple2.1),
                                ),
                        )
                    }
                },
                Self::Big(val2) => {
                    let tuple: (i64, i64) = val1.into();
                    Self::Big(
                        BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)) + val2,
                    )
                }
            },
            Self::Big(val1) => match other {
                Self::Big(val2) => Self::Big(val1 + val2),
                Self::Machine(val2) => {
                    let tuple: (i64, i64) = (*val2).into();
                    Self::Big(
                        val1 + BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)),
                    )
                }
            },
        }
    }
}

impl AddAssign for Number {
    fn add_assign(&mut self, other: Self) {
        let tmp = mem::take(self);
        *self = tmp + other;
    }
}

impl Sub for Number {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        match self {
            Self::Machine(val1) => match other {
                Self::Machine(val2) => match val1.checked_sub(&val2) {
                    Some(v) => Self::Machine(v),
                    None => {
                        let tuple1: (i64, i64) = val1.into();
                        let tuple2: (i64, i64) = val2.into();
                        Self::Big(
                            BigRational::new_raw(BigInt::from(tuple1.0), BigInt::from(tuple1.1))
                                - BigRational::new_raw(
                                    BigInt::from(tuple2.0),
                                    BigInt::from(tuple2.1),
                                ),
                        )
                    }
                },
                Self::Big(val2) => {
                    let tuple: (i64, i64) = val1.into();
                    Self::Big(
                        BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)) - val2,
                    )
                }
            },
            Self::Big(val1) => match other {
                Self::Big(val2) => Self::Big(val1 - val2),
                Self::Machine(val2) => {
                    let tuple: (i64, i64) = val2.into();
                    Self::Big(
                        val1 - BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)),
                    )
                }
            },
        }
    }
}

impl SubAssign for Number {
    fn sub_assign(&mut self, other: Self) {
        let tmp = mem::take(self);
        *self = tmp - other;
    }
}

impl Mul for Number {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        match self {
            Self::Machine(val1) => match other {
                Self::Machine(val2) => match val1.checked_mul(&val2) {
                    Some(v) => Self::Machine(v),
                    None => {
                        let tuple1: (i64, i64) = val1.into();
                        let tuple2: (i64, i64) = val2.into();
                        Self::Big(
                            BigRational::new_raw(BigInt::from(tuple1.0), BigInt::from(tuple1.1))
                                * BigRational::new_raw(
                                    BigInt::from(tuple2.0),
                                    BigInt::from(tuple2.1),
                                ),
                        )
                    }
                },
                Self::Big(val2) => {
                    let tuple: (i64, i64) = val1.into();
                    Self::Big(
                        BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)) * val2,
                    )
                }
            },
            Self::Big(val1) => match other {
                Self::Big(val2) => Self::Big(val1 * val2),
                Self::Machine(val2) => {
                    let tuple: (i64, i64) = val2.into();
                    Self::Big(
                        val1 * BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)),
                    )
                }
            },
        }
    }
}

impl MulAssign for Number {
    fn mul_assign(&mut self, other: Self) {
        let tmp = mem::take(self);
        *self = tmp * other;
    }
}

impl Div for Number {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        match self {
            Self::Machine(val1) => match other {
                Self::Machine(val2) => match val1.checked_div(&val2) {
                    Some(v) => Self::Machine(v),
                    None => {
                        let tuple1: (i64, i64) = val1.into();
                        let tuple2: (i64, i64) = val2.into();
                        Self::Big(
                            BigRational::new_raw(BigInt::from(tuple1.0), BigInt::from(tuple1.1))
                                / BigRational::new_raw(
                                    BigInt::from(tuple2.0),
                                    BigInt::from(tuple2.1),
                                ),
                        )
                    }
                },
                Self::Big(val2) => {
                    let tuple: (i64, i64) = val1.into();
                    Self::Big(
                        BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)) / val2,
                    )
                }
            },
            Self::Big(val1) => match other {
                Self::Big(val2) => Self::Big(val1 / val2),
                Self::Machine(val2) => {
                    let tuple: (i64, i64) = val2.into();
                    Self::Big(
                        val1 / BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)),
                    )
                }
            },
        }
    }
}

impl DivAssign for Number {
    fn div_assign(&mut self, other: Self) {
        let tmp = mem::take(self);
        *self = tmp / other;
    }
}

impl Rem for Number {
    type Output = Self;

    fn rem(self, other: Self) -> Self {
        match self {
            Self::Machine(val1) => match other {
                // todo: checked_rem for ratio?
                Self::Machine(val2) => {
                    let tuple1: (i64, i64) = val1.into();
                    let tuple2: (i64, i64) = val2.into();
                    Self::Big(
                        BigRational::new_raw(BigInt::from(tuple1.0), BigInt::from(tuple1.1))
                            % BigRational::new_raw(BigInt::from(tuple2.0), BigInt::from(tuple2.1)),
                    )
                }
                Self::Big(val2) => {
                    let tuple: (i64, i64) = val1.into();
                    Self::Big(
                        BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)) % val2,
                    )
                }
            },
            Self::Big(val1) => match other {
                Self::Big(val2) => Self::Big(val1 % val2),
                Self::Machine(val2) => {
                    let tuple: (i64, i64) = val2.into();
                    Self::Big(
                        val1 % BigRational::new_raw(BigInt::from(tuple.0), BigInt::from(tuple.1)),
                    )
                }
            },
        }
    }
}

impl RemAssign for Number {
    fn rem_assign(&mut self, other: Self) {
        let tmp = mem::take(self);
        *self = tmp % other;
    }
}

impl Neg for Number {
    type Output = Self;

    fn neg(self) -> Self {
        match self {
            Self::Machine(v) => Self::Machine(-v),
            Self::Big(v) => Self::Big(-v),
        }
    }
}
