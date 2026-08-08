<div align="center">
        
# Siffle

[![License: MIT](https://img.shields.io/github/license/djxy/siffle)](https://opensource.org/licenses/MIT)

</div>

Siffle is a Rust-based CLI tool measuring network latency over UDP and TCP. It runs a server that echoes all UDP datagrams and TCP streams back to the source, allowing the client to measure round-trip latency.

While originally developed to test [Siffleux](https://github.com/djxy/siffleux) TCP/UDP ingresses and egresses, it can measure latency across any network.

---

## How to Use

`siffle` is a single binary operates using a client-server model. Launch the echo server and then execute the client to test UDP or TCP latency.

### 1. Start the Echo Server

Run the server to echo incoming UDP datagrams and TCP streams.

```bash
# Listen on all interfaces (0.0.0.0) on default port 5678
siffle server

# Bind to a specific IP address and port
siffle server --ip 127.0.0.1 --port 8080
```

### 2. Run Latency Tests

#### Measure UDP Latency

```bash
# Run a 30 seconds UDP test with default settings (port 5678, 1000 mps)
siffle udp --server 127.0.0.1

# Run a 60 seconds UDP test sending ~5000 messages/sec on port 8080
siffle udp -s 127.0.0.1 -p 8080 -t 60 --mps 5000
```

#### Measure TCP Latency

```bash
# Run a 30 seconds TCP test with default settings (port 5678, 1000 mps)
siffle tcp --server 127.0.0.1

# Run a 15 seconds TCP test sending ~2000 messages/sec on port 8000
siffle tcp -s 127.0.0.1 -p 8000 -t 15 --mps 2000
```

## CLI Reference

### `siffle server`

Start the TCP and UDP echo servers.

| Option | Short | Description | Default |
| :--- | :--- | :--- | :--- |
| `--ip <IP>` | `-i` | IP address the TCP and UDP echo servers will listen on. | `0.0.0.0` |
| `--port <PORT>` | `-p` | Port the TCP and UDP echo servers will listen on. | `5678` |

### `siffle udp`

Start testing latency over UDP.

| Option | Short | Description | Default |
| :--- | :--- | :--- | :--- |
| `--server <SERVER>` | `-s` | **[Required]** IP address or hostname of the echo server. | |
| `--port <PORT>` | `-p` | Port of the echo server. | `5678` |
| `--duration <SECONDS>` | `-t` | Duration in seconds for the test. | `30` |
| `--mps <COUNT>` | | Target messages per second to send (approximate). | `1000` |

### `siffle tcp`

Start testing latency over TCP.

| Option | Short | Description | Default |
| :--- | :--- | :--- | :--- |
| `--server <SERVER>` | `-s` | **[Required]** IP address or hostname of the echo server. | |
| `--port <PORT>` | `-p` | Port of the echo server. | `5678` |
| `--duration <SECONDS>` | `-t` | Duration in seconds for the test. | `30` |
| `--mps <COUNT>` | | Target messages per second to send (approximate). | `1000` |
