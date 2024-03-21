use std::{
    collections::HashSet,
    io::{self, Read, Result, Write},
    net::TcpStream,
};

use ffi::Event;
use poll::Poll;

mod ffi;
mod poll;

/// Not the entire url, but everything after the domain addr
/// i.e. http://localhost/1000/hello => /1000/hello
fn get_req(path: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\n\
             Host: localhost\r\n\
             Connection: close\r\n\
             \r\n"
    )
}
fn handle_events(
    events: &[Event],
    streams: &mut [TcpStream],
    handled: &mut HashSet<usize>,
) -> Result<usize> {
    let mut handled_events = 0;
    for event in events {
        // token identifies which TcpStream we received an event for
        let index = event.token();
        let mut buffer = vec![0u8; 4096];

        loop {
            // Loop on TcpStream which has got the Server Response 
            // Save the Response using Read inside buffer, 
            // Response could be larger than buffer size, 
            // so it can loop multiple times and drain the buffer fully (Edge triggered) 
            // until there is no data in buffer (n=0) 
            // then break the loop
            match streams[index].read(&mut buffer) {
                Ok(n) if n == 0 => {
                    // `insert` returns false if the value already existed in the set. We
                    // handle it here since we must be sure that the TcpStream is fully
                    // drained due to using edge triggered epoll.
                    if !handled.insert(index) {
                        break;
                    }
                    handled_events += 1;
                    break;
                }
                Ok(n) => {
                    let txt = String::from_utf8_lossy(&buffer[..n]);

                    println!("RECEIVED: {:?}", event);
                    println!("{txt}\n------\n");
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => break,
                Err(e) => return Err(e),
            }
        }
    }

    Ok(handled_events)
}

fn main() -> Result<()> {
    let mut poll = Poll::new()?;
    let n_events = 5;

    let mut streams = vec![];
    let addr = "localhost:8080";

    for i in 0..n_events {
        let delay = (n_events - i) * 1000;
        let url_path = format!("/{delay}/request-{i}");
        let request = get_req(&url_path);
        let mut stream = std::net::TcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;

        stream.write_all(request.as_bytes())?;
        // NB! Token is equal to index in Vec
        poll.registry()
            .register(&stream, i, ffi::EPOLLIN | ffi::EPOLLET)?;

        streams.push(stream);
    }

    // ABOVE WILL SEND THE REQUEST TO SERVER

    // BELOW WILL HANDLE THE SERVER RESPONSE

    let mut handled_ids = HashSet::new();
    let mut handled_events = 0;

    while handled_events < n_events {
        let mut events = Vec::with_capacity(10);
        // this will tell the OS to park our thread and wake us up when an event has occurred.
        poll.poll(&mut events, None)?;

        if events.is_empty() {
            println!("TIMEOUT (OR SPURIOUS EVENT NOTIFICATION)");
            continue;
        }
        // Once epollwait has notified of the server response, we handle the event
        handled_events += handle_events(&events, &mut streams, &mut handled_ids)?;
    }

    println!("FINISHED");
    Ok(())
}
