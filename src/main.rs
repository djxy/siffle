mod cli;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use env_logger::{Builder, Env};
use log::info;
use serde::{Serialize, Serializer};

use crate::cli::{Cli, ClientArgs, Commands};

fn format_duration<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i64(duration.as_micros() as i64)
}

#[derive(Serialize, Debug)]
struct Latency {
    #[serde(serialize_with = "format_duration")]
    min: Duration,
    #[serde(serialize_with = "format_duration")]
    p25: Duration,
    #[serde(serialize_with = "format_duration")]
    p50: Duration,
    #[serde(serialize_with = "format_duration")]
    p75: Duration,
    #[serde(serialize_with = "format_duration")]
    p90: Duration,
    #[serde(serialize_with = "format_duration")]
    p99: Duration,
    #[serde(serialize_with = "format_duration")]
    p99_9: Duration,
    #[serde(serialize_with = "format_duration")]
    p99_99: Duration,
    #[serde(serialize_with = "format_duration")]
    max: Duration,
}

/// The details between 2 instants
#[derive(Serialize, Debug)]
struct Interval {
    messages_sent: usize,
    messages_received: usize,
    latency: Latency,
}

#[derive(Serialize, Debug)]
struct Results {
    /// Details about all the test
    total: Interval,

    /// Details about each second of the test.
    /// intervals[0] -> 0s to 1s
    /// intervals[1] -> 1s to 2s
    intervals: Vec<Interval>,
}

fn start_server(bind_addr: SocketAddr) {
    thread::spawn(move || {
        let listener = TcpListener::bind(bind_addr).expect("Failed to bind TCP server socket.");
        info!("TCP server listening on {}", bind_addr);

        let mut buffer = [0u8; 1024];

        while let Ok((mut stream, src)) = listener.accept() {
            info!("New TCP connection from {}", src);
            stream.set_nodelay(true).unwrap();

            loop {
                match stream.read(&mut buffer) {
                    Ok(length) => {
                        if length == 0 {
                            break;
                        }

                        if let Err(_) = stream.write(&buffer[..length]) {
                            break;
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            drop(stream);
            info!("TCP connection from {} disconnected.", src);
        }
    });

    let socket = UdpSocket::bind(bind_addr).expect("Failed to bind UDP server socket.");
    info!("UDP server listening on {}", bind_addr);
    let mut buf = [0u8; 1024];

    loop {
        if let Ok((len, src)) = socket.recv_from(&mut buf) {
            let _ = socket.send_to(&buf[..len], src);
        }
    }
}

fn start_udp_latency_test(server_addr: SocketAddr, args: ClientArgs) {
    let total_messages = args.mps * args.duration;

    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP client socket.");

    socket
        .connect(server_addr)
        .expect("Failed to connect to target");

    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    let receiver_socket = socket.try_clone().expect("Failed to clone socket");

    let receiver_handle = thread::spawn(move || {
        let mut ids_received_at: Vec<Option<Instant>> = vec![None; total_messages];
        let mut buf = [0u8; 1024];

        loop {
            match receiver_socket.recv(&mut buf) {
                Ok(_) => {
                    let received_at = Instant::now();
                    let id = u32::from_be_bytes(buf[..4].try_into().unwrap());

                    if id == u32::MAX {
                        continue;
                    }

                    ids_received_at[id as usize] = Some(received_at);
                }
                Err(_) => {
                    return ids_received_at;
                }
            }
        }
    });

    let mut ids_sent_at: Vec<Instant> = Vec::with_capacity(total_messages);
    let mut id_counter: u32 = 0;
    let mut payload = [0u8; 4];
    let test_duration = Duration::from_secs(args.duration as u64);
    let sleep_duration = Duration::from_millis(1);
    let start = Instant::now();

    loop {
        let progress = start.elapsed().as_micros() as f64 / test_duration.as_micros() as f64;
        let expected_messages_sent =
            total_messages.min((total_messages as f64 * progress) as usize);
        let messages_to_send = expected_messages_sent - ids_sent_at.len();

        for _ in 0..messages_to_send {
            payload.copy_from_slice(&id_counter.to_be_bytes());

            let send_time = Instant::now();
            let _ = socket.send(&payload);
            ids_sent_at.push(send_time);
            id_counter += 1;
        }

        thread::sleep(sleep_duration);

        if start.elapsed() > test_duration {
            break;
        }
    }

    thread::sleep(Duration::from_secs(1));

    drop(socket);

    print_results(
        ids_sent_at,
        receiver_handle.join().expect("Read thread panicked"),
        args,
    );
}

fn start_tcp_latency_test(server_addr: SocketAddr, args: ClientArgs) {
    let total_messages = args.mps * args.duration;
    let stream = TcpStream::connect(server_addr).expect("Failed to bind TCP client socket.");

    stream.set_nodelay(true).unwrap();

    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    let mut read_stream = stream.try_clone().unwrap();
    let mut write_stream = stream;

    let read_handle = thread::spawn(move || {
        let mut ids_received_at: Vec<Option<Instant>> = vec![None; total_messages];
        let mut buf = [0u8; 4];

        loop {
            match read_stream.read_exact(&mut buf) {
                Ok(_) => {
                    let received_at = Instant::now();
                    let id = u32::from_be_bytes(buf.try_into().unwrap());

                    if id == u32::MAX {
                        continue;
                    }

                    ids_received_at[id as usize] = Some(received_at);
                }
                Err(_) => {
                    return ids_received_at;
                }
            }
        }
    });

    let mut ids_sent_at: Vec<Instant> = Vec::with_capacity(total_messages);
    let mut id_counter: u32 = 0;
    let mut payload = [0u8; 4];
    let mps = args.mps as u128;
    let test_duration = Duration::from_secs(args.duration as u64);
    let sleep_duration = Duration::from_millis(1);
    let start = Instant::now();

    loop {
        let expected_messages_sent = ((start.elapsed().as_nanos() * mps) / 1_000_000_000) as usize;
        let messages_to_send = expected_messages_sent
            .min(total_messages)
            .saturating_sub(ids_sent_at.len());

        for _ in 0..messages_to_send {
            payload.copy_from_slice(&id_counter.to_be_bytes());

            let send_time = Instant::now();
            let _ = write_stream.write_all(&payload);
            ids_sent_at.push(send_time);
            id_counter += 1;
        }

        thread::sleep(sleep_duration);

        if start.elapsed() > test_duration {
            break;
        }
    }

    write_stream.shutdown(std::net::Shutdown::Write).unwrap();

    thread::sleep(Duration::from_secs(1));

    print_results(
        ids_sent_at,
        read_handle.join().expect("Read thread panicked"),
        args,
    );
}

fn print_results(
    messages_sent_at: Vec<Instant>,
    messages_received_at: Vec<Option<Instant>>,
    args: ClientArgs,
) {
    let mut total_messages_received = 0;
    let mut total_messages_sent = 0;
    let mut total_rtts: Vec<Duration> = Vec::new();
    let mut intervals: Vec<Interval> = Vec::with_capacity(args.duration);

    for second in 0..args.duration {
        let mut messages_sent = 0;
        let mut messages_received = 0;
        let mut rtts = Vec::with_capacity(args.mps);

        for i in 0..args.mps {
            let id = (second * args.mps) + i;

            if let Some(sent_at) = messages_sent_at.get(id) {
                messages_sent += 1;

                if let Some(received_at) = messages_received_at[id] {
                    messages_received += 1;
                    rtts.push(received_at.duration_since(*sent_at));
                }
            }
        }

        rtts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        intervals.push(Interval {
            messages_sent,
            messages_received,
            latency: Latency {
                min: *rtts.first().unwrap(),
                p25: get_rtt_percentile(0.25, &rtts),
                p50: get_rtt_percentile(0.50, &rtts),
                p75: get_rtt_percentile(0.75, &rtts),
                p90: get_rtt_percentile(0.90, &rtts),
                p99: get_rtt_percentile(0.99, &rtts),
                p99_9: get_rtt_percentile(0.999, &rtts),
                p99_99: get_rtt_percentile(0.9999, &rtts),
                max: *rtts.last().unwrap(),
            },
        });

        total_messages_sent += messages_sent;
        total_messages_received += messages_received;
        total_rtts.append(&mut rtts);
    }

    total_rtts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let results = Results {
        total: Interval {
            messages_received: total_messages_received,
            messages_sent: total_messages_sent,
            latency: Latency {
                min: *total_rtts.first().unwrap(),
                p25: get_rtt_percentile(0.25, &total_rtts),
                p50: get_rtt_percentile(0.50, &total_rtts),
                p75: get_rtt_percentile(0.75, &total_rtts),
                p90: get_rtt_percentile(0.90, &total_rtts),
                p99: get_rtt_percentile(0.99, &total_rtts),
                p99_9: get_rtt_percentile(0.999, &total_rtts),
                p99_99: get_rtt_percentile(0.9999, &total_rtts),
                max: *total_rtts.last().unwrap(),
            },
        },
        intervals,
    };

    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}

fn get_rtt_percentile(p: f64, rtts: &Vec<Duration>) -> Duration {
    let idx = ((rtts.len() as f64 * p).floor() as usize).min(rtts.len() - 1);
    rtts[idx]
}

fn main() {
    Builder::from_env(Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Server(args) => {
            start_server(SocketAddr::new(args.ip, args.port));
        }
        Commands::Udp(args) => {
            start_udp_latency_test(
                format!("{}:{}", args.server, args.port)
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap(),
                args,
            );
        }
        Commands::Tcp(args) => {
            start_tcp_latency_test(
                format!("{}:{}", args.server, args.port)
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap(),
                args,
            );
        }
    }
}
