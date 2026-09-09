// The included modules carry more than this test exercises.
#![allow(dead_code)]

//! Proves the reply stream is consumed as it arrives, over a real socket.
//!
//! The unit tests in `stream.rs` feed lines in directly, which cannot show the
//! property that actually matters to a person watching the window: that a
//! fragment reaches the caller *while the service is still writing*, not when
//! the response finally closes. This test holds the connection open between
//! fragments and asserts each one is seen before the next is sent.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// The crate is a binary, so the modules under test are included directly.
#[path = "../src/protocol.rs"]
mod protocol;
#[path = "../src/session.rs"]
mod session;
#[path = "../src/stream.rs"]
mod stream;

use stream::TurnStream;

const SCHEMA: &str = "apocky.apocrypha-chat-stream.v1";

fn delta(text: &str) -> String {
    format!("{{\"schema_version\":\"{SCHEMA}\",\"type\":\"delta\",\"text\":\"{text}\"}}\n")
}

fn completed(session: &str, request: &str, text: &str) -> String {
    format!(
        "{{\"schema_version\":\"{SCHEMA}\",\"type\":\"completed\",\"result\":{{\
\"outcome\":\"completed\",\"session_id\":\"{session}\",\"request_id\":\"{request}\",\
\"text\":\"{text}\",\"model_id\":\"apocrypha-runtime\",\"response_digest\":\"{}\"}}}}\n",
        "c".repeat(64)
    )
}

/// Serves one NDJSON response, pausing between fragments.
fn serve(session: String, request: String, fragments: Vec<String>, gap: Duration) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let address = listener.local_addr().expect("an address").to_string();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("a connection");
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson; charset=utf-8\r\nConnection: close\r\n\r\n")
            .expect("headers");
        socket.flush().expect("flush headers");
        for fragment in &fragments {
            socket.write_all(delta(fragment).as_bytes()).expect("a fragment");
            socket.flush().expect("flush fragment");
            thread::sleep(gap);
        }
        let whole: String = fragments.concat();
        socket
            .write_all(completed(&session, &request, &whole).as_bytes())
            .expect("the terminal event");
        socket.flush().expect("flush terminal");
    });
    (address, handle)
}

/// Reads the body the way `ApiClient::stream_turn` does.
fn drive(address: &str, on_delta: &mut dyn FnMut(&str)) -> TurnStream {
    let socket = TcpStream::connect(address).expect("a connection");
    let mut reader = BufReader::new(socket);
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("a header line");
        if line == "\r\n" || line.is_empty() {
            break;
        }
        headers.push_str(&line);
    }
    assert!(
        headers.to_ascii_lowercase().contains("application/x-ndjson"),
        "the fixture must serve the streaming content type",
    );
    let mut turn = TurnStream::new();
    for line in reader.lines() {
        let line = line.expect("a body line");
        if let Some(fragment) = turn.accept(&line).expect("a valid event") {
            on_delta(&fragment);
        }
    }
    turn
}

#[test]
fn fragments_reach_the_window_while_the_service_is_still_writing() {
    let session = protocol::new_id();
    let request = protocol::new_id();
    let fragments = vec!["The ".to_string(), "reply ".to_string(), "arrives ".to_string(), "in parts.".to_string()];
    let gap = Duration::from_millis(120);
    let (address, server) = serve(session.clone(), request.clone(), fragments.clone(), gap);

    let (sender, receiver) = mpsc::channel();
    let started = Instant::now();
    let client = thread::spawn(move || {
        let mut seen = |fragment: &str| {
            sender.send((fragment.to_string(), started.elapsed())).expect("a receiver");
        };
        drive(&address, &mut seen)
    });

    // Each fragment must be observed before the service has sent the next one.
    let mut observed = Vec::new();
    for index in 0..fragments.len() {
        let (fragment, at) = receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("fragment {index} never arrived"));
        assert!(
            at < gap * (index as u32 + 2),
            "fragment {index} arrived at {at:?}, which is too late to have been streamed",
        );
        observed.push(fragment);
    }
    assert_eq!(observed, fragments, "every fragment arrives, in order");

    let turn = client.join().expect("the client thread");
    server.join().expect("the server thread");
    assert!(turn.is_finished());
    assert_eq!(turn.text(), "The reply arrives in parts.");
    assert_eq!(turn.finish(&session, &request).unwrap(), "The reply arrives in parts.");
    assert!(
        started.elapsed() >= gap * fragments.len() as u32,
        "the fixture must actually have held the connection open",
    );
}

#[test]
fn a_connection_that_dies_mid_reply_is_not_reported_as_an_answer() {
    let session = protocol::new_id();
    let request = protocol::new_id();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let address = listener.local_addr().expect("an address").to_string();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("a connection");
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n")
            .expect("headers");
        socket.write_all(delta("half of a th").as_bytes()).expect("a fragment");
        socket.flush().expect("flush");
        // The service goes away without ever finishing the turn.
    });

    let mut seen = String::new();
    let turn = drive(&address, &mut |fragment| seen.push_str(fragment));
    server.join().expect("the server thread");

    assert_eq!(seen, "half of a th", "what was written is still what was shown");
    assert!(!turn.is_finished());
    let error = turn.finish(&session, &request).unwrap_err().to_string();
    assert!(
        error.contains("ended before the reply was finished"),
        "an unfinished stream must not be presented as a reply: {error}",
    );
}
