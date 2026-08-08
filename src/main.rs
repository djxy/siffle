mod cli;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use env_logger::{Builder, Env};
use log::{error, info};

use crate::cli::{Cli, ClientArgs, Commands};

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

    info!("Protocol:                UDP");
    info!("Server:                  {}", server_addr);
    info!("Duration:                {}s", args.duration);
    info!("Messages per second:     {}", args.mps);
    info!("Total messages expected: {}", total_messages);

    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP client socket.");

    socket
        .connect(server_addr)
        .expect("Failed to connect to target");

    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    let mut payload = [0u8; 4];

    payload.copy_from_slice(&u32::MAX.to_be_bytes());

    let warmup_duration = Duration::from_millis(400);
    let warmup_start = Instant::now();

    info!("Starting warmup");

    loop {
        let _ = socket.send(&payload);
        let _ = socket.recv(&mut payload);

        if warmup_start.elapsed() >= warmup_duration {
            break;
        }
    }

    info!("Warmup duration: {:?}", warmup_start.elapsed());

    thread::sleep(Duration::from_millis(100));

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
        receiver_handle.join().expect("Read thread panicked"),
        ids_sent_at,
    );
}

fn start_tcp_latency_test(server_addr: SocketAddr, args: ClientArgs) {
    let total_messages = args.mps * args.duration;

    info!("Protocol:                TCP");
    info!("Server:                  {}", server_addr);
    info!("Duration:                {}s", args.duration);
    info!("Messages per second:     {}", args.mps);
    info!("Total messages expected: {}", total_messages);

    let mut stream = TcpStream::connect(server_addr).expect("Failed to bind TCP client socket.");

    stream.set_nodelay(true).unwrap();

    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    let mut payload = [0u8; 4];

    payload.copy_from_slice(&u32::MAX.to_be_bytes());

    let warmup_duration = Duration::from_millis(400);
    let warmup_start = Instant::now();

    info!("Starting warmup");

    loop {
        let _ = stream.write(&payload);
        let _ = stream.read_exact(&mut payload);

        if warmup_start.elapsed() >= warmup_duration {
            break;
        }
    }

    info!("Warmup duration: {:?}", warmup_start.elapsed());

    thread::sleep(Duration::from_millis(100));

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
        read_handle.join().expect("Read thread panicked"),
        ids_sent_at,
    );
}

fn print_results(ids_received_at: Vec<Option<Instant>>, ids_sent_at: Vec<Instant>) {
    let mut messages_loss = 0;
    let mut rtt: Vec<Duration> = Vec::new();

    for id in 0..ids_sent_at.len() {
        let sent_at = ids_sent_at.get(id).unwrap();

        if let Some(received_at) = ids_received_at[id] {
            rtt.push(received_at.duration_since(*sent_at));
        } else {
            messages_loss += 1;
        }
    }

    info!("Messages sent:     {}", ids_sent_at.len());
    info!("Messages received: {}", rtt.len());
    info!("Messages loss:     {}", messages_loss);

    if rtt.is_empty() {
        error!("No responses received. Target may be unreachable or dropping all packets.");
        return;
    }

    rtt.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let calc_percentile = |p: f64| -> &Duration {
        let idx = ((rtt.len() as f64 * p).floor() as usize).min(rtt.len() - 1);
        &rtt[idx]
    };

    info!("--- Latency Results ---");
    info!("Min:    {:?}", rtt.first().unwrap());
    info!("p25.00: {:?}", calc_percentile(0.25));
    info!("p50.00: {:?}", calc_percentile(0.50));
    info!("p75.00: {:?}", calc_percentile(0.75));
    info!("p90.00: {:?}", calc_percentile(0.90));
    info!("p99.00: {:?}", calc_percentile(0.99));
    info!("p99.90: {:?}", calc_percentile(0.999));
    info!("p99.99: {:?}", calc_percentile(0.9999));
    info!("Max:    {:?}", rtt.last().unwrap());
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
