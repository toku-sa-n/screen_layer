# screen_layer

This crate provides layer structure of the screen, which is useful for developing an OS.

This crate uses features of `alloc` crate, so you have to extern `alloc` crate. This means you
have to define your own heap allocator.

Currently this crate only supports 24 or 32 bits color of BGR order.

## Examples

```rust
use screen_layer::{self, Layer, Vec2, RGB8};

const SCREEN_WIDTH: u32 = 10;
const SCREEN_HEIGHT: u32 = 10;
const BPP: u32 = 32;

let mut pseudo_vram = [0u8; (SCREEN_WIDTH * SCREEN_HEIGHT * BPP / 8) as usize];
let ptr = pseudo_vram.as_ptr() as usize;
let mut controller =
    unsafe { screen_layer::Controller::new(Vec2::new(SCREEN_WIDTH, SCREEN_HEIGHT), BPP, ptr) };

const LAYER_WIDTH: u32 = 5;
const LAYER_HEIGHT: u32 = 5;
let layer = Layer::new(Vec2::new(0, 0), Vec2::new(LAYER_WIDTH, LAYER_HEIGHT));
let id = controller.add_layer(layer);

controller
    .edit_layer(id, |layer: &mut Layer| {
        for i in 0..LAYER_WIDTH {
            layer[i as usize][i as usize] = Some(RGB8::new(0, 255, 0));
        }
    })
    .unwrap();

for i in 0..LAYER_WIDTH {
    assert_eq!(pseudo_vram[(BPP / 8 * (i * SCREEN_WIDTH + i)) as usize], 0);
    assert_eq!(pseudo_vram[(BPP / 8 * (i * SCREEN_WIDTH + i) + 1) as usize], 255);
    assert_eq!(pseudo_vram[(BPP / 8 * (i * SCREEN_WIDTH + i) + 2) as usize], 0);
}

controller.set_pixel(id, Vec2::one(), Some(RGB8::new(255, 0, 0)));
assert_eq!(pseudo_vram[(BPP / 8 * (1 * SCREEN_WIDTH + 1)) as usize], 0);
assert_eq!(pseudo_vram[(BPP / 8 * (1 * SCREEN_WIDTH + 1) + 1) as usize], 0);
assert_eq!(pseudo_vram[(BPP / 8 * (1 * SCREEN_WIDTH + 1) + 2) as usize], 255);
```

License: MPL-2.0
