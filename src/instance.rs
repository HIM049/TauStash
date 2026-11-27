use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, Listener, ListenerOptions, Name, Stream, prelude::*,
};
use std::io::{self, BufRead, BufReader, Write};
use tokio::sync::mpsc::UnboundedSender;

const SOCKET_NAME: &str = "com.him049.taustash.sock";

pub enum Instance {
    /// First instance, owning the listener.
    Primary(Listener),
    /// Another instance is running and has received this launch's arguments.
    Secondary,
}

fn socket_name() -> io::Result<Name<'static>> {
    if GenericNamespaced::is_supported() {
        SOCKET_NAME.to_ns_name::<GenericNamespaced>()
    } else {
        crate::paths::data_dir()
            .join(SOCKET_NAME)
            .to_fs_name::<GenericFilePath>()
    }
}

/// Check and bind instance
pub fn acquire() -> io::Result<Instance> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // A second round covers another process binding between our connect and bind.
    for _ in 0..2 {
        if let Ok(mut stream) = Stream::connect(socket_name()?) {
            let mut line = serde_json::to_string(&args)?;
            line.push('\n');
            stream.write_all(line.as_bytes())?;
            return Ok(Instance::Secondary);
        }

        match ListenerOptions::new()
            .name(socket_name()?)
            .try_overwrite(true)
            .create_sync()
        {
            Ok(listener) => return Ok(Instance::Primary(listener)),
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::ErrorKind::AddrInUse.into())
}

pub fn listen(listener: Listener, tx: UnboundedSender<Vec<String>>) {
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let conn = match conn {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::warn!("Instance connection failed: {:#}", e);
                    continue;
                }
            };
            let mut line = String::new();
            if BufReader::new(conn).read_line(&mut line).is_err() {
                continue;
            }
            let args = serde_json::from_str(&line).unwrap_or_default();
            if tx.send(args).is_err() {
                break;
            }
        }
    });
}
