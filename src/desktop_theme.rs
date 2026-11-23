//! The theme the desktop prefers, on Linux and the BSDs. winit tells egui the system theme
//! on Windows and macOS only, so with "Same as system" Oculante was dark on every Linux
//! desktop. 0.9.6 asked the desktop portal through dark-light once at the start. This asks
//! the portal too and follows it when it changes, with the few D-Bus messages that takes
//! on the session bus socket instead of a D-Bus library.

use egui::{Context, Theme};
use std::sync::atomic::{AtomicU8, Ordering};

/// The portal's color-scheme: 0 while nothing is known or the desktop has no preference,
/// 1 dark, 2 light
static COLOR_SCHEME: AtomicU8 = AtomicU8::new(0);

/// The theme the desktop prefers, once the portal said so
pub fn preferred() -> Option<Theme> {
    theme_of(COLOR_SCHEME.load(Ordering::Relaxed))
}

fn theme_of(color_scheme: u8) -> Option<Theme> {
    match color_scheme {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    }
}

/// Asks the desktop for its theme and keeps listening for changes, in the background. The
/// first call starts it, each answer comes with a repaint.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn watch(ctx: &Context) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let ctx = ctx.clone();
        let started = std::thread::Builder::new()
            .name("desktop theme".into())
            .spawn(move || {
                let found = |color_scheme: u32| {
                    log::debug!("The desktop's color scheme is {color_scheme}");
                    COLOR_SCHEME.store(color_scheme.min(3) as u8, Ordering::Relaxed);
                    ctx.request_repaint();
                };
                if let Err(e) = portal::listen(found) {
                    log::debug!("The desktop's theme is not known: {e}");
                }
            });
        if let Err(e) = started {
            log::debug!("Could not ask for the desktop's theme: {e}");
        }
    });
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn watch(_ctx: &Context) {}

#[cfg(all(unix, not(target_os = "macos")))]
mod portal {
    use std::io::{self, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
    const SETTINGS: &str = "org.freedesktop.portal.Settings";
    const APPEARANCE: &str = "org.freedesktop.appearance";
    const COLOR_SCHEME: &str = "color-scheme";
    const BUS: &str = "org.freedesktop.DBus";
    const BUS_PATH: &str = "/org/freedesktop/DBus";

    const METHOD_RETURN: u8 = 2;
    const ERROR: u8 = 3;
    const SIGNAL: u8 = 4;

    // the serials of the calls whose answers are waited for
    const READ_ONE: u32 = 3;
    const READ: u32 = 4;

    /// Reads the color scheme and each change of it and calls `found` with it: 0 for no
    /// preference, 1 dark, 2 light. Returns when there is no portal or the bus goes away.
    pub fn listen(mut found: impl FnMut(u32)) -> io::Result<()> {
        let mut bus = connect()?;
        authenticate(&mut bus)?;
        bus.write_all(&method_call(1, BUS, BUS_PATH, BUS, "Hello", &[]))?;
        let rule = format!(
            "type='signal',interface='{SETTINGS}',member='SettingChanged',path='{PORTAL_PATH}',arg0='{APPEARANCE}'"
        );
        bus.write_all(&method_call(2, BUS, BUS_PATH, BUS, "AddMatch", &[&rule]))?;
        let key = [APPEARANCE, COLOR_SCHEME];
        bus.write_all(&method_call(
            READ_ONE,
            PORTAL,
            PORTAL_PATH,
            SETTINGS,
            "ReadOne",
            &key,
        ))?;
        // the answer can take a while, a portal is often started for the first question
        bus.set_read_timeout(None)?;
        loop {
            let message = read_message(&mut bus)?;
            match (message.kind, message.reply_serial) {
                // ReadOne came with version 2 of the portal (2023), Read is in every one
                (ERROR, Some(READ_ONE)) => bus.write_all(&method_call(
                    READ,
                    PORTAL,
                    PORTAL_PATH,
                    SETTINGS,
                    "Read",
                    &key,
                ))?,
                (ERROR, Some(READ)) => return Err(io::Error::other("no desktop portal")),
                (METHOD_RETURN, Some(READ_ONE | READ)) if message.signature == "v" => {
                    found(message.body().variant_u32()?)
                }
                (SIGNAL, _)
                    if message.member.as_deref() == Some("SettingChanged")
                        && message.signature == "ssv" =>
                {
                    let mut body = message.body();
                    if body.string()? == APPEARANCE && body.string()? == COLOR_SCHEME {
                        found(body.variant_u32()?)
                    }
                }
                _ => {}
            }
        }
    }

    /// The session bus, from DBUS_SESSION_BUS_ADDRESS or where systemd puts it
    fn connect() -> io::Result<UnixStream> {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| {
            let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
            format!("unix:path={runtime}/bus")
        });
        let bus = match socket_of(&address) {
            Some(Socket::Path(path)) => UnixStream::connect(path)?,
            #[cfg(target_os = "linux")]
            Some(Socket::Abstract(name)) => {
                use std::os::linux::net::SocketAddrExt;
                let address = std::os::unix::net::SocketAddr::from_abstract_name(name)?;
                UnixStream::connect_addr(&address)?
            }
            _ => return Err(io::Error::other(format!("no unix socket in {address}"))),
        };
        bus.set_read_timeout(Some(Duration::from_secs(2)))?;
        bus.set_write_timeout(Some(Duration::from_secs(2)))?;
        Ok(bus)
    }

    #[derive(Debug, PartialEq)]
    pub(super) enum Socket {
        Path(String),
        #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
        Abstract(Vec<u8>),
    }

    /// The first unix socket of a D-Bus address like `unix:path=/run/user/1000/bus`.
    /// Several addresses are separated by `;`, bytes may be escaped as `%2f`.
    pub(super) fn socket_of(address: &str) -> Option<Socket> {
        for one in address.split(';') {
            let Some(keys) = one.strip_prefix("unix:") else {
                continue;
            };
            for key in keys.split(',') {
                if let Some(path) = key.strip_prefix("path=") {
                    return Some(Socket::Path(
                        String::from_utf8_lossy(&unescape(path)).into(),
                    ));
                }
                if let Some(name) = key.strip_prefix("abstract=") {
                    return Some(Socket::Abstract(unescape(name)));
                }
            }
        }
        None
    }

    fn unescape(value: &str) -> Vec<u8> {
        let bytes = value.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok());
            match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
                (b'%', Some(byte)) => {
                    out.push(byte);
                    i += 3;
                }
                (byte, _) => {
                    out.push(byte);
                    i += 1;
                }
            }
        }
        out
    }

    /// Logs in with the user id, as the other D-Bus libraries do. Without one the bus asks
    /// with an empty DATA and takes it from the socket.
    fn authenticate(bus: &mut UnixStream) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).ok();
        let hex: String = uid
            .map(|uid| {
                uid.to_string()
                    .bytes()
                    .map(|b| format!("{b:02x}"))
                    .collect()
            })
            .unwrap_or_default();
        let first = match hex.as_str() {
            "" => "\0AUTH EXTERNAL\r\n".to_string(),
            hex => format!("\0AUTH EXTERNAL {hex}\r\n"),
        };
        bus.write_all(first.as_bytes())?;
        for _ in 0..3 {
            let line = read_line(bus)?;
            if line == "OK" || line.starts_with("OK ") {
                return bus.write_all(b"BEGIN\r\n");
            }
            if line == "DATA" || line.starts_with("DATA ") {
                bus.write_all(b"DATA\r\n")?;
                continue;
            }
            return Err(io::Error::other(format!("the bus said {line}")));
        }
        Err(io::Error::other("the bus kept asking"))
    }

    /// A line of the login, read a byte at a time so nothing after it is taken
    fn read_line(bus: &mut impl Read) -> io::Result<String> {
        let mut line = Vec::new();
        let mut byte = [0];
        while !line.ends_with(b"\r\n") {
            bus.read_exact(&mut byte)?;
            line.push(byte[0]);
            if line.len() > 1024 {
                return Err(io::Error::other("the bus said too much"));
            }
        }
        line.truncate(line.len() - 2);
        Ok(String::from_utf8_lossy(&line).into_owned())
    }

    /// Writes values with D-Bus alignment, in little endian
    #[derive(Default)]
    struct Writer(Vec<u8>);

    impl Writer {
        fn align(&mut self, to: usize) {
            while !self.0.len().is_multiple_of(to) {
                self.0.push(0);
            }
        }

        fn u32(&mut self, value: u32) {
            self.align(4);
            self.0.extend(value.to_le_bytes());
        }

        fn string(&mut self, value: &str) {
            self.u32(value.len() as u32);
            self.0.extend(value.as_bytes());
            self.0.push(0);
        }

        fn signature(&mut self, value: &str) {
            self.0.push(value.len() as u8);
            self.0.extend(value.as_bytes());
            self.0.push(0);
        }

        /// A header field: its code and a variant
        fn field(&mut self, code: u8, kind: &str, value: &str) {
            self.align(8);
            self.0.push(code);
            self.signature(kind);
            match kind {
                "g" => self.signature(value),
                _ => self.string(value),
            }
        }
    }

    /// A method call with strings as its arguments
    pub(super) fn method_call(
        serial: u32,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        args: &[&str],
    ) -> Vec<u8> {
        let mut body = Writer::default();
        for arg in args {
            body.string(arg);
        }
        // little endian, a method call, no flags, version 1
        let mut message = Writer(vec![b'l', 1, 0, 1]);
        message.u32(body.0.len() as u32);
        message.u32(serial);
        // the length of the header fields, known once they are written
        message.u32(0);
        message.field(1, "o", path);
        message.field(6, "s", destination);
        message.field(2, "s", interface);
        message.field(3, "s", member);
        if !args.is_empty() {
            message.field(8, "g", &"s".repeat(args.len()));
        }
        let fields = (message.0.len() - 16) as u32;
        message.0[12..16].copy_from_slice(&fields.to_le_bytes());
        message.align(8);
        message.0.extend(body.0);
        message.0
    }

    /// What is needed of a message that came from the bus
    #[derive(Debug)]
    pub(super) struct Message {
        pub kind: u8,
        pub reply_serial: Option<u32>,
        pub member: Option<String>,
        pub signature: String,
        body: Vec<u8>,
        big_endian: bool,
    }

    impl Message {
        /// The body starts at a multiple of 8, so its alignment counts from its start
        pub fn body(&self) -> Reader<'_> {
            Reader {
                bytes: &self.body,
                pos: 0,
                big_endian: self.big_endian,
            }
        }
    }

    pub(super) fn read_message(bus: &mut impl Read) -> io::Result<Message> {
        let mut bytes = vec![0; 16];
        bus.read_exact(&mut bytes)?;
        let big_endian = match bytes[0] {
            b'l' => false,
            b'B' => true,
            _ => return Err(io::Error::other("not a D-Bus message")),
        };
        let number = |at: usize| {
            let four = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
            let value = match big_endian {
                true => u32::from_be_bytes(four),
                false => u32::from_le_bytes(four),
            };
            value as usize
        };
        let (body_length, fields_length) = (number(4), number(12));
        if body_length > 1 << 20 || fields_length > 1 << 16 {
            return Err(io::Error::other("a message too large"));
        }
        let header_end = (16 + fields_length).next_multiple_of(8);
        bytes.resize(header_end + body_length, 0);
        bus.read_exact(&mut bytes[16..])?;

        let mut message = Message {
            kind: bytes[1],
            reply_serial: None,
            member: None,
            signature: String::new(),
            body: bytes[header_end..].to_vec(),
            big_endian,
        };
        let mut fields = Reader {
            bytes: &bytes[..16 + fields_length],
            pos: 16,
            big_endian,
        };
        while fields.pos < 16 + fields_length {
            fields.align(8);
            let code = fields.u8()?;
            match (code, fields.signature()?.as_str()) {
                (5, "u") => message.reply_serial = Some(fields.u32()?),
                (_, "u") => _ = fields.u32()?,
                (3, "s") => message.member = Some(fields.string()?),
                (_, "s" | "o") => _ = fields.string()?,
                (8, "g") => message.signature = fields.signature()?,
                (_, "g") => _ = fields.signature()?,
                _ => return Err(io::Error::other("a header field of another type")),
            }
        }
        Ok(message)
    }

    /// Reads values with D-Bus alignment
    pub(super) struct Reader<'a> {
        bytes: &'a [u8],
        pos: usize,
        big_endian: bool,
    }

    impl Reader<'_> {
        fn align(&mut self, to: usize) {
            self.pos = self.pos.next_multiple_of(to);
        }

        fn take(&mut self, count: usize) -> io::Result<&[u8]> {
            let taken = self
                .bytes
                .get(self.pos..self.pos + count)
                .ok_or_else(|| io::Error::other("a message cut short"))?;
            self.pos += count;
            Ok(taken)
        }

        fn u8(&mut self) -> io::Result<u8> {
            Ok(self.take(1)?[0])
        }

        fn u32(&mut self) -> io::Result<u32> {
            self.align(4);
            let big_endian = self.big_endian;
            let b = self.take(4)?;
            let four = [b[0], b[1], b[2], b[3]];
            Ok(match big_endian {
                true => u32::from_be_bytes(four),
                false => u32::from_le_bytes(four),
            })
        }

        pub fn string(&mut self) -> io::Result<String> {
            let length = self.u32()? as usize;
            let text = String::from_utf8_lossy(self.take(length)?).into_owned();
            self.take(1)?;
            Ok(text)
        }

        fn signature(&mut self) -> io::Result<String> {
            let length = self.u8()? as usize;
            let text = String::from_utf8_lossy(self.take(length)?).into_owned();
            self.take(1)?;
            Ok(text)
        }

        /// A variant that holds a u32, also inside another variant as Read gives it
        pub fn variant_u32(&mut self) -> io::Result<u32> {
            for _ in 0..4 {
                match self.signature()?.as_str() {
                    "u" => return self.u32(),
                    "v" => continue,
                    other => return Err(io::Error::other(format!("a variant of {other}"))),
                }
            }
            Err(io::Error::other("variants too deep"))
        }
    }
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::portal::*;

    /// A message as the bus sends it: little or big endian, with these header fields
    /// (code, type, value) and this body
    fn message(big_endian: bool, kind: u8, fields: &[(u8, &str, &str)], body: &[u8]) -> Vec<u8> {
        let u32_bytes = |v: u32| match big_endian {
            true => v.to_be_bytes(),
            false => v.to_le_bytes(),
        };
        let mut m = vec![if big_endian { b'B' } else { b'l' }, kind, 0, 1];
        m.extend(u32_bytes(body.len() as u32));
        m.extend(u32_bytes(7));
        m.extend([0; 4]);
        for (code, kind, value) in fields {
            while m.len() % 8 != 0 {
                m.push(0);
            }
            m.extend([*code, 1, kind.as_bytes()[0], 0]);
            match *kind {
                "u" => m.extend(u32_bytes(value.parse().unwrap())),
                "g" => {
                    m.push(value.len() as u8);
                    m.extend(value.as_bytes());
                    m.push(0);
                }
                _ => {
                    m.extend(u32_bytes(value.len() as u32));
                    m.extend(value.as_bytes());
                    m.push(0);
                }
            }
        }
        let fields = u32_bytes((m.len() - 16) as u32);
        m[12..16].copy_from_slice(&fields);
        while m.len() % 8 != 0 {
            m.push(0);
        }
        m.extend(body);
        m
    }

    #[test]
    fn ci_a_method_call_is_read_back() {
        let call = method_call(
            3,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "ReadOne",
            &["org.freedesktop.appearance", "color-scheme"],
        );
        assert_eq!(&call[..4], b"l\x01\x00\x01");
        let read = read_message(&mut call.as_slice()).unwrap();
        assert_eq!(read.kind, 1);
        assert_eq!(read.member.as_deref(), Some("ReadOne"));
        assert_eq!(read.signature, "ss");
        let mut body = read.body();
        assert_eq!(body.string().unwrap(), "org.freedesktop.appearance");
        assert_eq!(body.string().unwrap(), "color-scheme");
        // Hello has no body and no signature
        let hello = method_call(
            1,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            &[],
        );
        assert_eq!(hello.len() % 8, 0);
        assert_eq!(read_message(&mut hello.as_slice()).unwrap().signature, "");
    }

    #[test]
    fn ci_the_answers_of_read_one_and_read() {
        // ReadOne: a variant holding the u32, here "prefer light"
        let body = [1, b'u', 0, 0, 2, 0, 0, 0];
        let reply = message(
            false,
            2,
            &[(5, "u", "3"), (8, "g", "v"), (7, "s", ":1.4")],
            &body,
        );
        let read = read_message(&mut reply.as_slice()).unwrap();
        assert_eq!((read.kind, read.reply_serial), (2, Some(3)));
        assert_eq!(read.body().variant_u32().unwrap(), 2);

        // Read of version 1 wraps it in another variant, here from a big endian bus
        let body = [1, b'v', 0, 1, b'u', 0, 0, 0, 0, 0, 0, 1];
        let reply = message(true, 2, &[(5, "u", "4"), (8, "g", "v")], &body);
        let read = read_message(&mut reply.as_slice()).unwrap();
        assert_eq!(read.reply_serial, Some(4));
        assert_eq!(read.body().variant_u32().unwrap(), 1);
    }

    #[test]
    fn ci_a_changed_color_scheme() {
        let mut body = Vec::new();
        for text in ["org.freedesktop.appearance", "color-scheme"] {
            while body.len() % 4 != 0 {
                body.push(0);
            }
            body.extend((text.len() as u32).to_le_bytes());
            body.extend(text.as_bytes());
            body.push(0);
        }
        body.extend([1, b'u', 0]);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        body.extend(1u32.to_le_bytes());
        let signal = message(
            false,
            4,
            &[
                (1, "o", "/org/freedesktop/portal/desktop"),
                (2, "s", "org.freedesktop.portal.Settings"),
                (3, "s", "SettingChanged"),
                (8, "g", "ssv"),
            ],
            &body,
        );
        let read = read_message(&mut signal.as_slice()).unwrap();
        assert_eq!(read.member.as_deref(), Some("SettingChanged"));
        assert_eq!(read.signature, "ssv");
        let mut body = read.body();
        assert_eq!(body.string().unwrap(), "org.freedesktop.appearance");
        assert_eq!(body.string().unwrap(), "color-scheme");
        assert_eq!(body.variant_u32().unwrap(), 1);
    }

    #[test]
    fn ci_broken_messages_are_errors() {
        let reply = message(
            false,
            2,
            &[(5, "u", "3"), (8, "g", "v")],
            &[1, b'u', 0, 0, 2, 0, 0, 0],
        );
        for cut in [0, 5, 16, reply.len() - 1] {
            assert!(read_message(&mut &reply[..cut]).is_err(), "cut at {cut}");
        }
        assert!(read_message(&mut &b"no message at all, not even close"[..]).is_err());
        // a string in the variant
        let reply = message(
            false,
            2,
            &[(5, "u", "3"), (8, "g", "v")],
            &[1, b's', 0, 0, 0, 0, 0, 0, 0],
        );
        assert!(
            read_message(&mut reply.as_slice())
                .unwrap()
                .body()
                .variant_u32()
                .is_err()
        );
        // variants in variants without end
        let reply = message(false, 2, &[(8, "g", "v")], &[1, b'v', 0].repeat(10));
        assert!(
            read_message(&mut reply.as_slice())
                .unwrap()
                .body()
                .variant_u32()
                .is_err()
        );
    }

    #[test]
    fn ci_bus_addresses() {
        assert_eq!(
            socket_of("unix:path=/run/user/1000/bus"),
            Some(Socket::Path("/run/user/1000/bus".into()))
        );
        assert_eq!(
            socket_of("unix:abstract=/tmp/dbus-AbC,guid=1234"),
            Some(Socket::Abstract(b"/tmp/dbus-AbC".to_vec()))
        );
        assert_eq!(
            socket_of("tcp:host=localhost,port=1;unix:path=/tmp/a%20b"),
            Some(Socket::Path("/tmp/a b".into()))
        );
        assert_eq!(socket_of("tcp:host=localhost,port=1"), None);
    }
}
