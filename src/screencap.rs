use std::{fs::File, io::{BufReader, Read, Write}, path::PathBuf, process::{Command, Stdio}};

use image::{DynamicImage, ImageError, RgbaImage};

#[derive(Debug)]
pub enum LoadBitmapError {
    ImageError(ImageError),
    IoError(std::io::Error),
}

impl From<std::io::Error> for LoadBitmapError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}
impl From<ImageError> for LoadBitmapError {
    fn from(value: ImageError) -> Self {
        Self::ImageError(value)
    }
}

pub fn load_bitmap(input: &[u8]) -> Result<DynamicImage, LoadBitmapError> {
    match image::load_from_memory_with_format(input, image::ImageFormat::Bmp) {
        Ok(image) => {
            Ok(image)
        },
        Err(err) => {
            match err {
                image::ImageError::Decoding(_) => {
                    let width = u32::from_le_bytes(input[..4].try_into().unwrap());
                    let height = u32::from_le_bytes(input[4..8].try_into().unwrap());
                    let image_buffer = RgbaImage::from_raw(width, height, input[16..].to_vec()).expect("Invalid bitmap data");
                    let image:DynamicImage = image_buffer.into();
                    Ok(image)
                },
                _ => {
                    Err(LoadBitmapError::ImageError(err))
                }
            }
        },
    }
}

pub fn load_bitmap_from_file(path: PathBuf) -> Result<DynamicImage, LoadBitmapError> {
    let mut buf = Vec::new();
    File::open(path)?.read_to_end(&mut buf)?;
    load_bitmap(&buf)
}

pub fn load_png_from_file(path: PathBuf) -> Result<DynamicImage, LoadBitmapError> {
    image::load(BufReader::new(File::open(path)?), image::ImageFormat::Png).map_err(|err|err.into())
}

#[derive(Debug)]
pub enum ScreencapError {
    LoadBitmapError(LoadBitmapError),
    IoError(std::io::Error),
    Failed,
}
impl From<std::io::Error> for ScreencapError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}
impl From<LoadBitmapError> for ScreencapError {
    fn from(value: LoadBitmapError) -> Self {
        Self::LoadBitmapError(value)
    }
}

pub fn screencap(device:&str) -> Result<DynamicImage, ScreencapError> {
    let cmd = Command::new("adb").arg("-s").arg(device).arg("exec-out").arg("screencap")
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .stdout(Stdio::piped())
    .spawn()?;
    let output = cmd.wait_with_output()?;
    if output.status.success() {
        load_bitmap(&output.stdout).map_err(|err|err.into())
    }
    else {
        Err(ScreencapError::Failed)
    }
}