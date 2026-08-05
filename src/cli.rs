use std::net::{IpAddr, Ipv4Addr};

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "siffle", version, about = "Measure TCP and UDP latency.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, short = 'v', default_value_t = false)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the TCP and UDP echo servers
    Server(ServerAgrs),
    /// Start testing latency over UDP
    Udp(ClientArgs),
    /// Start testing latency over TCP
    Tcp(ClientArgs),
}

#[derive(Args)]
pub struct ServerAgrs {
    /// IP address the TCP and UDP echo servers will listen on
    #[arg(long, default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    pub ip: IpAddr,

    /// Port the TCP and UDP echo servers will listen on
    #[arg(long, short = 'p', default_value_t = 3001)]
    pub port: u16,
}

#[derive(Args)]
pub struct ClientArgs {
    /// IP address or the hostname of the UDP/TCP echo server
    #[arg(long, short = 's')]
    pub server: String,

    /// Port of the UDP/TCP echo server
    #[arg(long, short = 'p', default_value_t = 3001)]
    pub port: u16,

    /// Duration in seconds the latency test should be
    #[arg(long, short = 't', default_value_t = 30)]
    pub duration: usize,

    /// The number of messages per second to send
    /// It isn't a guaranteed number, just an approximation
    #[arg(long, default_value_t = 1000)]
    pub mps: usize,
}
