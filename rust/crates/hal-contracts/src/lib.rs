#![no_std]
#![allow(clippy::missing_errors_doc)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbEvent<'a> {
    Connected,
    Disconnected,
    Received(&'a [u8]),
}

pub trait UsbTransport {
    type Error;

    fn transmit(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

pub trait Clock {
    fn milliseconds(&self) -> u64;
}

pub trait SettingsStore {
    type Error;

    fn load(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error>;
    fn save(&mut self, data: &[u8]) -> Result<(), Self::Error>;
}

pub trait DisplayDevice {
    type Error;

    fn dimensions(&self) -> (u16, u16);
    fn frame_buffer(&mut self) -> &mut [u16];
    fn present(&mut self) -> Result<(), Self::Error>;
}

pub trait TouchDevice {
    type Error;

    fn poll(&mut self) -> Result<Option<TouchPoint>, Self::Error>;
}

pub trait SwitchDevice {
    type Error;

    fn pressed_mask(&mut self) -> Result<u32, Self::Error>;
}

pub trait LedDevice {
    type Error;

    fn set_rgb(&mut self, index: usize, red: u8, green: u8, blue: u8) -> Result<(), Self::Error>;
    fn present(&mut self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchPoint {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoDisplay;

impl DisplayDevice for NoDisplay {
    type Error = core::convert::Infallible;

    fn dimensions(&self) -> (u16, u16) {
        (0, 0)
    }

    fn frame_buffer(&mut self) -> &mut [u16] {
        &mut []
    }

    fn present(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoTouch;

impl TouchDevice for NoTouch {
    type Error = core::convert::Infallible;

    fn poll(&mut self) -> Result<Option<TouchPoint>, Self::Error> {
        Ok(None)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoLeds;

impl LedDevice for NoLeds {
    type Error = core::convert::Infallible;

    fn set_rgb(
        &mut self,
        _index: usize,
        _red: u8,
        _green: u8,
        _blue: u8,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn present(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
