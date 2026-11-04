use bytes::{Bytes, BytesMut};
use std::collections::HashMap;

use super::{FecError, FecHeader, FEC_HEADER_SIZE};

/// Galois Field GF(2^8) with polynomial 0x11d (x^8 + x^4 + x^3 + x^2 + 1).
struct GaloisField {
    exp: [u8; 512],
    log: [u8; 256],
}

impl GaloisField {
    const fn init() -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x = 1u16;
        let mut i = 0;
        while i < 255 {
            exp[i] = x as u8;
            exp[i + 255] = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if (x & 0x100) != 0 {
                x ^= 0x11d;
            }
            i += 1;
        }
        exp[510] = exp[0];
        exp[511] = exp[1];
        GaloisField { exp, log }
    }

    /// Reference log/exp product; the hot paths use [`MUL`] instead.
    #[cfg(test)]
    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            let log_a = self.log[a as usize] as usize;
            let log_b = self.log[b as usize] as usize;
            self.exp[log_a + log_b]
        }
    }

    #[inline(always)]
    fn inv(&self, a: u8) -> Result<u8, FecError> {
        if a == 0 {
            return Err(FecError::SingularMatrix);
        }
        let log_a = self.log[a as usize] as usize;
        Ok(self.exp[255 - log_a])
    }
}

static GF: GaloisField = GaloisField::init();

/// Full GF(2^8) product table: `MUL[a][b] == GF.mul(a, b)`.
///
/// 64 KiB, built at compile time. Row `MUL[c]` turns "multiply every byte by
/// `c`" into one branch-free lookup per byte, which is the inner loop of both
/// encode and decode.
static MUL: [[u8; 256]; 256] = {
    let gf = GaloisField::init();
    let mut table = [[0u8; 256]; 256];
    let mut a = 1;
    while a < 256 {
        let log_a = gf.log[a] as usize;
        let mut b = 1;
        while b < 256 {
            table[a][b] = gf.exp[log_a + gf.log[b] as usize];
            b += 1;
        }
        a += 1;
    }
    table
};

/// `dst[i] ^= coeff * src[i]` for every `i < src.len()`.
#[inline]
fn mul_acc(dst: &mut [u8], src: &[u8], coeff: u8) {
    let dst = &mut dst[..src.len()];
    match coeff {
        0 => {}
        1 => {
            for (d, s) in dst.iter_mut().zip(src) {
                *d ^= *s;
            }
        }
        _ => {
            let row = &MUL[coeff as usize];
            for (d, s) in dst.iter_mut().zip(src) {
                *d ^= row[*s as usize];
            }
        }
    }
}

/// `buf[i] = coeff * buf[i]` in place.
#[inline]
fn mul_in_place(buf: &mut [u8], coeff: u8) {
    if coeff == 1 {
        return;
    }
    let row = &MUL[coeff as usize];
    for b in buf.iter_mut() {
        *b = row[*b as usize];
    }
}

/// Accumulates `coeff * symbol(packet)` into `dst`, where `symbol(packet)` is
/// `[len: u16 BE][payload][zero padding]`. Padding contributes nothing, so the
/// padded symbol is never materialised.
#[inline]
fn mul_acc_packet(dst: &mut [u8], packet: &[u8], coeff: u8) {
    let len = (packet.len() as u16).to_be_bytes();
    mul_acc(&mut dst[..2], &len, coeff);
    mul_acc(&mut dst[2..], packet, coeff);
}

/// Systematic Cauchy Reed-Solomon Erasure Codec over Galois Field GF(2^8).
///
/// The codec supports up to 256 total source and parity packets per block and
/// returns parity packets with a DeskQuest-compatible [`FecHeader`](super::FecHeader).
pub struct ReedSolomonFec;

#[allow(clippy::needless_range_loop)]
impl ReedSolomonFec {
    /// Cauchy matrix entry: 1 / (X_i ^ Y_j).
    /// Y_j = j for j in 0..k
    /// X_i = k + i for i in 0..m
    #[inline(always)]
    fn cauchy_coeff(
        k: usize,
        m: usize,
        parity_idx: usize,
        source_idx: usize,
    ) -> Result<u8, FecError> {
        if source_idx >= k || parity_idx >= m {
            return Err(FecError::CorruptHeader);
        }
        let x = (k + parity_idx) as u8;
        let y = source_idx as u8;
        let diff = x ^ y;
        GF.inv(diff)
    }

    /// Encodes source packet payloads into `parity_count` framed parity packets.
    ///
    /// Each returned `Bytes` contains a full FEC packet with [`FecHeader`](super::FecHeader)
    /// prepended.
    pub fn encode(
        block_id: u64,
        source_packets: &[Bytes],
        parity_count: usize,
    ) -> Result<Vec<Bytes>, FecError> {
        let k = source_packets.len();
        let m = parity_count;

        if k == 0 || m == 0 || k + m > 256 {
            return Err(FecError::InvalidConfiguration { k, m });
        }

        let max_len = source_packets.iter().map(|p| p.len()).max().unwrap_or(0);
        if max_len + 2 > u16::MAX as usize {
            return Err(FecError::InvalidConfiguration { k, m });
        }
        let symbol_size = max_len + 2;

        let mut parity_packets = Vec::with_capacity(m);

        for p_idx in 0..m {
            let mut parity_symbol = vec![0u8; symbol_size];

            for (s_idx, packet) in source_packets.iter().enumerate() {
                let coeff = Self::cauchy_coeff(k, m, p_idx, s_idx)?;
                mul_acc_packet(&mut parity_symbol, packet, coeff);
            }

            let header = FecHeader::new(
                super::FecScheme::ReedSolomon,
                block_id,
                k as u16,
                m as u16,
                p_idx as u16,
                symbol_size as u16,
            );

            let mut buf = BytesMut::with_capacity(FEC_HEADER_SIZE + symbol_size);
            header.encode(&mut buf);
            buf.extend_from_slice(&parity_symbol);
            parity_packets.push(buf.freeze());
        }

        Ok(parity_packets)
    }

    /// Decodes and reconstructs any missing source packets.
    /// `received_sources`: map of source_index (0..k-1) -> source payload.
    /// `received_parities`: slice of (parity_index, raw parity packet with FecHeader).
    /// Returns all K source packets in sequential order if recoverable.
    /// Reconstructs missing payloads and returns the complete block in index order.
    pub fn decode(
        block_id: u64,
        k: usize,
        m: usize,
        mut received_sources: HashMap<usize, Bytes>,
        received_parities: &[(usize, Bytes)],
    ) -> Result<Vec<Bytes>, FecError> {
        if k == 0 || m == 0 || k + m > 256 {
            return Err(FecError::InvalidConfiguration { k, m });
        }

        // Discard any extraneous indices >= k
        received_sources.retain(|&idx, _| idx < k);

        // Fast path: all K source packets already received
        if received_sources.len() == k {
            let mut result = Vec::with_capacity(k);
            for i in 0..k {
                result.push(
                    received_sources
                        .remove(&i)
                        .ok_or(FecError::InsufficientPackets {
                            needed: k,
                            received: k - 1,
                        })?,
                );
            }
            return Ok(result);
        }

        // Find missing source indices
        let missing_sources: Vec<usize> = (0..k)
            .filter(|i| !received_sources.contains_key(i))
            .collect();

        let num_missing = missing_sources.len();

        if received_parities.len() < num_missing {
            return Err(FecError::InsufficientPackets {
                needed: k,
                received: received_sources.len() + received_parities.len(),
            });
        }

        // Parse parity headers and symbols
        let mut parsed_parities: Vec<(usize, Vec<u8>)> = Vec::with_capacity(num_missing);
        let mut symbol_size = 0;

        for (p_idx, raw_pkt) in received_parities.iter().take(num_missing) {
            if *p_idx >= m {
                return Err(FecError::CorruptHeader);
            }
            let mut slice = &raw_pkt[..];
            let header = FecHeader::decode(&mut slice)?;

            if header.block_id != block_id
                || header.source_count as usize != k
                || header.parity_count as usize != m
                || header.parity_index as usize != *p_idx
                || header.scheme != super::FecScheme::ReedSolomon
            {
                return Err(FecError::CorruptHeader);
            }

            let p_symbol_size = header.symbol_size as usize;
            if p_symbol_size < 2 || slice.len() < p_symbol_size {
                return Err(FecError::CorruptHeader);
            }

            if symbol_size == 0 {
                symbol_size = p_symbol_size;
            } else if symbol_size != p_symbol_size {
                return Err(FecError::CorruptHeader);
            }

            parsed_parities.push((*p_idx, slice[..symbol_size].to_vec()));
        }

        for (&idx, packet) in &received_sources {
            if idx >= k || packet.len() + 2 > symbol_size {
                return Err(FecError::CorruptHeader);
            }
        }

        // Construct linear system A * X = R
        // where A is num_missing x num_missing
        // R is num_missing x symbol_size
        let mut a: Vec<Vec<u8>> = vec![vec![0u8; num_missing]; num_missing];
        let mut r: Vec<Vec<u8>> = Vec::with_capacity(num_missing);

        for (row, (p_idx, p_symbol)) in parsed_parities.into_iter().enumerate() {
            // R[row] = P[p_idx] ^ sum_{s in known} (C[p_idx, s] * S[s])
            let mut acc = p_symbol;
            for (&s_idx, packet) in &received_sources {
                let coeff = Self::cauchy_coeff(k, m, p_idx, s_idx)?;
                mul_acc_packet(&mut acc, packet, coeff);
            }
            r.push(acc);

            // Fill row of matrix A: A[row][col] = C[p_idx, missing_sources[col]]
            for col in 0..num_missing {
                let s_missing_idx = missing_sources[col];
                a[row][col] = Self::cauchy_coeff(k, m, p_idx, s_missing_idx)?;
            }
        }

        // Gauss-Jordan elimination to solve for missing symbols
        for i in 0..num_missing {
            // Pivot selection
            let mut pivot_row = i;
            while pivot_row < num_missing && a[pivot_row][i] == 0 {
                pivot_row += 1;
            }

            if pivot_row == num_missing {
                return Err(FecError::SingularMatrix);
            }

            if pivot_row != i {
                a.swap(i, pivot_row);
                r.swap(i, pivot_row);
            }

            let pivot_inv = GF.inv(a[i][i])?;
            mul_in_place(&mut a[i], pivot_inv);
            mul_in_place(&mut r[i], pivot_inv);

            // Eliminate all other rows. Rows `i` are moved out so the pivot row
            // can be read while the others are written.
            let pivot_a = std::mem::take(&mut a[i]);
            let pivot_r = std::mem::take(&mut r[i]);
            for other in 0..num_missing {
                if other != i {
                    let factor = a[other][i];
                    if factor != 0 {
                        mul_acc(&mut a[other], &pivot_a, factor);
                        mul_acc(&mut r[other], &pivot_r, factor);
                    }
                }
            }
            a[i] = pivot_a;
            r[i] = pivot_r;
        }

        // Recovered symbols are in r[col]
        for col in 0..num_missing {
            let missing_idx = missing_sources[col];
            let sym = &r[col];
            let len = u16::from_be_bytes([sym[0], sym[1]]) as usize;
            if 2 + len > symbol_size {
                return Err(FecError::CorruptHeader);
            }
            let payload = Bytes::copy_from_slice(&sym[2..2 + len]);
            received_sources.insert(missing_idx, payload);
        }

        let mut result = Vec::with_capacity(k);
        for i in 0..k {
            result.push(
                received_sources
                    .remove(&i)
                    .ok_or(FecError::InsufficientPackets {
                        needed: k,
                        received: k - 1,
                    })?,
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
mod tests {
    use super::*;

    #[test]
    fn test_galois_field_arithmetic() {
        assert_eq!(GF.mul(0, 42), 0);
        assert_eq!(GF.mul(42, 0), 0);
        assert_eq!(GF.mul(1, 42), 42);
        assert_eq!(GF.mul(42, 1), 42);

        for a in 1..=255u8 {
            let inv = GF.inv(a).expect("inversion should succeed for non-zero");
            assert_eq!(GF.mul(a, inv), 1, "Failed for a={a}");
        }
        assert!(matches!(GF.inv(0), Err(FecError::SingularMatrix)));
    }

    #[test]
    fn test_mul_table_matches_log_exp_product() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                assert_eq!(MUL[a as usize][b as usize], GF.mul(a, b), "a={a} b={b}");
            }
        }
    }

    /// Every loss pattern of up to `m` packets (sources and parities) across
    /// several block shapes and uneven payload lengths must round-trip.
    #[test]
    fn test_reed_solomon_recovers_every_small_loss_pattern() {
        for &(k, m) in &[(1usize, 1usize), (3, 2), (5, 3), (7, 4)] {
            let sources: Vec<Bytes> = (0..k)
                .map(|i| {
                    Bytes::from(
                        (0..(17 + i * 9))
                            .map(|b| (b * 31 + i * 7) as u8)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();
            let parities = ReedSolomonFec::encode(9, &sources, m).unwrap();
            let n = k + m;
            for lost in 0u32..(1 << n) {
                if lost.count_ones() as usize > m {
                    continue;
                }
                let received: HashMap<usize, Bytes> = (0..k)
                    .filter(|i| lost & (1 << i) == 0)
                    .map(|i| (i, sources[i].clone()))
                    .collect();
                let received_parities: Vec<(usize, Bytes)> = (0..m)
                    .filter(|j| lost & (1 << (k + j)) == 0)
                    .map(|j| (j, parities[j].clone()))
                    .collect();
                let recovered =
                    ReedSolomonFec::decode(9, k, m, received, &received_parities).unwrap();
                assert_eq!(recovered, sources, "k={k} m={m} lost={lost:#b}");
            }
        }
    }

    #[test]
    fn test_reed_solomon_single_packet_loss() {
        let sources = vec![
            Bytes::from_static(b"packet-zero"),
            Bytes::from_static(b"packet-one-intermediate-size"),
            Bytes::from_static(b"packet-two"),
            Bytes::from_static(b"packet-three"),
        ];

        let parities = ReedSolomonFec::encode(100, &sources, 2).unwrap();
        assert_eq!(parities.len(), 2);

        // Drop index 1
        let mut received = HashMap::new();
        received.insert(0, sources[0].clone());
        received.insert(2, sources[2].clone());
        received.insert(3, sources[3].clone());

        // Use parity 0
        let received_parities = vec![(0, parities[0].clone())];

        let recovered = ReedSolomonFec::decode(100, 4, 2, received, &received_parities).unwrap();
        assert_eq!(recovered, sources);
    }

    #[test]
    fn test_reed_solomon_multi_packet_loss() {
        let k = 8;
        let m = 4;
        let mut sources = Vec::new();
        for i in 0..k {
            sources.push(Bytes::from(format!(
                "video-chunk-data-stream-slice-{:04}",
                i
            )));
        }

        let parities = ReedSolomonFec::encode(200, &sources, m).unwrap();
        assert_eq!(parities.len(), m);

        // Simulate dropping 3 packets: index 1, 4, 6
        let mut received = HashMap::new();
        for i in 0..k {
            if i != 1 && i != 4 && i != 6 {
                received.insert(i, sources[i].clone());
            }
        }
        assert_eq!(received.len(), 5); // 3 missing

        // Provide 3 parities: parities 0, 2, 3 (parity 1 also dropped in transit!)
        let received_parities = vec![
            (0, parities[0].clone()),
            (2, parities[2].clone()),
            (3, parities[3].clone()),
        ];

        let recovered = ReedSolomonFec::decode(200, k, m, received, &received_parities).unwrap();
        assert_eq!(recovered, sources);
    }

    #[test]
    fn test_reed_solomon_max_loss_recovery() {
        let k = 6;
        let m = 3;
        let sources: Vec<Bytes> = (0..k)
            .map(|i| Bytes::from(vec![i as u8 * 17; 500]))
            .collect();

        let parities = ReedSolomonFec::encode(300, &sources, m).unwrap();

        // Drop all 3 allowed packets (indices 0, 2, 5)
        let mut received = HashMap::new();
        received.insert(1, sources[1].clone());
        received.insert(3, sources[3].clone());
        received.insert(4, sources[4].clone());

        // Provide all 3 parities
        let received_parities = vec![
            (0, parities[0].clone()),
            (1, parities[1].clone()),
            (2, parities[2].clone()),
        ];

        let recovered = ReedSolomonFec::decode(300, k, m, received, &received_parities).unwrap();
        assert_eq!(recovered, sources);
    }
}
