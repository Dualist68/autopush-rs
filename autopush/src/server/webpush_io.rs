//! I/O wrapper created through `Dispatch`
//!
//! Most I/O happens through just raw TCP sockets, but at the beginning of a
//! request we'll take a look at the headers and figure out where to route it.
//! After that, for tungstenite the websocket library, we'll want to replay the
//! data we already read as there's no ability to pass this in currently. That
//! means we'll parse headers twice, but alas!

use std::backtrace;
use std::io::{self, Read, Write};

use bytes::BytesMut;
use futures::Poll;
use tokio_core::net::TcpStream;
use tokio_io::{AsyncRead, AsyncWrite};

use crate::server::tls::MaybeTlsStream;

pub struct WebpushIo {
    tcp: MaybeTlsStream<TcpStream>,
    header_to_read: Option<BytesMut>,
}

impl WebpushIo {
    pub fn new(tcp: MaybeTlsStream<TcpStream>, header: BytesMut) -> Self {
        Self {
            tcp,
            header_to_read: Some(header),
        }
    }
}

impl Read for WebpushIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Start off by replaying the bytes already read, and after that just
        // delegate everything to the internal `TcpStream`
        if let Some(ref mut header) = self.header_to_read {
            let n = (&header[..]).read(buf)?;
            header.split_to(n);
            if buf.is_empty() || n > 0 {
                return Ok(n);
            }
        }
        self.header_to_read = None;
        let res = self.tcp.read(buf);
        if res.is_err() {
            if let Some(e) = res.as_ref().err() {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    // quietly report the error. The socket closed early.
                    trace!("🢤 Detected WouldBlock, connection terminated abruptly");
                } else {
                    // report the backtrace because it can be difficult to determine
                    // what the caller is.
                    warn!("🢤 ERR: {:?}\n{:?}", &e, backtrace::Backtrace::capture());
                }
            }
        } else {
            trace!("🢤 {:?}", &res);
        }
        res
    }
}

// All `write` calls are routed through the `TcpStream` instance directly as we
// don't buffer this at all.
impl Write for WebpushIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        trace!("🢧 {:?}", String::from_utf8_lossy(buf));
        self.tcp.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        trace!("🚽");
        self.tcp.flush()
    }
}

impl AsyncRead for WebpushIo {}

impl AsyncWrite for WebpushIo {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        AsyncWrite::shutdown(&mut self.tcp)
    }
}
