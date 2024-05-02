#![deny(clippy::all)]

extern crate clustering;
extern crate image;
extern crate itertools;
extern crate lab;
#[macro_use]
extern crate napi_derive;

use std::io::Cursor;

use clustering::*;
use image::{DynamicImage, GenericImageView, ImageResult};
use image::imageops::FilterType;
use image::io::Reader;
use itertools::Itertools;
use lab::Lab;
use napi::{
  bindgen_prelude::{Array, AsyncTask},
  Result, Task
};
use napi::bindgen_prelude::Buffer;
use de2000::are_colors_similar;
use crate::JsBufferOrString::{JsBuffer, Path};

mod de2000;

enum JsBufferOrString {
  JsBuffer(Buffer),
  Path(String),
}

fn open_js_buffer(buffer: Buffer) -> ImageResult<DynamicImage> {
  let reader = Reader::new(Cursor::new(buffer.as_ref()))
      .with_guessed_format()?;
  reader.decode()
}

fn get_top_colours(data: JsBufferOrString) -> Result<Vec<[u8; 3]>> {
  let img_file = match data {
      Path(path) => image::open(path),
      JsBuffer(buffer) => open_js_buffer(buffer)
  };
  let img = match img_file {
    Ok(file) => file.resize(48, 48, FilterType::Nearest),
    Err(e) => {
      return Err(napi::Error::new(
        napi::Status::GenericFailure,
        format!("failed to read file, {}", e),
      ));
    }
  };

  let colors = img
    .pixels()
    .map(|(_x, _y, rgba)| {
      let lab = Lab::from_rgba(&rgba.0);
      vec![lab.l, lab.a, lab.b]
    })
    .dedup()
    .collect::<Vec<_>>();

  let k = 16;
  let max_iter = 1024;

  let clusters = kmeans(k, &colors, max_iter)
    .centroids
    .iter()
    // .filter(|c| c.at(0) + c.at(1) + c.at(2) > 0.0)
    .map(|c| Lab {
      l: c.at(0) as f32,
      a: c.at(1) as f32,
      b: c.at(2) as f32,
    })
    // sorts by L component of lab
    .collect::<Vec<_>>();
  let clusters_len = clusters.len();

  let sorted_clusters = clusters
    .iter()
    .sorted_by(|c1, c2| c1.l.total_cmp(&c2.l))
    .collect::<Vec<_>>();

  let reduced = sorted_clusters
    .iter()
    .enumerate()
    .filter(|(i, color)| {
      if i + 1 >= clusters_len {
        false
      } else {
        let next_color = sorted_clusters[i + 1];
        !are_colors_similar(color, &next_color)
      }
    })
    .map(|(_, color)| *(*color))
    // restores original sorting which was based on frequency
    .sorted_by_key(|&c| clusters.iter().position(|&r| r == c).unwrap())
    .collect::<Vec<_>>();

  Ok(lab::labs_to_rgbs(&reduced))
}

pub struct ExtractColours {
  path: JsBufferOrString,
}

impl Task for ExtractColours {
  type Output = Vec<[u8; 3]>;
  type JsValue = Array;

  fn compute(&mut self) -> Result<Self::Output> {
    get_top_colours(match &self.path {
      Path(path) => Path(path.clone()),
      JsBuffer(buffer) => JsBuffer(buffer.clone())
    })
  }

  fn resolve(&mut self, env: napi::Env, output: Self::Output) -> Result<Self::JsValue> {
    env.create_array(output.len() as u32).and_then(|mut arr| {
      for (i, color) in output.iter().enumerate() {
        arr.set(i as u32, *color)?
      }
      Ok(arr)
    })
  }
}

pub struct ExtractHexColours {
  path: JsBufferOrString,
}

impl Task for ExtractHexColours {
  type Output = Vec<String>;
  type JsValue = Array;

  fn compute(&mut self) -> Result<Self::Output> {
    match get_top_colours(match &self.path {
      Path(path) => Path(path.clone()),
      JsBuffer(buffer) => JsBuffer(buffer.clone())
    }) {
      Ok(colors) => Ok(colors.iter().map(|color| rgb2hex(color)).collect()),
      Err(err) => Err(err),
    }
  }

  fn resolve(&mut self, env: napi::Env, output: Self::Output) -> Result<Self::JsValue> {
    env.create_array(output.len() as u32).and_then(|mut arr| {
      for (i, color) in output.iter().enumerate() {
        arr.set(i as u32, color.clone())?
      }
      Ok(arr)
    })
  }
}

#[napi]
pub fn top_colours(path: String) -> AsyncTask<ExtractColours> {
  AsyncTask::new(ExtractColours { path: Path(path) })
}

#[napi]
pub fn top_colors(path: String) -> AsyncTask<ExtractColours> {
  AsyncTask::new(ExtractColours { path: Path(path)  })
}

#[napi]
pub fn top_colours_hex(path: String) -> AsyncTask<ExtractHexColours> {
  AsyncTask::new(ExtractHexColours { path: Path(path)  })
}

#[napi]
pub fn top_colors_hex(path: String) -> AsyncTask<ExtractHexColours> {
  AsyncTask::new(ExtractHexColours { path: Path(path)  })
}

#[napi]
pub fn top_colours_hex_buffer(buffer: Buffer) -> AsyncTask<ExtractHexColours> {
  AsyncTask::new(ExtractHexColours { path: JsBuffer(buffer)  })
}

#[napi]
pub fn top_colours_buffer(buffer: Buffer) -> AsyncTask<ExtractColours> {
  AsyncTask::new(ExtractColours { path: JsBuffer(buffer)  })
}

#[napi]
pub fn top_colors_hex_buffer(buffer: Buffer) -> AsyncTask<ExtractHexColours> {
  AsyncTask::new(ExtractHexColours { path: JsBuffer(buffer)  })
}

#[napi]
pub fn top_colors_buffer(buffer: Buffer) -> AsyncTask<ExtractColours> {
  AsyncTask::new(ExtractColours { path: JsBuffer(buffer)  })
}

fn rgb2hex([r, g, b]: &[u8; 3]) -> String {
  format!("#{r:02x}{g:02x}{b:02x}")
}
