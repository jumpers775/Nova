# Migration Plan: Rodio to GStreamer

## Current Implementation Analysis
- Using rodio 0.20 for audio playback
- Using symphonia for metadata extraction and codec support
- Audio playback handled through `LocalAudioBackend` implementing `AudioBackend` trait
- Current seeking implementation has limitations:
  - Requires recreating decoder and sink for each seek operation
  - No native seeking support in rodio
  - Potential audio glitches during seeking

## Migration Steps

### 1. Dependencies
Add required GStreamer dependencies to Cargo.toml:
```toml
[dependencies]
gstreamer = "0.22"
gstreamer-player = "0.22"
gstreamer-audio = "0.22"
```
Remove rodio dependency but keep symphonia for metadata extraction.

### 2. Code Changes

#### Phase 1: File Reorganization
1. Move `LocalAudioBackend` from `src/services/audio_player.rs` to `src/services/local/audio.rs`
2. Update module declarations in `src/services/local/mod.rs`
3. Update imports in `src/services/audio_player.rs`

#### Phase 2: LocalAudioBackend Refactor
Replace rodio implementation with GStreamer while maintaining the same struct:
```rust
pub struct LocalAudioBackend {
    pipeline: Arc<RwLock<Option<gstreamer::Element>>>,
    is_playing: Arc<RwLock<bool>>,
    current_duration: Arc<RwLock<Option<Duration>>>,
    current_path: Arc<RwLock<Option<std::path::PathBuf>>>,
}
```

Key changes:
1. Replace rodio's Sink with GStreamer's playbin element
2. Remove thread_local audio stream handling
3. Use GStreamer's native position tracking
4. Implement proper seeking through GStreamer's seek flags

#### Phase 3: Core Functionality
Update AudioBackend trait implementations:
1. `play()`: 
   - Create playbin pipeline
   - Set URI from local file path
   - Configure audio sink
   - Start playback

2. `stop()`:
   - Set pipeline to NULL state
   - Clean up resources

3. `pause()/resume()`:
   - Use GStreamer PAUSED/PLAYING states
   - Maintain position information

4. `get_position()/set_position()`:
   - Use GStreamer's native position query
   - Implement smooth seeking with proper flags

5. `get_duration()`:
   - Query duration from pipeline
   - Cache for performance

6. `set_volume()`:
   - Control playbin volume property

### 3. Testing Strategy
1. Basic playback functionality
   - Play/pause/stop controls
   - Volume control
   - Track duration reporting
2. Seeking capabilities
   - Accurate position reporting
   - Smooth seeking without audio glitches
   - Seeking while paused
3. Error handling
   - Invalid file handling
   - Pipeline errors
   - Resource availability

### 4. Cleanup
1. Remove rodio dependency from Cargo.toml
2. Clean up any unused imports
3. Update documentation
4. Remove any remaining rodio-specific code

## Benefits
1. Native seeking support through GStreamer's robust media handling
2. Better playback control and state management
3. Simpler position tracking using native GStreamer capabilities
4. Maintain robust metadata extraction through symphonia

## Potential Challenges
1. GStreamer installation requirements on different platforms
2. Error handling adaptation for GStreamer
3. State management during pipeline transitions

## Timeline
1. Phase 1: 1 day
2. Phase 2: 2-3 days
3. Phase 3: 1-2 days
4. Testing and cleanup: 1-2 days

Total estimated time: 5-8 days

## Next Steps
1. Set up development environment with GStreamer dependencies
2. Create new branch for audio backend migration
3. Move LocalAudioBackend to local/ folder
4. Begin GStreamer implementation while maintaining symphonia for metadata extraction