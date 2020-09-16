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
//! const SCREEN_WIDTH: usize = 10;
//! const SCREEN_HEIGHT: usize = 10;
//! const BPP: usize = 32;
//! let mut pseudo_vram = [0u8; SCREEN_WIDTH * SCREEN_HEIGHT * BPP / 8];
//! let ptr = pseudo_vram.as_ptr() as usize;
//! let mut controller =
//!     unsafe { screen_layer::Controller::new(Vec2::new(SCREEN_WIDTH, SCREEN_HEIGHT), BPP, ptr) };
//!
//! const LAYER_WIDTH: usize = 5;
//! const LAYER_HEIGHT: usize = 5;
//! let layer = Layer::new(Vec2::new(0, 0), Vec2::new(LAYER_WIDTH, LAYER_HEIGHT));
//! let id = controller.add_layer(layer);
//!
//! controller
//!     .edit_layer(id, |layer: &mut Layer| {
//!         for i in 0..LAYER_WIDTH {
//!             layer[i][i] = Some(RGB8::new(0, 255, 0));
//!         }
//!     })
//!     .unwrap();
//!
//! for i in 0..LAYER_WIDTH {
//!     assert_eq!(pseudo_vram[BPP / 8 * (i * SCREEN_WIDTH + i)], 0);
//!     assert_eq!(pseudo_vram[BPP / 8 * (i * SCREEN_WIDTH + i) + 1], 255);
//!     assert_eq!(pseudo_vram[BPP / 8 * (i * SCREEN_WIDTH + i) + 2], 0);
//! }
//! ```

#![no_std]

#[deny(clippy::all)]
#[macro_use]
extern crate alloc;

use {
    alloc::vec::Vec,
    core::{
        mem::size_of,
        ops::{Index, IndexMut},
        ptr,
        sync::atomic::{AtomicUsize, Ordering::Relaxed},
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
    /// invalid memory if `base_addr_of_vram` is an invalid address.
    ///
    /// Also this library may access to the memory outside of VRAM if `resolution` contains larger
    /// value than the actual one.
    pub unsafe fn new(
        resolution: Vec2<usize>,
        bits_per_pixel: usize,
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
    /// After editing, layers will be redrawn.
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

    /// Slide a layer.
    ///
    /// The value of `new_top_left` can be negative, or larger than screen resolution. In such
    /// cases, any part of the layer that extends outside the screen will not be drawn.
    ///
    /// After sliding, layers will be redrawn.
    pub fn slide_layer(&mut self, id: Id, new_top_left: Vec2<isize>) -> Result<(), Error> {
        let layer = self.id_to_layer(id)?;
        let old_top_left = layer.top_left;
        let layer_len = layer.len;
        layer.slide(new_top_left);
        self.redraw(old_top_left, layer_len);
        self.redraw(new_top_left, layer_len);
        Ok(())
    }

    fn redraw(&self, mut vram_top_left: Vec2<isize>, len: Vec2<usize>) {
        vram_top_left = Vec2::<isize>::max(
            Vec2::min(vram_top_left, self.vram.resolution.as_()),
            Vec2::zero(),
        );

        let vram_bottom_right = vram_top_left + len.as_();
        let vram_bottom_right = Vec2::<isize>::max(
            Vec2::min(vram_bottom_right, self.vram.resolution.as_()),
            Vec2::zero(),
        );

        for layer in &self.collection {
            let layer_bottom_right = layer.top_left + layer.len.as_();

            let top_left =
                Vec2::<isize>::min(Vec2::max(vram_top_left, layer.top_left), layer_bottom_right);
            let bottom_right =
                Vec2::<isize>::max(top_left, Vec2::min(vram_bottom_right, layer_bottom_right));

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
    top_left: Vec2<isize>,
    len: Vec2<usize>,
    id: Id,
}

impl Layer {
    /// Creates an instance of this struct.
    ///
    /// `top_left`, `len`, and `top_left + len`  can be negative, or larger than the resolution of
    /// the screen. In such cases, parts that does not fit in the screen will not be drawn.
    pub fn new(top_left: Vec2<isize>, len: Vec2<usize>) -> Self {
        Self {
            buf: vec![vec![None; len.x]; len.y],
            top_left,
            len,
            id: Id::new(),
        }
    }

    fn slide(&mut self, new_top_left: Vec2<isize>) {
        self.top_left = new_top_left;
    }
}

/// Layer can index into each pixels.
impl Index<usize> for Layer {
    /// `None` represents the pixel is transparent.
    type Output = Vec<Option<RGB8>>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.buf[index]
    }
}

impl IndexMut<usize> for Layer {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.buf[index]
    }
}

/// An almost unique id to distinguish each layers.
///
/// You have to save this id to edit, and slide a layer.
///
/// The id may conflict if you create lots of layers. Strictly speaking, creating `usize::MAX` layers
/// will create layers having the same ID.
#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq, Hash, Debug, Default)]
pub struct Id(usize);
impl Id {
    fn new() -> Self {
        static ID: AtomicUsize = AtomicUsize::new(0);
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
    resolution: Vec2<usize>,
    bpp: usize,
    base_addr: usize,
}

impl Vram {
    fn new(resolution: Vec2<usize>, bpp: usize, base_addr: usize) -> Self {
        Self {
            resolution,
            bpp,
            base_addr,
        }
    }

    fn set_color(&self, coord: Vec2<usize>, rgb: RGB8) {
        assert_eq!(
            Vec2::<usize>::max(Vec2::<usize>::min(coord, self.resolution), Vec2::zero()),
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
