use core::fmt;

pub static mut WRITER: Lazy<Writer> = Lazy::new(|| Writer {
    column_position: 0,
    color_code: ColorCode::new(Color::Yellow, Color::Black),
    buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
});

/// The standard color palette in VGA text mode.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// A combination of a foreground and a background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(fg: Color, bg: Color) -> ColorCode {
        ColorCode((bg as u8) << 4 | (fg as u8))
    }
}

/// A screen character in the VGA text buffer, consisting of an ASCII character and a `ColorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

/// The height of the text buffer (normally 25 lines).
const BUFFER_HEIGHT: usize = 25;
/// The width of the text buffer (normally 80 columns).
const BUFFER_WIDTH: usize = 80;

/// A structure representing the VGA text buffer.
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// A writer type that allows writing ASCII bytes and strings to an underlying `Buffer`.
///
/// Wraps lines at `BUFFER_WIDTH`. Supports newline characters and implements the
/// `core::fmt::Write` trait.
pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    /// Writes an ASCII byte to the buffer.
    ///
    /// Wraps lines at `BUFFER_WIDTH`. Supports the `\n` newline character.
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code: self.color_code,
                });
                self.column_position += 1;
            }
        }
    }

    /// Writes the given ASCII string to the buffer.
    ///
    /// Wraps lines at `BUFFER_WIDTH`. Supports the `\n` newline character. Does **not**
    /// support strings with non-ASCII characters, since they can't be printed in the VGA text
    /// mode.
    pub fn write_string(&mut self, string: &str) {
        for byte in string.bytes() {
            match byte {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    unsafe {
        #[allow(static_mut_refs)]
        WRITER.write_fmt(args).unwrap_unchecked();
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::vga_buffer::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

#[derive(Debug)]
#[repr(transparent)]
struct Volatile<T: Copy>(T);

#[allow(dead_code)]
impl<T: Copy> Volatile<T> {
    fn new(value: T) -> Self {
        Self(value)
    }

    fn read(&self) -> T {
        unsafe { core::ptr::read_volatile(&self.0) }
    }

    fn write(&mut self, value: T) {
        unsafe { core::ptr::write_volatile(&mut self.0, value) };
    }
}

impl<T: Copy + Default> Default for Volatile<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

use core::cell::{Cell, UnsafeCell};

struct OnceCell<T> {
    // Invariant: written to at most once.
    inner: UnsafeCell<Option<T>>,
}

// No threads nor CPU interrupts at this stage so lying to the compiler is fine.
unsafe impl<T> Send for OnceCell<T> {}
unsafe impl<T> Sync for OnceCell<T> {}

#[allow(dead_code)]
impl<T> OnceCell<T> {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(None),
        }
    }

    pub fn get(&self) -> Option<&T> {
        unsafe { &*self.inner.get() }.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.inner.get_mut().as_mut()
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        if self.get().is_some() {
            return Err(value);
        }
        unsafe { *self.inner.get() = Some(value) };
        Ok(())
    }

    /// The reentrant case is allowed and is UB. An `intialising` flag can be used in the future.
    /// ```
    /// let cell = OnceCell::new();
    /// let x = cell.get_or_init(|| {
    ///     cell.get_or_init(|| 2);
    ///     1
    /// });
    /// assert_eq!(x, 1);
    /// ```
    pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        if let Some(value) = self.get() {
            return value;
        }
        unsafe {
            *self.inner.get() = Some(f());
            self.get().unwrap_unchecked()
        }
    }

    /// The reentrant case is statically impossible since we would be borrowing `&mut self` more
    /// than once at a time and the compiler makes sure the invariant holds.
    pub fn get_mut_or_init(&mut self, f: impl FnOnce() -> T) -> &mut T {
        self.inner.get_mut().get_or_insert_with(f)
    }
}

pub struct Lazy<T, F = fn() -> T> {
    cell: OnceCell<T>,
    init: Cell<Option<F>>,
}

// No threads nor CPU interrupts at this stage so lying to the compiler is fine.
unsafe impl<T> Send for Lazy<T> {}
unsafe impl<T> Sync for Lazy<T> {}

impl<T, F: FnOnce() -> T> Lazy<T, F> {
    const fn new(init: F) -> Self {
        Self {
            cell: OnceCell::new(),
            init: Cell::new(Some(init)),
        }
    }

    pub fn force(this: &Lazy<T, F>) -> &T {
        this.cell.get_or_init(|| match this.init.take() {
            Some(f) => f(),
            None => unreachable!(),
        })
    }

    pub fn force_mut(this: &mut Lazy<T, F>) -> &mut T {
        this.cell.get_mut_or_init(|| match this.init.take() {
            Some(f) => f(),
            None => unreachable!(),
        })
    }
}

impl<T, F: FnOnce() -> T> core::ops::Deref for Lazy<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        Lazy::force(self)
    }
}

impl<T, F: FnOnce() -> T> core::ops::DerefMut for Lazy<T, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Lazy::force_mut(self)
    }
}
