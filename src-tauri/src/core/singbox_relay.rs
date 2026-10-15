use anyhow::{Context as _, Result};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    sync::{oneshot, watch},
    task::{JoinHandle, JoinSet},
    time::{Instant, sleep_until},
};

const SOCKS_VERSION: u8 = 5;
const AUTH_VERSION: u8 = 1;
const CONNECT: u8 = 1;
const MAX_RELAY_CONNECTIONS: usize = 64;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const DEAD_CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const TCP_KEEPALIVE_TIME: Duration = Duration::from_secs(30);
const TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct RelayEndpoint {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

pub struct AuthenticatedRelay {
    endpoint: RelayEndpoint,
    activity: RelayActivity,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct RelayActivity {
    active_connections: Arc<AtomicUsize>,
    last_activity: Arc<tokio::sync::Mutex<Instant>>,
}

struct ActiveConnection(Arc<AtomicUsize>);

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl RelayActivity {
    fn new() -> Self {
        Self {
            active_connections: Arc::new(AtomicUsize::new(0)),
            last_activity: Arc::new(tokio::sync::Mutex::new(Instant::now())),
        }
    }

    pub async fn idle_for(&self, duration: Duration) -> bool {
        self.active_connections.load(Ordering::Acquire) == 0 && self.last_activity.lock().await.elapsed() >= duration
    }

    async fn touched(&self) {
        *self.last_activity.lock().await = Instant::now();
    }
}

impl AuthenticatedRelay {
    pub async fn start(upstream: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .context("failed to bind the sing-box relay to loopback")?;
        let port = listener.local_addr()?.port();
        let password = session_token()?;
        let endpoint = RelayEndpoint {
            host: "127.0.0.1".into(),
            port,
            username: "clash5".into(),
            password,
        };
        let expected_password = endpoint.password.clone();
        let activity = RelayActivity::new();
        let task_activity = activity.clone();
        let (stop, mut stopped) = oneshot::channel();
        let permits = Arc::new(tokio::sync::Semaphore::new(MAX_RELAY_CONNECTIONS));
        let task = tokio::spawn(async move {
            let mut clients = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    _ = clients.join_next(), if !clients.is_empty() => {}
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                                continue;
                            };
                            let password = expected_password.clone();
                            task_activity.active_connections.fetch_add(1, Ordering::AcqRel);
                            task_activity.touched().await;
                            let activity = task_activity.clone();
                            clients.spawn(async move {
                                let _permit = permit;
                                let _active = ActiveConnection(Arc::clone(&activity.active_connections));
                                let _ = serve_client(stream, upstream, b"clash5", password.as_bytes()).await;
                                activity.touched().await;
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        });
        Ok(Self {
            endpoint,
            activity,
            stop: Some(stop),
            task: Some(task),
        })
    }

    pub const fn endpoint(&self) -> &RelayEndpoint {
        &self.endpoint
    }

    pub fn activity(&self) -> RelayActivity {
        self.activity.clone()
    }

    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for AuthenticatedRelay {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn session_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("failed to generate the sing-box relay token")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

async fn serve_client(mut client: TcpStream, upstream: SocketAddr, username: &[u8], password: &[u8]) -> Result<()> {
    authenticate_client(&mut client, password).await?;
    let (request, address, domain_length) = read_connect_request(&mut client).await?;
    forward_connect(
        &mut client,
        upstream,
        username,
        password,
        &request,
        &address,
        domain_length,
    )
    .await
}

async fn authenticate_client(client: &mut TcpStream, password: &[u8]) -> Result<()> {
    let mut greeting = [0_u8; 2];
    read_handshake(client, &mut greeting).await?;
    if greeting[0] != SOCKS_VERSION {
        anyhow::bail!("unsupported relay protocol");
    }
    let mut methods = vec![0_u8; greeting[1] as usize];
    read_handshake(client, &mut methods).await?;
    if !methods.contains(&2) {
        client.write_all(&[SOCKS_VERSION, 0xff]).await?;
        anyhow::bail!("relay authentication is required");
    }
    client.write_all(&[SOCKS_VERSION, 2]).await?;

    let mut auth = [0_u8; 2];
    read_handshake(client, &mut auth).await?;
    if auth[0] != AUTH_VERSION {
        client.write_all(&[AUTH_VERSION, 1]).await?;
        anyhow::bail!("invalid relay authentication");
    }
    let mut username = vec![0_u8; auth[1] as usize];
    read_handshake(client, &mut username).await?;
    let mut password_length = [0_u8; 1];
    read_handshake(client, &mut password_length).await?;
    let mut candidate = vec![0_u8; password_length[0] as usize];
    read_handshake(client, &mut candidate).await?;
    let accepted = constant_time_eq(&username, b"clash5") && constant_time_eq(&candidate, password);
    candidate.fill(0);
    if !accepted {
        client.write_all(&[AUTH_VERSION, 1]).await?;
        anyhow::bail!("relay authentication failed");
    }
    client.write_all(&[AUTH_VERSION, 0]).await?;

    Ok(())
}

async fn read_connect_request(client: &mut TcpStream) -> Result<([u8; 4], Vec<u8>, Option<u8>)> {
    let mut request = [0_u8; 4];
    read_handshake(client, &mut request).await?;
    if request[0] != SOCKS_VERSION || request[1] != CONNECT || request[2] != 0 {
        anyhow::bail!("unsupported relay request");
    }
    let (domain_length, address_length) = match request[3] {
        1 => (None, 4),
        4 => (None, 16),
        3 => {
            let mut length = [0_u8; 1];
            read_handshake(client, &mut length).await?;
            (Some(length[0]), length[0] as usize)
        }
        _ => anyhow::bail!("invalid relay address type"),
    };
    let mut address = vec![0_u8; address_length + 2];
    read_handshake(client, &mut address).await?;

    Ok((request, address, domain_length))
}

async fn forward_connect(
    client: &mut TcpStream,
    upstream: SocketAddr,
    username: &[u8],
    password: &[u8],
    request: &[u8; 4],
    address: &[u8],
    domain_length: Option<u8>,
) -> Result<()> {
    let mut upstream_stream = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(upstream))
        .await
        .context("sing-box relay connection timed out")??;
    configure_tcp_keepalive(client)?;
    configure_tcp_keepalive(&upstream_stream)?;
    upstream_stream.write_all(&[SOCKS_VERSION, 1, 2]).await?;
    let mut upstream_method = [0_u8; 2];
    read_handshake(&mut upstream_stream, &mut upstream_method).await?;
    if upstream_method != [SOCKS_VERSION, 2] {
        client.write_all(&[SOCKS_VERSION, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        anyhow::bail!("sing-box SOCKS inbound rejected negotiation");
    }
    upstream_stream.write_all(&[AUTH_VERSION, username.len() as u8]).await?;
    upstream_stream.write_all(username).await?;
    upstream_stream.write_all(&[password.len() as u8]).await?;
    upstream_stream.write_all(password).await?;
    let mut upstream_auth = [0_u8; 2];
    read_handshake(&mut upstream_stream, &mut upstream_auth).await?;
    if upstream_auth != [AUTH_VERSION, 0] {
        client.write_all(&[SOCKS_VERSION, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        anyhow::bail!("sing-box SOCKS inbound rejected authentication");
    }
    upstream_stream.write_all(request).await?;
    if let Some(length) = domain_length {
        upstream_stream.write_all(&[length]).await?;
    }
    upstream_stream.write_all(address).await?;
    let mut response = [0_u8; 4];
    read_handshake(&mut upstream_stream, &mut response).await?;
    let reply_address_length = match response[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut length = [0_u8; 1];
            read_handshake(&mut upstream_stream, &mut length).await?;
            client.write_all(&response).await?;
            client.write_all(&length).await?;
            length[0] as usize
        }
        _ => anyhow::bail!("sing-box returned an invalid SOCKS address"),
    };
    let mut reply_address = vec![0_u8; reply_address_length + 2];
    read_handshake(&mut upstream_stream, &mut reply_address).await?;
    if response[3] != 3 {
        client.write_all(&response).await?;
    }
    client.write_all(&reply_address).await?;
    if response[1] != 0 {
        anyhow::bail!("sing-box CONNECT request failed");
    }
    forward_until_idle(client, &mut upstream_stream, DEAD_CONNECTION_IDLE_TIMEOUT).await?;
    Ok(())
}

fn configure_tcp_keepalive(stream: &TcpStream) -> Result<()> {
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(TCP_KEEPALIVE_TIME)
        .with_interval(TCP_KEEPALIVE_INTERVAL);
    socket2::SockRef::from(stream)
        .set_tcp_keepalive(&keepalive)
        .context("failed to enable relay TCP keepalive")
}

async fn forward_until_idle(client: &mut TcpStream, upstream: &mut TcpStream, idle_timeout: Duration) -> Result<()> {
    let (client_reader, mut client_writer) = tokio::io::split(client);
    let (upstream_reader, mut upstream_writer) = tokio::io::split(upstream);
    let (activity, mut changes) = watch::channel(Instant::now());
    let mut copy = Box::pin(async {
        tokio::try_join!(
            copy_with_activity(client_reader, &mut upstream_writer, activity.clone()),
            copy_with_activity(upstream_reader, &mut client_writer, activity),
        )?;
        Ok::<_, std::io::Error>(())
    });

    loop {
        let last_activity = *changes.borrow_and_update();
        tokio::select! {
            result = &mut copy => {
                result.context("relay stream forwarding failed")?;
                return Ok(());
            }
            _ = sleep_until(last_activity + idle_timeout) => {
                if *changes.borrow() <= last_activity {
                    anyhow::bail!("relay connection exceeded its idle timeout");
                }
            }
            result = changes.changed() => {
                if result.is_err() {
                    return Ok(());
                }
            }
        }
    }
}

async fn copy_with_activity<R, W>(mut reader: R, mut writer: W, activity: watch::Sender<Instant>) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            writer.shutdown().await?;
            return Ok(());
        }
        activity.send_replace(Instant::now());
        writer.write_all(&buffer[..count]).await?;
    }
}

async fn read_handshake(stream: &mut TcpStream, bytes: &mut [u8]) -> Result<()> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, stream.read_exact(bytes))
        .await
        .context("relay handshake timed out")??;
    Ok(())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0));
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn relay_rejects_clients_without_username_password_auth() -> Result<()> {
        let upstream = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let relay = AuthenticatedRelay::start(upstream.local_addr()?).await?;
        let mut client = TcpStream::connect((relay.endpoint.host.as_str(), relay.endpoint.port)).await?;
        client.write_all(&[SOCKS_VERSION, 1, 0]).await?;
        let mut response = [0_u8; 2];
        client.read_exact(&mut response).await?;
        assert_eq!(response, [SOCKS_VERSION, 0xff]);
        relay.stop().await;
        Ok(())
    }

    #[tokio::test]
    async fn relay_activity_prevents_idle_exit_while_a_client_is_connected() -> Result<()> {
        let upstream = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let relay = AuthenticatedRelay::start(upstream.local_addr()?).await?;
        let activity = relay.activity();
        let client = TcpStream::connect((relay.endpoint.host.as_str(), relay.endpoint.port)).await?;
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!activity.idle_for(Duration::from_millis(10)).await);
        drop(client);
        tokio::time::timeout(Duration::from_secs(1), async {
            while !activity.idle_for(Duration::from_millis(30)).await {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .context("relay activity did not become idle after the client disconnected")?;
        relay.stop().await;
        Ok(())
    }

    async fn tcp_pair() -> Result<(TcpStream, TcpStream)> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let client = TcpStream::connect(address).await?;
        let (server, _) = listener.accept().await?;
        Ok((client, server))
    }

    #[tokio::test]
    async fn relay_closes_a_connection_after_its_idle_timeout() -> Result<()> {
        let (mut client, _client_peer) = tcp_pair().await?;
        let (mut upstream, _upstream_peer) = tcp_pair().await?;
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            forward_until_idle(&mut client, &mut upstream, Duration::from_millis(30)),
        )
        .await
        .context("idle relay connection was not closed")?;
        let error = result.err().context("an idle relay connection should be closed")?;
        assert!(error.to_string().contains("idle timeout"));
        Ok(())
    }

    #[tokio::test]
    async fn relay_authenticates_then_forwards_socks_connect() -> Result<()> {
        let upstream = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let upstream_address = upstream.local_addr()?;
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await?;
            let mut greeting = [0_u8; 3];
            stream.read_exact(&mut greeting).await?;
            anyhow::ensure!(greeting == [SOCKS_VERSION, 1, 2]);
            stream.write_all(&[SOCKS_VERSION, 2]).await?;
            let mut username_length = [0_u8; 2];
            stream.read_exact(&mut username_length).await?;
            anyhow::ensure!(username_length == [AUTH_VERSION, 6]);
            let mut username = [0_u8; 6];
            stream.read_exact(&mut username).await?;
            anyhow::ensure!(&username == b"clash5");
            let mut password_length = [0_u8; 1];
            stream.read_exact(&mut password_length).await?;
            let mut password = vec![0_u8; password_length[0] as usize];
            stream.read_exact(&mut password).await?;
            anyhow::ensure!(!password.is_empty());
            stream.write_all(&[AUTH_VERSION, 0]).await?;
            let mut request = [0_u8; 10];
            stream.read_exact(&mut request).await?;
            anyhow::ensure!(request == [SOCKS_VERSION, CONNECT, 0, 1, 8, 8, 8, 8, 1, 187]);
            stream.write_all(&[SOCKS_VERSION, 0, 0, 1, 127, 0, 0, 1, 0, 80]).await?;
            Result::<()>::Ok(())
        });
        let relay = AuthenticatedRelay::start(upstream_address).await?;
        let endpoint = relay.endpoint();
        let mut client = TcpStream::connect((endpoint.host.as_str(), endpoint.port)).await?;
        client.write_all(&[SOCKS_VERSION, 1, 2]).await?;
        let mut response = [0_u8; 2];
        client.read_exact(&mut response).await?;
        assert_eq!(response, [SOCKS_VERSION, 2]);
        client.write_all(&[AUTH_VERSION, 6]).await?;
        client.write_all(b"clash5").await?;
        client.write_all(&[endpoint.password.len() as u8]).await?;
        client.write_all(endpoint.password.as_bytes()).await?;
        client.read_exact(&mut response).await?;
        assert_eq!(response, [AUTH_VERSION, 0]);
        client
            .write_all(&[SOCKS_VERSION, CONNECT, 0, 1, 8, 8, 8, 8, 1, 187])
            .await?;
        let mut connect_response = [0_u8; 10];
        client.read_exact(&mut connect_response).await?;
        assert_eq!(connect_response[1], 0);
        fixture.await??;
        relay.stop().await;
        Ok(())
    }

    #[test]
    fn session_tokens_are_fresh_and_constant_time_comparison_checks_length() -> Result<()> {
        let first = session_token()?;
        let second = session_token()?;
        assert_ne!(first, second);
        assert!(constant_time_eq(b"token", b"token"));
        assert!(!constant_time_eq(b"token", b"tokenx"));
        Ok(())
    }
}
