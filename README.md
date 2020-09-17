# screen_layer

This crate provides layer structure of the screen, which is useful for developing an OS.

This crate uses features of `alloc` crate, so you have to extern `alloc` crate. This means you
have to define your own heap allocator.

Currently this crate only supports 24 or 32 bits color of BGR order.

## Examples

```rust
use screen_layer::{self, Layer, Vec2, RGB8};

const SCREEN_WIDTH: usize = 10;
const SCREEN_HEIGHT: usize = 10;
const BPP: usize = 32;
let mut pseudo_vram = [0u8; SCREEN_WIDTH * SCREEN_HEIGHT * BPP / 8];
let ptr = pseudo_vram.as_ptr() as usize;
let mut controller =
    unsafe { screen_layer::Controller::new(Vec2::new(SCREEN_WIDTH, SCREEN_HEIGHT), BPP, ptr) };

const LAYER_WIDTH: usize = 5;
const LAYER_HEIGHT: usize = 5;
let layer = Layer::new(Vec2::new(0, 0), Vec2::new(LAYER_WIDTH, LAYER_HEIGHT));
let id = controller.add_layer(layer);

controller
    .edit_layer(id, |layer: &mut Layer| {
        for i in 0..LAYER_WIDTH {
            layer[i][i] = Some(RGB8::new(0, 255, 0));
        }
    })
    .unwrap();

for i in 0..LAYER_WIDTH {
    assert_eq!(pseudo_vram[BPP / 8 * (i * SCREEN_WIDTH + i)], 0);
    assert_eq!(pseudo_vram[BPP / 8 * (i * SCREEN_WIDTH + i) + 1], 255);
    assert_eq!(pseudo_vram[BPP / 8 * (i * SCREEN_WIDTH + i) + 2], 0);
}
```

License: MPL-2.0
