// from https://github.com/rsaarelm/scancode-rs
/*
The MIT License (MIT)

Copyright (c) 2016 Risto Saarelma

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
 */

use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

/// Default values for hardware keyboard keys.
///
/// After USB HID Usage Tables document at http://www.usb.org/developers/hidpage/Hut1_12v2.pdf
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Serialize, Deserialize, TryFromPrimitive)]
pub enum ScanCode {
    A = 4,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    N,
    M,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num1 = 30,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Num0,
    Enter = 40,
    Escape,
    Backspace,
    Tab,
    Space,
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    NonUsHash = 50,
    Semicolon,
    Apostrophe,
    Grave,
    Comma,
    Period,
    Slash,
    CapsLock,
    F1 = 58,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    PrintScreen = 70,
    ScrollLock,
    Pause,
    Insert,
    Home,
    PageUp,
    Delete,
    End,
    PageDown,
    Right,
    Left,
    Down,
    Up,
    NumLock = 83,
    PadDivide,
    PadMultiply,
    PadMinus,
    PadPlus,
    PadEnter,
    Pad1 = 89,
    Pad2,
    Pad3,
    Pad4,
    Pad5,
    Pad6,
    Pad7,
    Pad8,
    Pad9,
    Pad0,
    PadDecimal,
    NonUsBackslash = 100,
    PadEquals = 103,
    Menu = 118,
    Mute = 127,
    VolumeUp,
    VolumeDown,
    SysReq = 154,
    LeftControl = 224,
    LeftShift,
    LeftAlt,
    LeftGui,
    RightControl,
    RightShift,
    RightAlt,
    RightGui,
}

impl ScanCode {
    /// Try to convert a hardware scancode from the current platform to a Scancode enum value.
    pub fn from_usb_hid(hardware_scancode: u32) -> Option<ScanCode> {
        ScanCode::try_from(hardware_scancode as u8).ok()
    }

    /// Try to convert a hardware scancode from the current platform to a Scancode enum value.
    pub fn from_platform(hardware_scancode: u8) -> Option<ScanCode> {
        #[cfg(target_os = "linux")]
        use scancode_linux::MAP;
        #[cfg(target_os = "macos")]
        use scancode_macos::MAP;
        #[cfg(target_os = "windows")]
        use scancode_windows::MAP;

        if (hardware_scancode as usize) < MAP.len() {
            MAP[hardware_scancode as usize]
        } else {
            None
        }
    }

    pub fn from_windows(hardware_scancode: u8) -> Option<ScanCode> {
        if (hardware_scancode as usize) < scancode_windows::MAP.len() {
            scancode_windows::MAP[hardware_scancode as usize]
        } else {
            None
        }
    }

    pub fn from_macos(hardware_scancode: u8) -> Option<ScanCode> {
        if (hardware_scancode as usize) < scancode_macos::MAP.len() {
            scancode_macos::MAP[hardware_scancode as usize]
        } else {
            None
        }
    }

    pub fn from_linux(hardware_scancode: u8) -> Option<ScanCode> {
        if (hardware_scancode as usize) < scancode_linux::MAP.len() {
            scancode_linux::MAP[hardware_scancode as usize]
        } else {
            None
        }
    }
}

impl Into<u32> for ScanCode {
    fn into(self) -> u32 {
        self as u32
    }
}

mod scancode_macos {
    use super::ScanCode;
    use super::ScanCode::*;

    /// Keyboard scancode map for OS X.
    pub static MAP: [Option<ScanCode>; 127] = [
        Some(A),
        Some(S),
        Some(D),
        Some(F),
        Some(H),
        Some(G),
        Some(Z),
        Some(X),
        Some(C),
        Some(V),
        Some(NonUsBackslash),
        Some(B),
        Some(Q),
        Some(W),
        Some(E),
        Some(R),
        Some(Y),
        Some(T),
        Some(Num1),
        Some(Num2),
        Some(Num3),
        Some(Num4),
        Some(Num6),
        Some(Num5),
        Some(Equals),
        Some(Num9),
        Some(Num7),
        Some(Minus),
        Some(Num8),
        Some(Num0),
        Some(RightBracket),
        Some(O),
        Some(U),
        Some(LeftBracket),
        Some(I),
        Some(P),
        Some(Enter),
        Some(L),
        Some(J),
        Some(Apostrophe),
        Some(K),
        Some(Semicolon),
        Some(Backslash),
        Some(Comma),
        Some(Slash),
        Some(N),
        Some(M),
        Some(Period),
        Some(Tab),
        Some(Space),
        Some(Grave),
        Some(Backspace),
        Some(PadEnter),
        Some(Escape),
        Some(RightGui),
        Some(LeftGui),
        Some(LeftShift),
        Some(CapsLock),
        Some(LeftAlt),
        Some(LeftControl),
        Some(RightShift),
        Some(RightAlt),
        Some(RightControl),
        None,
        None,
        Some(PadDecimal),
        None,
        Some(PadMultiply),
        None,
        Some(PadPlus),
        None,
        Some(NumLock),
        Some(VolumeUp),
        Some(VolumeDown),
        Some(Mute),
        Some(PadDivide),
        Some(PadEnter),
        None,
        Some(PadMinus),
        None,
        None,
        Some(PadEquals),
        Some(Pad0),
        Some(Pad1),
        Some(Pad2),
        Some(Pad3),
        Some(Pad4),
        Some(Pad5),
        Some(Pad6),
        Some(Pad7),
        None,
        Some(Pad8),
        Some(Pad9),
        None,
        None,
        None,
        Some(F5),
        Some(F6),
        Some(F7),
        Some(F3),
        Some(F8),
        Some(F9),
        None,
        Some(F11),
        None,
        None, // F13,
        Some(Pause),
        Some(PrintScreen),
        None,
        Some(F10),
        None,
        Some(F12),
        None,
        Some(ScrollLock),
        Some(Insert),
        Some(Home),
        Some(PageUp),
        Some(Delete),
        Some(F4),
        Some(End),
        Some(F2),
        Some(PageDown),
        Some(F1),
        Some(Left),
        Some(Right),
        Some(Down),
        Some(Up),
    ];
}

mod scancode_linux {
    use super::ScanCode;
    use super::ScanCode::*;

    /// Keyboard scancode map for Linux.
    pub static MAP: [Option<ScanCode>; 136] = [
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(Escape),
        Some(Num1),
        Some(Num2),
        Some(Num3),
        Some(Num4),
        Some(Num5),
        Some(Num6),
        Some(Num7),
        Some(Num8),
        Some(Num9),
        Some(Num0),
        Some(Minus),
        Some(Equals),
        Some(Backspace),
        Some(Tab),
        Some(Q),
        Some(W),
        Some(E),
        Some(R),
        Some(T),
        Some(Y),
        Some(U),
        Some(I),
        Some(O),
        Some(P),
        Some(LeftBracket),
        Some(RightBracket),
        Some(Enter),
        Some(LeftControl),
        Some(A),
        Some(S),
        Some(D),
        Some(F),
        Some(G),
        Some(H),
        Some(J),
        Some(K),
        Some(L),
        Some(Semicolon),
        Some(Apostrophe),
        Some(Grave),
        Some(LeftShift),
        Some(Backslash),
        Some(Z),
        Some(X),
        Some(C),
        Some(V),
        Some(B),
        Some(N),
        Some(M),
        Some(Comma),
        Some(Period),
        Some(Slash),
        Some(RightShift),
        Some(PadMultiply),
        Some(LeftAlt),
        Some(Space),
        Some(CapsLock),
        Some(F1),
        Some(F2),
        Some(F3),
        Some(F4),
        Some(F5),
        Some(F6),
        Some(F7),
        Some(F8),
        Some(F9),
        Some(F10),
        Some(NumLock),
        Some(ScrollLock),
        Some(Pad7),
        Some(Pad8),
        Some(Pad9),
        Some(PadMinus),
        Some(Pad4),
        Some(Pad5),
        Some(Pad6),
        Some(PadPlus),
        Some(Pad1),
        Some(Pad2),
        Some(Pad3),
        Some(Pad0),
        Some(PadDecimal),
        None,
        None,
        Some(NonUsBackslash),
        Some(F11),
        Some(F12),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(PadEnter),
        Some(RightControl),
        Some(PadDivide),
        Some(SysReq),
        Some(RightAlt),
        None,
        Some(Home),
        Some(Up),
        Some(PageUp),
        Some(Left),
        Some(Right),
        Some(End),
        Some(Down),
        Some(PageDown),
        Some(Insert),
        Some(Delete),
        None,
        Some(Mute),
        Some(VolumeDown),
        Some(VolumeUp),
        None,
        None,
        None,
        Some(Pause),
        None,
        None,
        None,
        None,
        None,
        Some(LeftGui),
        Some(RightGui),
        Some(Menu),
    ];
}

mod scancode_windows {
    use super::ScanCode;
    use super::ScanCode::*;

    /// Keyboard scancode map for Microsoft Windows.
    pub static MAP: [Option<ScanCode>; 94] = [
        None,
        Some(Escape),
        Some(Num1),
        Some(Num2),
        Some(Num3),
        Some(Num4),
        Some(Num5),
        Some(Num6),
        Some(Num7),
        Some(Num8),
        Some(Num9),
        Some(Num0),
        Some(Minus),
        Some(Equals),
        Some(Backspace),
        Some(Tab),
        Some(Q),
        Some(W),
        Some(E),
        Some(R),
        Some(T),
        Some(Y),
        Some(U),
        Some(I),
        Some(O),
        Some(P),
        Some(LeftBracket),
        Some(RightBracket),
        Some(Enter),
        Some(LeftControl),
        Some(A),
        Some(S),
        Some(D),
        Some(F),
        Some(G),
        Some(H),
        Some(J),
        Some(K),
        Some(L),
        Some(Semicolon),
        Some(Apostrophe),
        Some(Grave),
        Some(LeftShift),
        Some(Backslash),
        Some(Z),
        Some(X),
        Some(C),
        Some(V),
        Some(B),
        Some(N),
        Some(M),
        Some(Comma),
        Some(Period),
        Some(Slash),
        Some(RightShift),
        Some(PadMultiply), // Also PrintScreen
        Some(LeftAlt),
        Some(Space),
        Some(CapsLock),
        Some(F1),
        Some(F2),
        Some(F3),
        Some(F4),
        Some(F5),
        Some(F6),
        Some(F7),
        Some(F8),
        Some(F9),
        Some(F10),
        Some(NumLock),
        Some(ScrollLock),
        Some(Home),   // Also Pad7
        Some(Up),     // Also Pad8
        Some(PageUp), // Also Pad9
        Some(PadMinus),
        Some(Left), // Also Pad4
        Some(Pad5),
        Some(Right), // Also Pad6
        Some(PadPlus),
        Some(End),      // Also Pad1
        Some(Down),     // Also Pad2
        Some(PageDown), // Also Pad3
        Some(Insert),   // Also Pad0
        Some(Delete),   // Also PadDecimal
        None,
        None,
        Some(NonUsBackslash),
        Some(F11),
        Some(F12),
        Some(Pause),
        None,
        Some(LeftGui),
        Some(RightGui),
        Some(Menu),
    ];
}
