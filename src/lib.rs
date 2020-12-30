//  This Source Code Form is subject to the terms of the Mozilla Public
//  License, v. 2.0. If a copy of the MPL was not distributed with this
//  file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This crate provides layer structure of the screen, which is useful for developing an OS.
//!
//! This crate uses features of `alloc` crate, so you have to extern `alloc` crate. This means you
//! have to define your own heap allocator.
//!
//! Currently this crate only supports 24 or 32 bits color of BGR order.
//!
//! # Examples
//!
//! ```rust
//! use screen_layer::{self, Layer, Vec2, RGB8};
//!
//! const SCREEN_WIDTH: u32 = 10;
//! const SCREEN_HEIGHT: u32 = 10;
//! const BPP: u32 = 32;
//!
//! let mut pseudo_vram = [0u8; (SCREEN_WIDTH * SCREEN_HEIGHT * BPP / 8) as usize];
//! let ptr = pseudo_vram.as_ptr() as usize;
//! let mut controller =
//!     unsafe { screen_layer::Controller::new(Vec2::new(SCREEN_WIDTH, SCREEN_HEIGHT), BPP, ptr) };
//!
//! const LAYER_WIDTH: u32 = 5;
//! const LAYER_HEIGHT: u32 = 5;
//! let layer = Layer::new(Vec2::new(0, 0), Vec2::new(LAYER_WIDTH, LAYER_HEIGHT));
//! let id = controller.add_layer(layer);
//!
//! controller
//!     .edit_layer(id, |layer: &mut Layer| {
//!         for i in 0..LAYER_WIDTH {
//!             layer[i as usize][i as usize] = Some(RGB8::new(0, 255, 0));
//!         }
//!     })
//!     .unwrap();
//!
//! for i in 0..LAYER_WIDTH {
//!     assert_eq!(pseudo_vram[(BPP / 8 * (i * SCREEN_WIDTH + i)) as usize], 0);
//!     assert_eq!(pseudo_vram[(BPP / 8 * (i * SCREEN_WIDTH + i) + 1) as usize], 255);
//!     assert_eq!(pseudo_vram[(BPP / 8 * (i * SCREEN_WIDTH + i) + 2) as usize], 0);
//! }
//!
//! controller.set_pixel(id, Vec2::one(), Some(RGB8::new(255, 0, 0)));
//! assert_eq!(pseudo_vram[(BPP / 8 * (1 * SCREEN_WIDTH + 1)) as usize], 0);
//! assert_eq!(pseudo_vram[(BPP / 8 * (1 * SCREEN_WIDTH + 1) + 1) as usize], 0);
//! assert_eq!(pseudo_vram[(BPP / 8 * (1 * SCREEN_WIDTH + 1) + 2) as usize], 255);
//! ```

#![no_std]

#[deny(clippy::all)]
#[macro_use]
extern crate alloc;

use {
    alloc::vec::Vec,
    core::{
        convert::{TryFrom, TryInto},
        mem::size_of,
        ops::{Index, IndexMut},
        ptr,
        sync::atomic::{AtomicU64, Ordering::Relaxed},
    },
};

/// This type is used to represent color of each pixels.
pub use rgb::RGB8;

/// This type is used to represent the coordinate, and width and height of a layer.
pub use vek::Vec2;

/// A controller of layers.
#[derive(Debug, Default)]
pub struct Controller {
    vram: Vram,
    collection: Vec<Layer>,
}

impl Controller {
    /// Creates an instance of this type.
    ///
    /// # Safety
    ///
    /// This function is unsafe because this library may break memory safety by trying to access an
    /// invalid memory if `base_addr_of_vram` is not a correct address.
    ///
    /// Also this library may access to the memory outside of VRAM if `resolution` contains larger
    /// value than the actual one.
    pub unsafe fn new(
        resolution: Vec2<u32>,
        bits_per_pixel: u32,
        base_addr_of_vram: usize,
    ) -> Self {
        Self {
            vram: Vram::new(resolution, bits_per_pixel, base_addr_of_vram),
            collection: Vec::new(),
        }
    }

    /// Add a layer.
    ///
    /// This method returns an ID of the layer. You must save the id to edit the one.
    ///
    /// Added layer comes to the front. All layers behind the one will be hidden.
    ///
    /// After adding a layer, layers will be redrawn.
    pub fn add_layer(&mut self, layer: Layer) -> Id {
        let id = layer.id;
        let top_left = layer.top_left;
        let len = layer.len;
        self.collection.push(layer);
        self.redraw(top_left, len);
        id
    }

    /// Edit a layer.
    ///
    /// You can edit a layer by indexing `Layer` type. For more information, see the description of
    /// `Index` implementation of `Layer` type.
    ///
    /// After editing, layers will be redrawn. This may take time if the layer is large. In such
    /// cases, use [`set_pixel`] instead.
    pub fn edit_layer<T>(&mut self, id: Id, f: T) -> Result<(), Error>
    where
        T: Fn(&mut Layer),
    {
        let layer = self.id_to_layer(id)?;
        let layer_top_left = layer.top_left;
        let layer_len = layer.len;
        f(layer);
        self.redraw(layer_top_left, layer_len);
        Ok(())
    }

    /// Set a color on pixel.
    ///
    /// `coord` is the coordinate of the relative position from the top-left of the layer. If `color` is `None`, the pixel is transparent.
    ///
    /// After editing, only the edited pixel will be redrawn.
    pub fn set_pixel(
        &mut self,
        id: Id,
        coord: Vec2<u32>,
        color: Option<RGB8>,
    ) -> Result<(), Error> {
        let layer = self.id_to_layer(id)?;
        let layer_top_left = layer.top_left;
        layer[usize::try_from(coord.y).unwrap()][usize::try_from(coord.x).unwrap()] = color;

        self.redraw(layer_top_left + coord.as_(), Vec2::one());
        Ok(())
    }

    /// Slide a layer.
    ///
    /// The value of `new_top_left` can be negative, or larger than screen resolution. In such
    /// cases, any part of the layer that extends outside the screen will not be drawn.
    ///
    /// After sliding, layers will be redrawn.
    pub fn slide_layer(&mut self, id: Id, new_top_left: Vec2<i32>) -> Result<(), Error> {
        let layer = self.id_to_layer(id)?;
        let old_top_left = layer.top_left;
        let layer_len = layer.len;
        layer.slide(new_top_left);
        self.redraw(old_top_left, layer_len);
        self.redraw(new_top_left, layer_len);
        Ok(())
    }

    fn redraw(&self, mut vram_top_left: Vec2<i32>, len: Vec2<u32>) {
        vram_top_left = Vec2::<i32>::max(
            Vec2::min(vram_top_left, self.vram.resolution.as_()),
            Vec2::zero(),
        );

        let vram_bottom_right = vram_top_left + len.as_();
        let vram_bottom_right = Vec2::<i32>::max(
            Vec2::min(vram_bottom_right, self.vram.resolution.as_()),
            Vec2::zero(),
        );

        for layer in &self.collection {
            let layer_bottom_right = layer.top_left + layer.len.as_();

            let top_left =
                Vec2::<i32>::min(Vec2::max(vram_top_left, layer.top_left), layer_bottom_right);
            let bottom_right =
                Vec2::<i32>::max(top_left, Vec2::min(vram_bottom_right, layer_bottom_right));

            for y in top_left.y..bottom_right.y {
                for x in top_left.x..bottom_right.x {
                    if let Some(rgb) =
                        layer.buf[(y - layer.top_left.y) as usize][(x - layer.top_left.x) as usize]
                    {
                        self.vram.set_color(Vec2::new(x, y).as_(), rgb)
                    }
                }
            }
        }
    }

    fn id_to_layer(&mut self, id: Id) -> Result<&mut Layer, Error> {
        self.collection
            .iter_mut()
            .find(|layer| layer.id == id)
            .ok_or_else(|| Error::NoSuchLayer(id))
    }
}

/// Represents a layer.
#[derive(PartialEq, Eq, Hash, Debug, Default)]
pub struct Layer {
    buf: Vec<Vec<Option<RGB8>>>,
    top_left: Vec2<i32>,
    len: Vec2<u32>,
    id: Id,
}

impl Layer {
    /// Creates an instance of this struct.
    ///
    /// `top_left`, `len`, and `top_left + len`  can be negative, or larger than the resolution of
    /// the screen. In such cases, parts that does not fit in the screen will not be drawn.
    pub fn new(top_left: Vec2<i32>, len: Vec2<u32>) -> Self {
        Self {
            buf: vec![vec![None; len.x.try_into().unwrap()]; len.y.try_into().unwrap()],
            top_left,
            len,
            id: Id::new(),
        }
    }

    fn slide(&mut self, new_top_left: Vec2<i32>) {
        self.top_left = new_top_left;
    }
}

/// Layer can index into each pixels.
impl Index<usize> for Layer {
    /// `None` represents the pixel is transparent.
    type Output = [Option<RGB8>];

    fn index(&self, index: usize) -> &Self::Output {
        &self.buf[index]
    }
}

impl IndexMut<usize> for Layer {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.buf[index]
    }
}

/// An almost unique id to distinguish each layer.
///
/// You have to save this id to edit, and slide a layer.
///
/// The id may conflict if you create lots of layers. Strictly speaking, creating more than `u64::MAX` layers
/// will create layers having the same ID.
#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Hash, Debug, Default)]
pub struct Id(u64);
impl Id {
    fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        Self(ID.fetch_add(1, Relaxed))
    }
}

/// Errors returned by each method.
#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Hash, Debug)]
pub enum Error {
    /// No layer has the provided ID.
    NoSuchLayer(Id),
}

#[derive(Debug, Default)]
struct Vram {
    resolution: Vec2<u32>,
    bpp: u32,
    base_addr: usize,
}

impl Vram {
    fn new(resolution: Vec2<u32>, bpp: u32, base_addr: usize) -> Self {
        Self {
            resolution,
            bpp,
            base_addr,
        }
    }

    fn set_color(&self, coord: Vec2<u32>, rgb: RGB8) {
        assert_eq!(
            Vec2::<u32>::max(Vec2::<u32>::min(coord, self.resolution), Vec2::zero()),
            coord
        );

        let offset_from_base = ((coord.y * self.resolution.x + coord.x) * self.bpp / 8) as isize;
        let ptr = (self.base_addr as isize + offset_from_base) as usize;

        // Using `offset` causes UB. See the official doc of `offset` method.
        // TODO: Support for other orders of RGB.
        unsafe {
            ptr::write(ptr as _, rgb.b);
            ptr::write((ptr + size_of::<u8>()) as _, rgb.g);
            ptr::write((ptr + size_of::<u8>() * 2) as _, rgb.r);
        }
    }
}
