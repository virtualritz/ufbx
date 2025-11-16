//! High-precision floating point parsing
//!
//! This module implements exact decimal-to-binary floating point conversion
//! matching the behavior of the ufbx C implementation. It uses arbitrary precision
//! arithmetic (via BigInt) to ensure exact rounding in all cases.
//!
//! # Algorithm Overview
//!
//! The parser uses a two-phase approach:
//! 1. **Fast path**: For simple cases (small integers, common decimals), use native f64 arithmetic
//! 2. **Slow path**: For complex cases, use BigInt arithmetic to compute exact result
//!
//! The slow path implements proper decimal-to-binary conversion:
//! - Parse digits into a BigInt mantissa
//! - Handle negative exponents via division
//! - Handle positive exponents via multiplication by powers of 5
//! - Extract the high bits and round correctly
//!
//! # Precision Guarantees
//!
//! - Correctly rounded to nearest (ties to even) for all inputs
//! - Handles denormalized numbers
//! - Handles special values: +/-Infinity, NaN, -0.0
//! - Matches IEEE 754 rounding behavior exactly

use core::f64;

/// Maximum number of BigInt limbs needed for float parsing
const MAX_LIMBS: usize = 14;

/// Bits per BigInt limb
const LIMB_BITS: u32 = 32;

/// Bits in the accumulator (used for intermediate calculations)
const ACCUM_BITS: u32 = LIMB_BITS * 2;

/// Maximum value for a single limb
const LIMB_MAX: u64 = (1u64 << LIMB_BITS) - 1;

/// Type for a single limb in BigInt
type Limb = u32;

/// Type for accumulator (intermediate calculations)
type Accum = u64;

/// Arbitrary precision integer for exact float parsing
///
/// This is a simple big integer implementation using base-2^32 limbs.
/// It only supports the operations needed for float parsing:
/// - Multiply-add (MAD)
/// - Division
/// - Left shift
/// - High bit extraction
#[derive(Clone)]
struct BigInt {
    /// Limbs in little-endian order (least significant first)
    limbs: Vec<Limb>,
    /// Current number of used limbs
    length: usize,
}

impl BigInt {
    /// Create a new BigInt with the given capacity
    fn new(capacity: usize) -> Self {
        Self {
            limbs: vec![0; capacity],
            length: 0,
        }
    }

    /// Multiply-Add-Digit: self = self * multiplicand + addend
    ///
    /// This is the core operation for building up the mantissa during parsing.
    /// It's equivalent to: self *= multiplicand; self += addend;
    fn mad(&mut self, multiplicand: Accum, addend: Accum) {
        debug_assert!((multiplicand | addend) >> (ACCUM_BITS - 1) == 0);

        let m_lo = multiplicand as Limb;
        let m_hi = (multiplicand >> LIMB_BITS) as Limb;
        let mut carry = addend;

        for i in 0..self.length {
            let limb = self.limbs[i] as Accum;
            let lo = limb * m_lo as Accum + (carry & LIMB_MAX);
            let hi = limb * m_hi as Accum;
            self.limbs[i] = lo as Limb;
            carry = (carry >> 32) + (lo >> 32) + hi;
        }

        while carry != 0 {
            self.limbs[self.length] = carry as Limb;
            self.length += 1;
            debug_assert!(self.length < self.limbs.len());
            carry >>= 32;
        }
    }

    /// Divide this BigInt by another, storing quotient in q and returning whether there's a remainder
    ///
    /// This implements Knuth's Algorithm D from TAOCP Volume 2.
    /// Returns true if there's a non-zero remainder (needed for correct rounding).
    fn div(&self, divisor: &BigInt, quotient: &mut BigInt) -> bool {
        let n = divisor.length as i32;
        let m = self.length as i32 - n;
        let v_hi = divisor.limbs[divisor.length - 1];

        debug_assert!(n >= 2 && m >= 1);
        debug_assert!(v_hi >> (LIMB_BITS - 1) != 0);
        debug_assert!(self.limbs[n as usize + m as usize - 1] >> (LIMB_BITS - 1) == 0);

        let mut un = self.limbs.clone();
        let vn = &divisor.limbs;
        un[(n + m) as usize] = 0;
        quotient.length = 0;

        for j in (0..m).rev() {
            let u_hi = ((un[(n + j) as usize] as Accum) << LIMB_BITS) | un[(n + j - 1) as usize] as Accum;
            let mut qhat = u_hi / v_hi as Accum;
            let mut rhat = u_hi % v_hi as Accum;

            // Refine estimate
            while qhat >> LIMB_BITS != 0 || qhat * vn[(n - 2) as usize] as Accum > ((rhat << LIMB_BITS) | un[(j + n - 2) as usize] as Accum) {
                qhat -= 1;
                rhat += v_hi as Accum;
                if rhat >> LIMB_BITS != 0 {
                    break;
                }
            }

            // Multiply and subtract
            let mut carry = 0u32;
            for i in 0..n {
                let p = qhat * vn[i as usize] as Accum;
                let t = un[(i + j) as usize] as i64 - carry as i64 - (p as Limb) as i64;
                un[(i + j) as usize] = t as Limb;
                carry = ((p >> LIMB_BITS) as i64 - (t >> LIMB_BITS)) as u32;
            }

            let t = un[(j + n) as usize] as i64 - carry as i64;
            un[(j + n) as usize] = t as Limb;

            // Add back if we subtracted too much
            if t >> LIMB_BITS != 0 {
                qhat -= 1;
                carry = 0;
                for i in 0..n {
                    let t = un[(i + j) as usize] as Accum + vn[i as usize] as Accum + carry as Accum;
                    un[(i + j) as usize] = t as Limb;
                    carry = (t >> LIMB_BITS) as Limb;
                }
                un[(j + n) as usize] = un[(j + n) as usize].wrapping_add(carry);
            }

            quotient.limbs[j as usize] = qhat as Limb;
            if qhat != 0 && quotient.length == 0 {
                debug_assert!((j as usize + 1) < quotient.limbs.len());
                quotient.length = j as usize + 1;
            }
        }

        // Check for remainder
        for i in 0..n {
            if un[i as usize] != 0 {
                return true;
            }
        }
        false
    }

    /// Multiply by a power of 5
    fn mul_pow5(&mut self, mut power: u32) {
        while power > 27 {
            self.mad(POW5_TAB[27], 0);
            power -= 27;
        }
        self.mad(POW5_TAB[power as usize], 0);
    }

    /// Shift left by the given number of bits
    fn shift_left(&mut self, amount: u32) {
        let words = (amount / LIMB_BITS) as usize;
        let bits = amount % LIMB_BITS;
        let bits_down = LIMB_BITS - bits - 1;

        debug_assert!(self.length + words + 1 < self.limbs.len());

        // Check if we need an extra limb
        let needs_extra = (self.limbs[self.length - 1] >> 1 >> bits_down) != 0;
        let new_length = self.length + words + if needs_extra { 1 } else { 0 };
        self.limbs[self.length] = 0;

        // Fast path for small shifts
        if self.length <= 3 && words <= 3 {
            let l0 = self.limbs[0];
            let l1 = if self.length >= 2 { self.limbs[1] } else { 0 };
            let l2 = if self.length >= 3 { self.limbs[2] } else { 0 };

            self.limbs[0] = 0;
            self.limbs[1] = 0;
            self.limbs[2] = 0;

            self.limbs[words] = l0 << bits;
            self.limbs[words + 1] = (l1 << bits) | (l0 >> 1 >> bits_down);
            self.limbs[words + 2] = (l2 << bits) | (l1 >> 1 >> bits_down);
            self.limbs[words + 3] = l2 >> 1 >> bits_down;
        } else {
            // General case
            for i in (1..=self.length).rev() {
                self.limbs[i + words] = (self.limbs[i] << bits) | (self.limbs[i - 1] >> 1 >> bits_down);
            }
            self.limbs[words] = self.limbs[0] << bits;
            for i in 0..words {
                self.limbs[i] = 0;
            }
        }

        self.length = new_length;
    }

    /// Get the limb at the given index from the top
    fn top_limb(&self, index: usize) -> Limb {
        if index < self.length {
            self.limbs[self.length - 1 - index]
        } else {
            0
        }
    }

    /// Extract high 64 bits for conversion to float
    ///
    /// Returns the high 64 bits, updates the exponent, and sets tail flag if there are non-zero low bits
    fn extract_high(&self, exponent: &mut i32, tail: &mut bool) -> u64 {
        debug_assert!(self.length != 0);

        let mut result = 0u64;
        let limb_count = 64 / LIMB_BITS;

        for i in 0..limb_count {
            result = (result << LIMB_BITS) | self.top_limb(i as usize) as u64;
        }

        let shift = result.leading_zeros();
        result <<= shift;

        let lo = self.top_limb(limb_count as usize);
        if shift > 0 {
            result |= (lo as u64) >> (LIMB_BITS - shift);
        }

        *tail |= (lo << shift) != 0;
        for i in (limb_count + 1)..(self.length as u32) {
            *tail |= self.top_limb(i as usize) != 0;
        }

        *exponent += (self.length as i32 * LIMB_BITS as i32 - shift as i32 - 1);
        result
    }
}

/// Precomputed powers of 5 up to 5^27
///
/// These are used to efficiently multiply by powers of 10 (= 2^n * 5^n)
const POW5_TAB: [u64; 28] = [
    0x1, 0x5, 0x19, 0x7d, 0x271, 0xc35, 0x3d09, 0x1312d, 0x5f5e1,
    0x1dcd65, 0x9502f9, 0x2e90edd, 0xe8d4a51, 0x48c27395, 0x16bcc41e9, 0x71afd498d,
    0x2386f26fc1, 0xb1a2bc2ec5, 0x3782dace9d9, 0x1158e460913d, 0x56bc75e2d631, 0x1b1ae4d6e2ef5,
    0x878678326eac9, 0x2a5a058fc295ed, 0xd3c21bcecceda1, 0x422ca8b0a00a425, 0x14adf4b7320334b9, 0x6765c793fa10079d,
];

/// Precomputed powers of 10 for fast path
///
/// These allow us to quickly parse common decimal values without BigInt arithmetic
const POW10_TAB_F64: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// Shift right with rounding (round to nearest, ties to even)
fn shift_right_round(value: u64, shift: u32, tail: bool) -> u64 {
    if shift == 0 {
        return value;
    }
    if shift > 64 {
        return 0;
    }

    let result = value >> (shift - 1);
    let tail_mask = (1u64 << (shift - 1)) - 1;

    let r_odd = (result & 0x2) != 0;
    let r_round = (result & 0x1) != 0;
    let r_tail = tail || (value & tail_mask) != 0;
    let round_bit = if r_round && (r_odd || r_tail) { 1 } else { 0 };

    (result >> 1) + round_bit
}

/// Scan for a case-insensitive match
fn scan_ignorecase(p: &[u8], fmt: &[u8]) -> bool {
    if p.len() < fmt.len() {
        return false;
    }
    for i in 0..fmt.len() {
        if (p[i] | 0x20) != fmt[i] {
            return false;
        }
    }
    true
}

/// Parse special floating point values: infinity and NaN
///
/// Supports multiple formats:
/// - Standard: "inf", "infinity", "nan", "nan(...)"
/// - MSVC legacy: "1.#INF", "1.#NAN", "1.#IND"
fn parse_inf_nan(str: &[u8]) -> Option<(f64, usize)> {
    if str.is_empty() {
        return None;
    }

    let mut pos = 0;
    let negative = if str[pos] == b'+' || str[pos] == b'-' {
        let neg = str[pos] == b'-';
        pos += 1;
        neg
    } else {
        false
    };

    let mut top_bits = 0u32;

    // Check for legacy MSVC format: 1.#INF / 1.#NAN
    if str.len() - pos >= 3 && str[pos] >= b'0' && str[pos] <= b'9' && str[pos + 1] == b'.' && str[pos + 2] == b'#' {
        pos += 3;
        if scan_ignorecase(&str[pos..], b"inf") {
            pos += 3;
            top_bits = 0x7ff0;
        } else if scan_ignorecase(&str[pos..], b"nan") || scan_ignorecase(&str[pos..], b"ind") {
            pos += 3;
            top_bits = 0x7ff8;
        } else {
            return None;
        }
        // Skip trailing digits
        while pos < str.len() && str[pos] >= b'0' && str[pos] <= b'9' {
            pos += 1;
        }
    } else {
        // Standard format
        if scan_ignorecase(&str[pos..], b"nan") {
            pos += 3;
            top_bits = 0x7ff8;

            // Handle nan(n-char-sequence)
            if pos < str.len() && str[pos] == b'(' {
                pos += 1;
                while pos < str.len() && str[pos] != b')' {
                    let c = str[pos];
                    if !((c >= b'0' && c <= b'9') || (c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z')) {
                        return None;
                    }
                    pos += 1;
                }
                if pos >= str.len() {
                    return None;
                }
                pos += 1; // Skip ')'
            }
        } else if scan_ignorecase(&str[pos..], b"inf") {
            let full_infinity = scan_ignorecase(&str[pos..], b"infinity");
            pos += if full_infinity { 8 } else { 3 };
            top_bits = 0x7ff0;
        } else {
            return None;
        }
    }

    top_bits |= if negative { 0x8000 } else { 0 };
    let bits = (top_bits as u64) << 48;
    Some((f64::from_bits(bits), pos))
}

/// Flags for parse_double
#[derive(Clone, Copy)]
pub struct ParseFlags {
    /// Allow fast path optimization
    pub allow_fast_path: bool,
    /// Parse as f32 (binary32) instead of f64
    pub as_binary32: bool,
}

impl Default for ParseFlags {
    fn default() -> Self {
        Self {
            allow_fast_path: should_use_fast_path(),
            as_binary32: false,
        }
    }
}

/// Check if we should use the fast path
///
/// The fast path requires:
/// - Double precision evaluation (FLT_EVAL_METHOD == 0 or 1)
/// - Round-to-nearest mode
fn should_use_fast_path() -> bool {
    // Check if 1.0 + eps == 1.0 - eps (round to nearest)
    const EPS: f64 = 2.2250738585072014e-308;
    (1.0 + EPS) == (1.0 - EPS)
}

/// Parse a double-precision float from a string
///
/// This is the main entry point for float parsing. It handles:
/// - Sign parsing
/// - Digit accumulation
/// - Decimal point
/// - Scientific notation (e/E)
/// - Special values (inf, nan, -0)
/// - Fast path for simple values
/// - Exact BigInt-based parsing for complex values
///
/// # Arguments
/// * `str` - The string to parse (will parse until first non-numeric character)
/// * `flags` - Parsing flags (fast path, f32 vs f64)
///
/// # Returns
/// * `Some((value, bytes_parsed))` if successful
/// * `None` if no valid number found
///
/// # Examples
/// ```
/// use ufbx::utils::float_parse::{parse_double, ParseFlags};
///
/// let (val, len) = parse_double(b"3.14159", ParseFlags::default()).unwrap();
/// assert_eq!(len, 7);
/// assert!((val - 3.14159).abs() < 1e-10);
///
/// let (val, _) = parse_double(b"-1.23e-4", ParseFlags::default()).unwrap();
/// assert_eq!(val, -0.000123);
///
/// let (val, _) = parse_double(b"inf", ParseFlags::default()).unwrap();
/// assert!(val.is_infinite() && val.is_sign_positive());
/// ```
pub fn parse_double(str: &[u8], flags: ParseFlags) -> Option<(f64, usize)> {
    if str.is_empty() {
        return None;
    }

    let mut pos = 0;

    // Parse sign
    let negative = if str[pos] == b'+' || str[pos] == b'-' {
        let neg = str[pos] == b'-';
        pos += 1;
        if pos >= str.len() {
            return None;
        }
        neg
    } else {
        false
    };

    let start_pos = pos;

    // Accumulate digits
    let mut digits = 0u64;
    let mut num_digits = 0u32;
    let mut dec_exponent = 0i32;
    let mut has_dot = false;
    let mut digits_valid = true;

    let mut big_mantissa = BigInt::new(42);

    // Parse mantissa
    while pos < str.len() {
        let c = str[pos];
        if c >= b'0' && c <= b'9' {
            if big_mantissa.length < MAX_LIMBS {
                digits = digits * 10 + (c - b'0') as u64;
                num_digits += 1;

                if num_digits >= 18 {
                    debug_assert!((num_digits as usize) < POW5_TAB.len());
                    big_mantissa.mad(POW5_TAB[num_digits as usize] << num_digits, digits);
                    digits = 0;
                    num_digits = 0;
                    digits_valid = false;
                }

                if has_dot {
                    dec_exponent -= 1;
                }
            } else {
                if !has_dot {
                    dec_exponent += 1;
                }
            }
            pos += 1;
        } else if c == b'.' && !has_dot {
            has_dot = true;
            pos += 1;
        } else {
            break;
        }
    }

    // Parse exponent
    if pos < str.len() && (str[pos] == b'e' || str[pos] == b'E') {
        pos += 1;
        if pos >= str.len() {
            return None;
        }

        let exp_negative = if str[pos] == b'+' || str[pos] == b'-' {
            let neg = str[pos] == b'-';
            pos += 1;
            if pos >= str.len() {
                return None;
            }
            neg
        } else {
            false
        };

        let mut exp = 0i32;
        let exp_start = pos;
        while pos < str.len() {
            let c = str[pos];
            if c >= b'0' && c <= b'9' {
                pos += 1;
                exp = exp * 10 + (c - b'0') as i32;
                if exp >= 10000 {
                    break;
                }
            } else {
                break;
            }
        }

        if pos == exp_start {
            return None;
        }

        dec_exponent += if exp_negative { -exp } else { exp };
    }

    // Check for special values
    if pos < str.len() {
        let c = str[pos];
        if c == b'#' || c == b'i' || c == b'I' || c == b'n' || c == b'N' {
            return parse_inf_nan(str);
        }
    }

    // Must have parsed at least one digit
    if pos == start_pos {
        return None;
    }

    // Fast path: simple values that fit in f64 exactly
    if flags.allow_fast_path && big_mantissa.length == 0 && dec_exponent >= -22 && dec_exponent <= 22 && (digits >> 53) == 0 {
        let value = if dec_exponent < 0 {
            digits as f64 / POW10_TAB_F64[(-dec_exponent) as usize]
        } else {
            digits as f64 * POW10_TAB_F64[dec_exponent as usize]
        };
        return Some((if negative { -value } else { value }, pos));
    }

    // Set up BigInt mantissa
    if big_mantissa.length == 0 {
        big_mantissa.limbs[0] = digits as Limb;
        big_mantissa.limbs[1] = (digits >> 32) as Limb;
        big_mantissa.length = if digits >> 32 != 0 { 2 } else if digits != 0 { 1 } else { 0 };

        if big_mantissa.length == 0 {
            return Some((if negative { -0.0 } else { 0.0 }, pos));
        }
    } else {
        debug_assert!((num_digits as usize) < POW5_TAB.len());
        big_mantissa.mad(POW5_TAB[num_digits as usize] << num_digits, digits);
    }

    // Encoding parameters (f64 vs f32)
    let (enc_sign_shift, enc_mantissa_bits, enc_max_exponent) = if flags.as_binary32 {
        (31u32, 24u32, 127i32)
    } else {
        (63u32, 53u32, 1023i32)
    };

    let mut exponent = 0i32;
    let mut tail = false;

    // Handle exponent
    if dec_exponent < 0 {
        // Underflow check
        if dec_exponent + (big_mantissa.length as i32) * 10 <= -325 {
            return Some((if negative { -0.0 } else { 0.0 }, pos));
        }

        // Division by power of 10 = division by power of 5, then shift
        let mut big_divisor = BigInt::new(42);
        let pow5 = (-dec_exponent) as u32;
        let initial_pow5 = pow5.min(27);
        let pow5_value = POW5_TAB[initial_pow5 as usize];
        let remaining_pow5 = pow5 - initial_pow5;
        exponent += dec_exponent;

        // Optimized path for small divisors
        if remaining_pow5 == 0 && digits_valid && (digits >> 63) == 0 {
            let divisor_zeros = pow5_value.leading_zeros();
            let mantissa_zeros = digits.leading_zeros() - 1;
            let divisor_bits = pow5_value << divisor_zeros;
            let mantissa_bits = digits << mantissa_zeros;

            big_divisor.limbs[0] = divisor_bits as Limb;
            big_divisor.limbs[1] = (divisor_bits >> 32) as Limb;
            big_divisor.length = 2;

            big_mantissa.limbs[0] = 0;
            big_mantissa.limbs[1] = 0;
            big_mantissa.limbs[2] = mantissa_bits as Limb;
            big_mantissa.limbs[3] = (mantissa_bits >> 32) as Limb;
            big_mantissa.length = 4;

            exponent += divisor_zeros as i32 - mantissa_zeros as i32 - 64;
        } else {
            // General case
            big_divisor.limbs[0] = pow5_value as Limb;
            big_divisor.limbs[1] = (pow5_value >> 32) as Limb;
            big_divisor.length = if pow5_value >> 32 != 0 { 2 } else { 1 };

            if remaining_pow5 > 0 {
                big_divisor.mul_pow5(remaining_pow5);
            }

            // Normalize divisor
            let divisor_zeros = big_divisor.limbs[big_divisor.length - 1].leading_zeros();
            let divisor_zeros = if big_divisor.length == 1 {
                divisor_zeros + LIMB_BITS
            } else {
                divisor_zeros
            };
            big_divisor.shift_left(divisor_zeros);
            let divisor_bits = big_divisor.length as u32 * LIMB_BITS;

            // Calculate mantissa shift needed
            let mantissa_zeros = big_mantissa.limbs[big_mantissa.length - 1].leading_zeros();
            let mantissa_bits = big_mantissa.length as u32 * LIMB_BITS - mantissa_zeros;
            let mantissa_min_bits = divisor_bits + enc_mantissa_bits + 2;
            let mut mantissa_shift = if mantissa_bits < mantissa_min_bits {
                mantissa_min_bits - mantissa_bits
            } else {
                0
            };

            // Align mantissa to never have a high bit
            if ((mantissa_shift - mantissa_zeros) & (LIMB_BITS - 1)) == 0 {
                mantissa_shift += 1;
            }

            if mantissa_shift > 0 {
                big_mantissa.shift_left(mantissa_shift);
            }

            exponent += divisor_zeros as i32 - mantissa_shift as i32;
        }

        let mut big_quotient = BigInt::new(42);
        tail = big_mantissa.div(&big_divisor, &mut big_quotient);
        big_mantissa = big_quotient;
    } else if dec_exponent > 0 {
        // Overflow check
        if dec_exponent + (big_mantissa.length as i32 - 1) * 9 >= 310 {
            let inf = if negative { f64::NEG_INFINITY } else { f64::INFINITY };
            return Some((inf, pos));
        }

        exponent += dec_exponent;
        big_mantissa.mul_pow5(dec_exponent as u32);
    }

    // Extract high bits and convert to float
    let mantissa = big_mantissa.extract_high(&mut exponent, &mut tail);
    let sign_bit = if negative { 1u64 << enc_sign_shift } else { 0 };

    let mantissa_shift = 64 - enc_mantissa_bits;

    // Handle overflow/underflow
    if exponent > enc_max_exponent {
        let inf = if negative { f64::NEG_INFINITY } else { f64::INFINITY };
        return Some((inf, pos));
    } else if exponent <= -enc_max_exponent {
        // Denormalized number
        let extra_shift = (-enc_max_exponent + 1 - exponent) as u32;
        let mantissa = shift_right_round(mantissa, mantissa_shift + extra_shift, tail);

        if mantissa == 0 {
            return Some((if negative { -0.0 } else { 0.0 }, pos));
        }

        let bits = mantissa | sign_bit;

        if flags.as_binary32 {
            return Some((f32::from_bits(bits as u32) as f64, pos));
        } else {
            return Some((f64::from_bits(bits), pos));
        }
    }

    let mantissa = shift_right_round(mantissa, mantissa_shift, tail);

    if mantissa == 0 {
        return Some((if negative { -0.0 } else { 0.0 }, pos));
    }

    // Assemble final bits
    let mut bits = mantissa;
    bits += ((exponent + enc_max_exponent - 1) as u64) << (enc_mantissa_bits - 1);
    bits |= sign_bit;

    if flags.as_binary32 {
        Some((f32::from_bits(bits as u32) as f64, pos))
    } else {
        Some((f64::from_bits(bits), pos))
    }
}

/// Parse a single-precision float from a string
///
/// This is a convenience wrapper around `parse_double` with the `as_binary32` flag set.
///
/// # Examples
/// ```
/// use ufbx::utils::float_parse::parse_float;
///
/// let (val, len) = parse_float(b"3.14").unwrap();
/// assert_eq!(len, 4);
/// assert!((val - 3.14).abs() < 1e-6);
/// ```
pub fn parse_float(str: &[u8]) -> Option<(f32, usize)> {
    let mut flags = ParseFlags::default();
    flags.as_binary32 = true;
    parse_double(str, flags).map(|(v, len)| (v as f32, len))
}

/// Parse a signed 64-bit integer from a string
///
/// # Examples
/// ```
/// use ufbx::utils::float_parse::parse_i64;
///
/// let (val, len) = parse_i64(b"-12345").unwrap();
/// assert_eq!(val, -12345);
/// assert_eq!(len, 6);
///
/// let (val, len) = parse_i64(b"+999").unwrap();
/// assert_eq!(val, 999);
/// assert_eq!(len, 4);
/// ```
pub fn parse_i64(str: &[u8]) -> Option<(i64, usize)> {
    if str.is_empty() {
        return None;
    }

    let mut pos = 0;
    let negative = str[0] == b'-';
    let positive = str[0] == b'+';

    if negative || positive {
        pos += 1;
        if pos >= str.len() {
            return None;
        }
    }

    let mut abs_val = 0u64;
    let start = pos;

    while pos < str.len() && pos - start < 30 {
        let c = str[pos];
        if c >= b'0' && c <= b'9' {
            abs_val = 10 * abs_val + (c - b'0') as u64;
            pos += 1;
        } else {
            break;
        }
    }

    if pos == start || pos - start == 30 {
        return None;
    }

    let value = if negative {
        (0i64).wrapping_sub(abs_val as i64)
    } else {
        abs_val as i64
    };

    Some((value, pos))
}

/// Parse an unsigned 32-bit integer from a string with the given radix
///
/// Supports radix 10 and 16.
///
/// # Examples
/// ```
/// use ufbx::utils::float_parse::parse_u32_radix;
///
/// let val = parse_u32_radix(b"12345", 10);
/// assert_eq!(val, 12345);
///
/// let val = parse_u32_radix(b"deadbeef", 16);
/// assert_eq!(val, 0xdeadbeef);
///
/// let val = parse_u32_radix(b"CAFE", 16);
/// assert_eq!(val, 0xcafe);
/// ```
pub fn parse_u32_radix(str: &[u8], radix: u32) -> u32 {
    let mut value = 0u32;

    for &c in str {
        if c >= b'0' && c <= b'9' {
            value = value * radix + (c - b'0') as u32;
        } else if radix == 16 && c >= b'a' && c <= b'f' {
            value = value * radix + (c - b'a' + 10) as u32;
        } else if radix == 16 && c >= b'A' && c <= b'F' {
            value = value * radix + (c - b'A' + 10) as u32;
        } else {
            break;
        }
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let (val, len) = parse_double(b"123", ParseFlags::default()).unwrap();
        assert_eq!(val, 123.0);
        assert_eq!(len, 3);
    }

    #[test]
    fn test_parse_decimal() {
        let (val, len) = parse_double(b"3.14159", ParseFlags::default()).unwrap();
        assert_eq!(len, 7);
        assert!((val - 3.14159).abs() < 1e-10);
    }

    #[test]
    fn test_parse_scientific() {
        let (val, _) = parse_double(b"1.23e-4", ParseFlags::default()).unwrap();
        assert!((val - 0.000123).abs() < 1e-10);

        let (val, _) = parse_double(b"5e10", ParseFlags::default()).unwrap();
        assert_eq!(val, 5e10);
    }

    #[test]
    fn test_parse_negative() {
        let (val, _) = parse_double(b"-42.5", ParseFlags::default()).unwrap();
        assert_eq!(val, -42.5);
    }

    #[test]
    fn test_parse_infinity() {
        let (val, _) = parse_double(b"inf", ParseFlags::default()).unwrap();
        assert!(val.is_infinite() && val.is_sign_positive());

        let (val, _) = parse_double(b"-infinity", ParseFlags::default()).unwrap();
        assert!(val.is_infinite() && val.is_sign_negative());

        let (val, _) = parse_double(b"1.#INF", ParseFlags::default()).unwrap();
        assert!(val.is_infinite());
    }

    #[test]
    fn test_parse_nan() {
        let (val, _) = parse_double(b"nan", ParseFlags::default()).unwrap();
        assert!(val.is_nan());

        let (val, _) = parse_double(b"NaN(123)", ParseFlags::default()).unwrap();
        assert!(val.is_nan());

        let (val, _) = parse_double(b"1.#NAN", ParseFlags::default()).unwrap();
        assert!(val.is_nan());
    }

    #[test]
    fn test_parse_zero() {
        let (val, _) = parse_double(b"0", ParseFlags::default()).unwrap();
        assert_eq!(val, 0.0);

        let (val, _) = parse_double(b"-0.0", ParseFlags::default()).unwrap();
        assert_eq!(val, -0.0);
        assert!(val.is_sign_negative());
    }

    #[test]
    fn test_parse_i64() {
        let (val, len) = parse_i64(b"-12345").unwrap();
        assert_eq!(val, -12345);
        assert_eq!(len, 6);

        let (val, _) = parse_i64(b"+999").unwrap();
        assert_eq!(val, 999);

        assert!(parse_i64(b"").is_none());
        assert!(parse_i64(b"-").is_none());
    }

    #[test]
    fn test_parse_u32_radix() {
        assert_eq!(parse_u32_radix(b"12345", 10), 12345);
        assert_eq!(parse_u32_radix(b"ff", 16), 255);
        assert_eq!(parse_u32_radix(b"DEADBEEF", 16), 0xdeadbeef);
    }

    #[test]
    fn test_bigint_mad() {
        let mut bi = BigInt::new(10);
        bi.limbs[0] = 5;
        bi.length = 1;
        bi.mad(10, 3);
        assert_eq!(bi.limbs[0], 53);
        assert_eq!(bi.length, 1);
    }

    #[test]
    fn test_bigint_shift() {
        let mut bi = BigInt::new(10);
        bi.limbs[0] = 1;
        bi.length = 1;
        bi.shift_left(32);
        assert_eq!(bi.limbs[0], 0);
        assert_eq!(bi.limbs[1], 1);
        assert_eq!(bi.length, 2);
    }

    #[test]
    fn test_parse_denormal() {
        // Test a very small denormalized number
        let (val, _) = parse_double(b"2.2250738585072014e-308", ParseFlags::default()).unwrap();
        assert!(val > 0.0);
        assert!(val < 1e-307);
    }

    #[test]
    fn test_parse_max_float() {
        let (val, _) = parse_double(b"1.7976931348623157e308", ParseFlags::default()).unwrap();
        assert!(val.is_finite());
        assert!(val > 1e307);
    }
}
