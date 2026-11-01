use bytes::Bytes;
use questkit_fec::ReedSolomonFec;
use std::collections::HashMap;

struct SimpleRng(u64);

impl SimpleRng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }

    fn should_drop(&mut self, loss_rate: f64) -> bool {
        self.next_u32() < (loss_rate * (u32::MAX as f64)) as u32
    }
}

#[test]
fn recovers_frames_over_a_five_percent_loss_channel() {
    let mut rng = SimpleRng(0xDEAD_BEEF);
    let (frame_count, chunks_per_frame, parity_count) = (30, 10, 2);
    let mut frames_recovered = 0;
    let mut packets_dropped = 0;
    let mut fec_reconstructions = 0;

    for frame_id in 1..=frame_count {
        let source: Vec<Bytes> = (0..chunks_per_frame)
            .map(|chunk| Bytes::from(format!("frame-{frame_id:04}-chunk-{chunk:02}-payload")))
            .collect();
        let parity = ReedSolomonFec::encode(frame_id as u64, &source, parity_count).unwrap();
        let mut received_sources = HashMap::new();
        let mut received_parities = Vec::new();

        for (index, payload) in source.iter().enumerate() {
            if rng.should_drop(0.05) {
                packets_dropped += 1;
            } else {
                received_sources.insert(index, payload.clone());
            }
        }
        for (index, payload) in parity.iter().enumerate() {
            if rng.should_drop(0.05) {
                packets_dropped += 1;
            } else {
                received_parities.push((index, payload.clone()));
            }
        }

        let missing = chunks_per_frame - received_sources.len();
        match ReedSolomonFec::decode(
            frame_id as u64,
            chunks_per_frame,
            parity_count,
            received_sources,
            &received_parities,
        ) {
            Ok(recovered) => {
                assert_eq!(recovered, source);
                frames_recovered += 1;
                fec_reconstructions += missing;
            }
            Err(_) => assert!(missing > received_parities.len()),
        }
    }

    assert!(packets_dropped > 0);
    assert!(fec_reconstructions > 0);
    assert!(
        frames_recovered >= 28,
        "recovered {frames_recovered}/30 frames"
    );
}

#[test]
fn recovery_fails_when_losses_exceed_available_parity() {
    let source = [b"zero".as_slice(), b"one", b"two", b"three"]
        .into_iter()
        .map(Bytes::copy_from_slice)
        .collect::<Vec<_>>();
    let parity = ReedSolomonFec::encode(42, &source, 2).unwrap();
    let received_sources = [(0, source[0].clone())].into_iter().collect();
    let received_parities = [(0, parity[0].clone()), (1, parity[1].clone())];

    assert!(ReedSolomonFec::decode(42, 4, 2, received_sources, &received_parities).is_err());
}
