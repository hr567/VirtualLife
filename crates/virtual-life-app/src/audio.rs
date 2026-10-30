//! Position PCM at decoder creation; never synchronously wait on the WASM audio thread.
use std::{sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug)]
pub struct WaveInfo {
    pub start: usize,
    pub length: usize,
    pub byte_rate: u32,
    pub alignment: u16,
}
impl WaveInfo {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
            return None;
        }
        let mut cursor = 12usize;
        let mut rate = 0;
        let mut alignment = 0;
        while cursor + 8 <= bytes.len() {
            let length =
                u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().ok()?) as usize;
            let end = cursor.checked_add(8 + length)?;
            if end > bytes.len() {
                return None;
            }
            match &bytes[cursor..cursor + 4] {
                b"fmt " if length >= 16 => {
                    if u16::from_le_bytes(bytes[cursor + 8..cursor + 10].try_into().ok()?) != 1 {
                        return None;
                    }
                    rate = u32::from_le_bytes(bytes[cursor + 16..cursor + 20].try_into().ok()?);
                    alignment =
                        u16::from_le_bytes(bytes[cursor + 20..cursor + 22].try_into().ok()?);
                }
                b"data" if rate > 0 && alignment > 0 && length > 0 => {
                    return Some(Self {
                        start: cursor + 8,
                        length,
                        byte_rate: rate,
                        alignment,
                    });
                }
                _ => {}
            }
            cursor = end.checked_add(length & 1)?;
        }
        None
    }
    pub fn duration(self) -> Duration {
        Duration::from_secs_f64(self.length as f64 / f64::from(self.byte_rate))
    }
    pub fn position(self, milliseconds: u64, looped: bool) -> Option<Duration> {
        let sample = (u128::from(milliseconds) * u128::from(self.byte_rate) / 1000)
            / u128::from(self.alignment)
            * u128::from(self.alignment);
        if !looped && sample >= self.length as u128 {
            return None;
        }
        Some(Duration::from_secs_f64(
            (sample % self.length as u128) as f64 / f64::from(self.byte_rate),
        ))
    }
}

/// Rotating a loop preserves its full period. Skipping then repeating would
/// incorrectly repeat only the tail after a saved playback position.
pub fn positioned_wave(
    bytes: Arc<[u8]>,
    info: WaveInfo,
    position: Duration,
    looped: bool,
) -> Arc<[u8]> {
    let skip = ((position.as_secs_f64() * f64::from(info.byte_rate) / f64::from(info.alignment))
        .round() as usize
        * usize::from(info.alignment))
    .min(info.length);
    if skip == 0 {
        return bytes;
    }
    let data = &bytes[info.start..info.start + info.length];
    let mut output = bytes[..info.start].to_vec();
    output.extend_from_slice(&data[skip..]);
    if looped {
        output.extend_from_slice(&data[..skip]);
    }
    let length = output.len() - info.start;
    output[info.start - 4..info.start].copy_from_slice(&(length as u32).to_le_bytes());
    if length & 1 != 0 {
        output.push(0)
    }
    let riff_size = (output.len() - 8) as u32;
    output[4..8].copy_from_slice(&riff_size.to_le_bytes());
    output.into()
}

#[cfg(any(feature = "web-client", test))]
pub(crate) mod synchronization {
    use super::*;
    use std::collections::HashMap;

    pub struct CachedWave<T> {
        pub bytes: Arc<[u8]>,
        pub info: WaveInfo,
        pub duration: Duration,
        /// Only the unmodified source is shared. Positioned tails/rotated loops
        /// belong to their playback entity and are released when it despawns.
        pub source: Option<T>,
    }
    struct CacheEntry<T> {
        revision: Option<u64>,
        bytes: Option<Arc<[u8]>>,
        wave: Option<CachedWave<T>>,
    }
    pub struct WaveCache<T> {
        entries: HashMap<String, CacheEntry<T>>,
    }
    impl<T> Default for WaveCache<T> {
        fn default() -> Self {
            Self {
                entries: HashMap::new(),
            }
        }
    }
    impl<T> WaveCache<T> {
        /// Bank revisions may also come from images/video. Recheck the raw Arc
        /// only on a revision, and reparse only if that resource actually changed.
        /// Missing/unsupported sources are cached until the next bank revision.
        pub fn resolve(
            &mut self,
            id: &str,
            revision: u64,
            load: impl FnOnce() -> Option<Arc<[u8]>>,
        ) -> Option<&mut CachedWave<T>> {
            if !self.entries.contains_key(id) {
                self.entries.insert(
                    id.into(),
                    CacheEntry {
                        revision: None,
                        bytes: None,
                        wave: None,
                    },
                );
            }
            let entry = self.entries.get_mut(id)?;
            if entry.revision != Some(revision) {
                let bytes = load();
                let unchanged = match (&entry.bytes, &bytes) {
                    (Some(old), Some(new)) => Arc::ptr_eq(old, new),
                    (None, None) => true,
                    _ => false,
                };
                if !unchanged {
                    entry.wave = bytes.as_ref().and_then(|bytes| {
                        let info = WaveInfo::parse(bytes)?;
                        Some(CachedWave {
                            bytes: bytes.clone(),
                            info,
                            duration: info.duration(),
                            source: None,
                        })
                    });
                    entry.bytes = bytes;
                }
                entry.revision = Some(revision);
            }
            entry.wave.as_mut()
        }
        pub fn retain(&mut self, mut used: impl FnMut(&str) -> bool) {
            self.entries.retain(|id, _| used(id));
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PlaybackState {
        pub volume: u8,
        pub paused: bool,
    }
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct PlaybackChanges {
        pub volume: Option<u8>,
        pub paused: Option<bool>,
    }
    impl PlaybackState {
        pub fn changes_from(self, previous: Option<Self>) -> PlaybackChanges {
            PlaybackChanges {
                volume: previous
                    .is_none_or(|old| old.volume != self.volume)
                    .then_some(self.volume),
                paused: previous
                    .is_none_or(|old| old.paused != self.paused)
                    .then_some(self.paused),
            }
        }
    }

    pub fn out_of_sync(
        origin: Duration,
        elapsed: Duration,
        duration: Duration,
        expected: Option<Duration>,
        looped: bool,
    ) -> bool {
        let Some(expected) = expected else {
            return true;
        };
        let actual = origin + elapsed;
        let actual = if looped {
            Duration::from_secs_f64(actual.as_secs_f64() % duration.as_secs_f64())
        } else {
            actual
        };
        let mut drift = actual.abs_diff(expected);
        if looped {
            drift = drift.min(duration.saturating_sub(drift));
        }
        drift > Duration::from_millis(250)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synchronization::*;
    fn wave() -> Arc<[u8]> {
        let mut b=b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x04\0\0\0\x04\0\0\0\x01\0\x08\0data\x04\0\0\0".to_vec();
        b.extend_from_slice(&[10, 20, 30, 40]);
        b.into()
    }
    #[test]
    fn restored_loop_preserves_samples_and_the_entire_original_period() {
        let b = wave();
        let info = WaveInfo::parse(&b).unwrap();
        let p = info.position(500, true).unwrap();
        let rotated = positioned_wave(b, info, p, true);
        assert_eq!(&rotated[44..], &[30, 40, 10, 20]);
        assert_eq!(
            WaveInfo::parse(&rotated).unwrap().duration(),
            Duration::from_secs(1)
        );
    }
    #[test]
    fn one_shot_resume_drops_only_played_samples_and_rejects_finished_position() {
        let b = wave();
        let info = WaveInfo::parse(&b).unwrap();
        let tail = positioned_wave(b, info, info.position(500, false).unwrap(), false);
        assert_eq!(&tail[44..], &[30, 40]);
        assert_eq!(
            u32::from_le_bytes(tail[4..8].try_into().unwrap()) as usize,
            tail.len() - 8
        );
        assert!(info.position(1000, false).is_none());
        assert_eq!(
            info.position(1500, true).unwrap(),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn wave_cache_reuses_verified_bytes_and_sources_until_the_resource_changes() {
        let bytes = wave();
        let mut cache = WaveCache::<usize>::default();
        let first = cache.resolve("loop", 0, || Some(bytes.clone())).unwrap();
        first.source = Some(7);
        assert_eq!(first.duration, Duration::from_secs(1));
        assert!(Arc::ptr_eq(&first.bytes, &bytes));
        assert_eq!(first.info.length, 4);
        let same = cache.resolve("loop", 0, || panic!("unchanged bank must not be read"));
        assert_eq!(same.unwrap().source, Some(7));
        let image_revision = cache.resolve("loop", 1, || Some(bytes.clone())).unwrap();
        assert_eq!(image_revision.source, Some(7));

        let mut replacement = bytes.to_vec();
        replacement[28..32].copy_from_slice(&8u32.to_le_bytes());
        let replacement = cache
            .resolve("loop", 2, || Some(replacement.into()))
            .unwrap();
        assert_eq!(replacement.duration, Duration::from_millis(500));
        assert!(replacement.source.is_none());
        assert!(
            cache
                .resolve("loop", 3, || Some(b"invalid".to_vec().into()))
                .is_none()
        );
        assert!(
            cache
                .resolve("loop", 3, || panic!("unsupported source must stay cached"))
                .is_none()
        );
        assert!(cache.resolve("loop", 4, || None).is_none());
        assert!(
            cache
                .resolve("loop", 4, || panic!("missing source must stay cached"))
                .is_none()
        );
        assert!(cache.resolve("loop", 5, || Some(bytes)).is_some());
    }

    #[test]
    fn wave_cache_releases_unused_source_handles_and_raw_pcm() {
        let bytes = wave();
        let source = Arc::new(());
        let mut cache = WaveCache::<Arc<()>>::default();
        cache
            .resolve("tail", 0, || Some(bytes.clone()))
            .unwrap()
            .source = Some(source.clone());
        cache.retain(|id| id == "tail");
        assert_eq!(Arc::strong_count(&source), 2);
        cache.retain(|_| false);
        assert_eq!(Arc::strong_count(&source), 1);
        assert_eq!(Arc::strong_count(&bytes), 1);
    }

    #[test]
    fn playback_settings_write_only_initialization_and_real_state_changes() {
        let playing = PlaybackState {
            volume: 50,
            paused: false,
        };
        assert_eq!(
            playing.changes_from(None),
            PlaybackChanges {
                volume: Some(50),
                paused: Some(false)
            }
        );
        assert_eq!(
            playing.changes_from(Some(playing)),
            PlaybackChanges::default()
        );
        let quieter = PlaybackState {
            volume: 25,
            ..playing
        };
        assert_eq!(
            quieter.changes_from(Some(playing)),
            PlaybackChanges {
                volume: Some(25),
                paused: None
            }
        );
        let paused = PlaybackState {
            paused: true,
            ..quieter
        };
        assert_eq!(
            paused.changes_from(Some(quieter)),
            PlaybackChanges {
                volume: None,
                paused: Some(true)
            }
        );
        assert_eq!(
            quieter.changes_from(Some(paused)),
            PlaybackChanges {
                volume: None,
                paused: Some(false)
            }
        );
    }

    #[test]
    fn synchronization_keeps_the_strict_threshold_loop_boundary_and_finished_tail() {
        let ms = Duration::from_millis;
        assert!(!out_of_sync(
            ms(500),
            ms(250),
            ms(1000),
            Some(ms(500)),
            false
        ));
        assert!(out_of_sync(
            ms(500),
            ms(251),
            ms(1000),
            Some(ms(500)),
            false
        ));
        assert!(!out_of_sync(
            ms(900),
            ms(150),
            ms(1000),
            Some(ms(990)),
            true
        ));
        assert!(out_of_sync(ms(900), ms(150), ms(1000), Some(ms(500)), true));
        assert!(!out_of_sync(ms(0), ms(100), ms(200), Some(ms(0)), true));
        assert!(out_of_sync(ms(500), ms(500), ms(1000), None, false));
    }
}
