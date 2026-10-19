// Adapted from Go sort/zsortfunc.go (Copyright 2022 The Go Authors).
// BSD license: ../licenses/GO-LICENSE.txt. Preserve tie order for extraction parity.
#[cfg(test)]
mod tests {
    #[test]
    fn exact_go_tie_order() {
        #[derive(serde::Deserialize)]
        struct Case {
            input: Vec<[i32; 2]>,
            expected: Vec<[i32; 2]>,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/sort-go.json")).unwrap();
        assert_eq!(cases.len(), 45);
        for (index, mut case) in cases.into_iter().enumerate() {
            super::sort(&mut case.input, |a, b| a[0] < b[0]);
            assert_eq!(case.input, case.expected, "sort case {index}");
        }
    }
}
pub fn sort<T>(v: &mut [T], less: impl Fn(&T, &T) -> bool) {
    fn insertion<T>(v: &mut [T], a: usize, b: usize, f: &impl Fn(&T, &T) -> bool) {
        for i in a + 1..b {
            let mut j = i;
            while j > a && f(&v[j], &v[j - 1]) {
                v.swap(j, j - 1);
                j -= 1;
            }
        }
    }
    fn sift<T>(v: &mut [T], mut root: usize, hi: usize, first: usize, f: &impl Fn(&T, &T) -> bool) {
        loop {
            let mut child = 2 * root + 1;
            if child >= hi {
                break;
            }
            if child + 1 < hi && f(&v[first + child], &v[first + child + 1]) {
                child += 1;
            }
            if !f(&v[first + root], &v[first + child]) {
                return;
            }
            v.swap(first + root, first + child);
            root = child;
        }
    }
    fn median<T>(
        v: &[T],
        mut a: usize,
        mut b: usize,
        mut c: usize,
        swaps: &mut usize,
        f: &impl Fn(&T, &T) -> bool,
    ) -> usize {
        if f(&v[b], &v[a]) {
            std::mem::swap(&mut a, &mut b);
            *swaps += 1;
        }
        if f(&v[c], &v[b]) {
            std::mem::swap(&mut b, &mut c);
            *swaps += 1;
        }
        if f(&v[b], &v[a]) {
            std::mem::swap(&mut a, &mut b);
            *swaps += 1;
        }
        b
    }
    fn partial<T>(v: &mut [T], a: usize, b: usize, f: &impl Fn(&T, &T) -> bool) -> bool {
        let mut i = a + 1;
        for _ in 0..5 {
            while i < b && !f(&v[i], &v[i - 1]) {
                i += 1;
            }
            if i == b {
                return true;
            }
            if b - a < 50 {
                return false;
            }
            v.swap(i, i - 1);
            if i - a >= 2 {
                let mut j = i - 1;
                while j >= 1 {
                    if !f(&v[j], &v[j - 1]) {
                        break;
                    }
                    v.swap(j, j - 1);
                    j -= 1;
                }
            }
            if b - i >= 2 {
                for j in i + 1..b {
                    if !f(&v[j], &v[j - 1]) {
                        break;
                    }
                    v.swap(j, j - 1);
                }
            }
        }
        false
    }
    fn pdq<T>(
        v: &mut [T],
        mut a: usize,
        mut b: usize,
        mut limit: u32,
        f: &impl Fn(&T, &T) -> bool,
    ) {
        let (mut balanced, mut partitioned) = (true, true);
        loop {
            let len = b - a;
            if len <= 12 {
                insertion(v, a, b, f);
                return;
            }
            if limit == 0 {
                for i in (0..=(len - 1) / 2).rev() {
                    sift(v, i, len, a, f);
                }
                for i in (0..len).rev() {
                    v.swap(a, a + i);
                    sift(v, 0, i, a, f);
                }
                return;
            }
            if !balanced {
                let mut random = len as u64;
                let modulus = 1usize << (usize::BITS - len.leading_zeros());
                for idx in a + (len / 4) * 2 - 1..=a + (len / 4) * 2 + 1 {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    let mut other = random as usize & (modulus - 1);
                    if other >= len {
                        other -= len;
                    }
                    v.swap(idx, a + other);
                }
                limit -= 1;
            }
            let (mut i, mut j, mut k) = (a + len / 4, a + len / 4 * 2, a + len / 4 * 3);
            let mut swaps = 0;
            if len >= 50 {
                i = median(v, i - 1, i, i + 1, &mut swaps, f);
                j = median(v, j - 1, j, j + 1, &mut swaps, f);
                k = median(v, k - 1, k, k + 1, &mut swaps, f);
            }
            let mut pivot = median(v, i, j, k, &mut swaps, f);
            let mut increasing = swaps == 0;
            if swaps == 12 {
                v[a..b].reverse();
                pivot = (b - 1) - (pivot - a);
                increasing = true;
            }
            if balanced && partitioned && increasing && partial(v, a, b, f) {
                return;
            }
            if a > 0 && !f(&v[a - 1], &v[pivot]) {
                v.swap(a, pivot);
                let (mut i, mut j) = (a + 1, b - 1);
                loop {
                    while i <= j && !f(&v[a], &v[i]) {
                        i += 1;
                    }
                    while i <= j && f(&v[a], &v[j]) {
                        j -= 1;
                    }
                    if i > j {
                        break;
                    }
                    v.swap(i, j);
                    i += 1;
                    j -= 1;
                }
                a = i;
                continue;
            }
            v.swap(a, pivot);
            let (mut i, mut j) = (a + 1, b - 1);
            while i <= j && f(&v[i], &v[a]) {
                i += 1;
            }
            while i <= j && !f(&v[j], &v[a]) {
                j -= 1;
            }
            partitioned = i > j;
            if !partitioned {
                v.swap(i, j);
                i += 1;
                j -= 1;
                loop {
                    while i <= j && f(&v[i], &v[a]) {
                        i += 1;
                    }
                    while i <= j && !f(&v[j], &v[a]) {
                        j -= 1;
                    }
                    if i > j {
                        break;
                    }
                    v.swap(i, j);
                    i += 1;
                    j -= 1;
                }
            }
            v.swap(j, a);
            let mid = j;
            if mid - a < b - mid {
                balanced = mid - a >= len / 8;
                pdq(v, a, mid, limit, f);
                a = mid + 1;
            } else {
                balanced = b - mid >= len / 8;
                pdq(v, mid + 1, b, limit, f);
                b = mid;
            }
        }
    }
    pdq(v, 0, v.len(), usize::BITS - v.len().leading_zeros(), &less)
}
