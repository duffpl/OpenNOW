//! Drains a UDP socket on a thread of its own so the kernel receive buffer never has to absorb
//! the receive loop's processing time.
//!
//! Where an unprivileged process cannot enlarge `SO_RCVBUF` (LG webOS caps it at 512 KiB), a
//! keyframe burst arriving while the loop is busy decrypting and assembling overflows the kernel
//! buffer and the packets are gone. Queued here, a burst only costs memory.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// About 12 MB of 1.5 KB datagrams: seconds of video at any bitrate the service sends.
const CAPACITY: usize = 8_192;

type Datagram = io::Result<(Vec<u8>, SocketAddr)>;

pub(crate) struct UdpPump {
    datagrams: Receiver<Datagram>,
    poll: Duration,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    max_gap_us: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl UdpPump {
    /// `socket` must carry a read timeout: it bounds how long the pump takes to notice `Drop`.
    pub(crate) fn spawn(socket: &UdpSocket, name: &str, poll: Duration) -> io::Result<Self> {
        let socket = socket.try_clone()?;
        let (sender, datagrams) = sync_channel(CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let max_gap_us = Arc::new(AtomicU64::new(0));
        let thread = thread::Builder::new().name(name.to_owned()).spawn({
            let stop = Arc::clone(&stop);
            let dropped = Arc::clone(&dropped);
            let max_gap_us = Arc::clone(&max_gap_us);
            move || pump(&socket, &sender, &stop, &dropped, &max_gap_us)
        })?;
        Ok(Self { datagrams, poll, stop, dropped, max_gap_us, thread: Some(thread) })
    }

    /// `UdpSocket::recv_from` with the socket's own semantics: an idle `poll` interval is
    /// `WouldBlock`, and socket errors arrive in order with the datagrams around them.
    pub(crate) fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        match self.datagrams.recv_timeout(self.poll) {
            Ok(Ok((datagram, source))) => {
                let length = datagram.len().min(buffer.len());
                buffer[..length].copy_from_slice(&datagram[..length]);
                Ok((length, source))
            }
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => Err(io::ErrorKind::WouldBlock.into()),
            Err(RecvTimeoutError::Disconnected) => {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "UDP pump thread exited"))
            }
        }
    }

    /// Datagrams discarded because the queue was full.
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Longest silence between two datagrams arriving at the socket since the last call. Measured
    /// on the pump thread, so it is the network's pause, not the receive loop's.
    pub(crate) fn take_max_gap(&self) -> Duration {
        Duration::from_micros(self.max_gap_us.swap(0, Ordering::Relaxed))
    }
}

impl Drop for UdpPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn pump(
    socket: &UdpSocket,
    sender: &SyncSender<Datagram>,
    stop: &AtomicBool,
    dropped: &AtomicU64,
    max_gap_us: &AtomicU64,
) {
    let mut buffer = vec![0_u8; 65_536];
    let mut last_arrival: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        let datagram = match socket.recv_from(&mut buffer) {
            Ok((length, source)) => {
                let now = Instant::now();
                if let Some(last) = last_arrival {
                    let gap = u64::try_from((now - last).as_micros()).unwrap_or(u64::MAX);
                    max_gap_us.fetch_max(gap, Ordering::Relaxed);
                }
                last_arrival = Some(now);
                Ok((buffer[..length].to_vec(), source))
            }
            Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {
                continue;
            }
            Err(error) => Err(error),
        };
        match sender.try_send(datagram) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (UdpSocket, UdpSocket) {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(receiver.local_addr().unwrap()).unwrap();
        (receiver, sender)
    }

    #[test]
    fn delivers_datagrams_in_order_with_their_source() {
        let (receiver, sender) = pair();
        let pump = UdpPump::spawn(&receiver, "test-pump", Duration::from_millis(500)).unwrap();
        for byte in 0..3_u8 {
            sender.send(&[byte; 4]).unwrap();
        }
        let mut buffer = [0_u8; 16];
        for byte in 0..3_u8 {
            let (length, source) = pump.recv_from(&mut buffer).unwrap();
            assert_eq!(&buffer[..length], &[byte; 4]);
            assert_eq!(source, sender.local_addr().unwrap());
        }
        assert_eq!(pump.dropped(), 0);
    }

    #[test]
    fn idle_interval_reads_as_would_block_and_drop_stops_the_thread() {
        let (receiver, _sender) = pair();
        let pump = UdpPump::spawn(&receiver, "test-pump", Duration::from_millis(10)).unwrap();
        let error = pump.recv_from(&mut [0_u8; 16]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        drop(pump);
    }
}
