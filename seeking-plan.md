# Plan for Implementing Seeking with rodio's try_seek

## Current Understanding
After reviewing rodio's documentation:
1. Sink doesn't provide direct access to the underlying Source
2. We need a different approach to access the Source for seeking

## Revised Implementation Plan

### 1. Store Source During Playback
```rust
pub struct LocalAudioBackend {
    sink: Arc<RwLock<Option<Sink>>>,
    // Other fields...
    current_source: Arc<RwLock<Option<Box<dyn Source<Item = f32> + Send + Sync>>>>,
}
```

### 2. Modify Play Method
```rust
fn play(&self, track: &Track) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create decoder
    let decoder = rodio::Decoder::new(reader)?;
    
    // Store source before appending to sink
    let source = decoder.convert_samples();
    *self.current_source.write() = Some(Box::new(source.clone()));
    
    // Create sink and append source
    sink.append(source);
}
```

### 3. Implement Seeking
```rust
fn set_position(&self, position: Duration) {
    // Try seeking on current source first
    if let Some(source) = self.current_source.write().as_mut() {
        if source.try_seek(position).is_ok() {
            // Update UI state
            *self.elapsed_time.write() = position;
            *self.start_time.write() = Some(Instant::now());
            return;
        }
    }
    
    // Fall back to recreating decoder if seek fails
    // (existing fallback code)
}
```

### Key Points
1. Source must be both Send + Sync for thread safety
2. Store source before appending to sink
3. Try seeking on stored source first
4. Fall back to recreation only if seeking fails

### Next Steps
1. Implement this approach
2. Test seeking behavior
3. Verify thread safety
4. Measure performance impact

### Expected Benefits
1. Fast seeking using try_seek when possible
2. Proper thread safety
3. Clean fallback behavior
4. Improved performance over current implementation