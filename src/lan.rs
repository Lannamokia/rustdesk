#[cfg(not(target_os = "ios"))]
use hbb_common::whoami;
use hbb_common::{
    allow_err,
    anyhow::bail,
    config::Config,
    config::{self, RENDEZVOUS_PORT},
    log,
    protobuf::Message as _,
    rendezvous_proto::*,
    tokio::{
        self,
        sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    },
    ResultType,
};

use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    time::Instant,
};

type Message = RendezvousMessage;

// vhd-machine-auth-bridge §17.5 / Requirement 20.5a, 20.5b:
// `controlled-only` 形态下被动 LAN 应答 SHALL 保留（同局域网 P2P 直连依赖它），
// 因此 `start_listening` 与下方 pong 构造路径都 NOT 受 `controlled-only` 约束。
// 但 pong 报文 SHALL 仅暴露既有 `PeerDiscovery` 字段（cmd / id / mac / hostname /
// username / platform），SHALL NOT 写入 `Bridge_Config.secret_version` /
// `Bridge_State` / `controlledMachineId` 或任何 §17 / §19 桥接元数据；
// `misc` 字段显式保持为空。该约束由本文件底部的回归测试守护。
#[cfg(not(target_os = "ios"))]
fn build_pong_peer_discovery(
    id: String,
    mac: String,
    hostname: String,
    username: String,
    platform: String,
) -> PeerDiscovery {
    PeerDiscovery {
        cmd: "pong".to_owned(),
        mac,
        id,
        hostname,
        username,
        platform,
        ..Default::default()
    }
}

#[cfg(not(target_os = "ios"))]
pub(super) fn start_listening() -> ResultType<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], get_broadcast_port()));
    let socket = std::net::UdpSocket::bind(addr)?;
    socket.set_read_timeout(Some(std::time::Duration::from_millis(1000)))?;
    log::info!("lan discovery listener started");
    loop {
        let mut buf = [0; 2048];
        if let Ok((len, addr)) = socket.recv_from(&mut buf) {
            if let Ok(msg_in) = Message::parse_from_bytes(&buf[0..len]) {
                match msg_in.union {
                    Some(rendezvous_message::Union::PeerDiscovery(p)) => {
                        if p.cmd == "ping"
                            && config::option2bool(
                                "enable-lan-discovery",
                                &Config::get_option("enable-lan-discovery"),
                            )
                        {
                            let id = Config::get_id();
                            if p.id == id {
                                continue;
                            }
                            if let Some(self_addr) = get_ipaddr_by_peer(&addr) {
                                let mut msg_out = Message::new();
                                let mut hostname = crate::whoami_hostname();
                                // The default hostname is "localhost" which is a bit confusing
                                if hostname == "localhost" {
                                    hostname = "unknown".to_owned();
                                }
                                let peer = build_pong_peer_discovery(
                                    id,
                                    get_mac(&self_addr),
                                    hostname,
                                    crate::platform::get_active_username(),
                                    whoami::platform().to_string(),
                                );
                                msg_out.set_peer_discovery(peer);
                                socket.send_to(&msg_out.write_to_bytes()?, addr).ok();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

// vhd-machine-auth-bridge §17.4 / Requirement 20.5:
// controlled-only 形态下 "主动局域网发现" 与 WOL 发送路径被裁剪。被动监听
// `start_listening` 由任务 17.5 单独保留以确保同 LAN 主控端仍能 P2P 发现本机。
// 保留函数签名以避免散落 cfg 到调用方（`ui_interface::discover` /
// `flutter_ffi::main_wol` 等），仅把函数体替换为非阻塞的 no-op。

#[cfg(not(feature = "controlled-only"))]
#[tokio::main(flavor = "current_thread")]
pub async fn discover() -> ResultType<()> {
    let sockets = send_query()?;
    let rx = spawn_wait_responses(sockets);
    handle_received_peers(rx).await?;

    log::info!("discover ping done");
    Ok(())
}

#[cfg(feature = "controlled-only")]
pub fn discover() -> ResultType<()> {
    log::warn!("vhd_bridge: refused active LAN discovery in controlled-only build");
    Ok(())
}

#[cfg(not(feature = "controlled-only"))]
pub fn send_wol(id: String) {
    let interfaces = default_net::get_interfaces();
    for peer in &config::LanPeers::load().peers {
        if peer.id == id {
            for (_, mac) in peer.ip_mac.iter() {
                if let Ok(mac_addr) = mac.parse() {
                    for interface in &interfaces {
                        for ipv4 in &interface.ipv4 {
                            // remove below mask check to avoid unexpected bug
                            // if (u32::from(ipv4.addr) & u32::from(ipv4.netmask)) == (u32::from(peer_ip) & u32::from(ipv4.netmask))
                            log::info!("Send wol to {mac_addr} of {}", ipv4.addr);
                            allow_err!(wol::send_wol(mac_addr, None, Some(IpAddr::V4(ipv4.addr))));
                        }
                    }
                }
            }
            break;
        }
    }
}

#[cfg(feature = "controlled-only")]
pub fn send_wol(id: String) {
    let _ = id;
    log::warn!("vhd_bridge: refused send_wol in controlled-only build");
}

#[inline]
fn get_broadcast_port() -> u16 {
    (RENDEZVOUS_PORT + 3) as _
}

fn get_mac(_ip: &IpAddr) -> String {
    #[cfg(not(target_os = "ios"))]
    if let Ok(mac) = get_mac_by_ip(_ip) {
        mac.to_string()
    } else {
        "".to_owned()
    }
    #[cfg(target_os = "ios")]
    "".to_owned()
}

#[cfg(not(target_os = "ios"))]
fn get_mac_by_ip(ip: &IpAddr) -> ResultType<String> {
    for interface in default_net::get_interfaces() {
        match ip {
            IpAddr::V4(local_ipv4) => {
                if interface.ipv4.iter().any(|x| x.addr == *local_ipv4) {
                    if let Some(mac_addr) = interface.mac_addr {
                        return Ok(mac_addr.address());
                    }
                }
            }
            IpAddr::V6(local_ipv6) => {
                if interface.ipv6.iter().any(|x| x.addr == *local_ipv6) {
                    if let Some(mac_addr) = interface.mac_addr {
                        return Ok(mac_addr.address());
                    }
                }
            }
        }
    }
    bail!("No interface found for ip: {:?}", ip);
}

// Mainly from https://github.com/shellrow/default-net/blob/cf7ca24e7e6e8e566ed32346c9cfddab3f47e2d6/src/interface/shared.rs#L4
fn get_ipaddr_by_peer<A: ToSocketAddrs>(peer: A) -> Option<IpAddr> {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };

    match socket.connect(peer) {
        Ok(()) => (),
        Err(_) => return None,
    };

    match socket.local_addr() {
        Ok(addr) => return Some(addr.ip()),
        Err(_) => return None,
    };
}

// Helpers below are only used by the active-discovery path (`discover`) and
// the WOL sender path (`send_wol`). Under `controlled-only` both senders are
// gated to no-ops, so these helpers also become unused; gate them to avoid
// dead-code warnings while keeping the passive `start_listening` unaffected.
#[cfg(not(feature = "controlled-only"))]
fn create_broadcast_sockets() -> Vec<UdpSocket> {
    let mut ipv4s = Vec::new();
    // TODO: maybe we should use a better way to get ipv4 addresses.
    // But currently, it's ok to use `[Ipv4Addr::UNSPECIFIED]` for discovery.
    // `default_net::get_interfaces()` causes undefined symbols error when `flutter build` on iOS simulator x86_64
    #[cfg(not(any(target_os = "ios")))]
    for interface in default_net::get_interfaces() {
        for ipv4 in &interface.ipv4 {
            ipv4s.push(ipv4.addr.clone());
        }
    }
    ipv4s.push(Ipv4Addr::UNSPECIFIED); // for robustness
    let mut sockets = Vec::new();
    for v4_addr in ipv4s {
        // removing v4_addr.is_private() check, https://github.com/rustdesk/rustdesk/issues/4663
        if let Ok(s) = UdpSocket::bind(SocketAddr::from((v4_addr, 0))) {
            if s.set_broadcast(true).is_ok() {
                sockets.push(s);
            }
        }
    }
    sockets
}

#[cfg(not(feature = "controlled-only"))]
fn send_query() -> ResultType<Vec<UdpSocket>> {
    let sockets = create_broadcast_sockets();
    if sockets.is_empty() {
        bail!("Found no bindable ipv4 addresses");
    }

    let mut msg_out = Message::new();
    // We may not be able to get the mac address on mobile platforms.
    // So we need to use the id to avoid discovering ourselves.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let id = crate::ui_interface::get_id();
    // `crate::ui_interface::get_id()` will cause error:
    // `get_id()` uses async code with `current_thread`, which is not allowed in this context.
    //
    // No need to get id for desktop platforms.
    // We can use the mac address to identify the device.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let id = "".to_owned();
    let peer = PeerDiscovery {
        cmd: "ping".to_owned(),
        id,
        ..Default::default()
    };
    msg_out.set_peer_discovery(peer);
    let out = msg_out.write_to_bytes()?;
    let maddr = SocketAddr::from(([255, 255, 255, 255], get_broadcast_port()));
    for socket in &sockets {
        allow_err!(socket.send_to(&out, maddr));
    }
    log::info!("discover ping sent");
    Ok(sockets)
}

#[cfg(not(feature = "controlled-only"))]
fn wait_response(
    socket: UdpSocket,
    timeout: Option<std::time::Duration>,
    tx: UnboundedSender<config::DiscoveryPeer>,
) -> ResultType<()> {
    let mut last_recv_time = Instant::now();

    let local_addr = socket.local_addr();
    let try_get_ip_by_peer = match local_addr.as_ref() {
        Err(..) => true,
        Ok(addr) => addr.ip().is_unspecified(),
    };
    let mut mac: Option<String> = None;

    socket.set_read_timeout(timeout)?;
    loop {
        let mut buf = [0; 2048];
        if let Ok((len, addr)) = socket.recv_from(&mut buf) {
            if let Ok(msg_in) = Message::parse_from_bytes(&buf[0..len]) {
                match msg_in.union {
                    Some(rendezvous_message::Union::PeerDiscovery(p)) => {
                        last_recv_time = Instant::now();
                        if p.cmd == "pong" {
                            let local_mac = if try_get_ip_by_peer {
                                if let Some(self_addr) = get_ipaddr_by_peer(&addr) {
                                    get_mac(&self_addr)
                                } else {
                                    "".to_owned()
                                }
                            } else {
                                match mac.as_ref() {
                                    Some(m) => m.clone(),
                                    None => {
                                        let m = if let Ok(local_addr) = local_addr {
                                            get_mac(&local_addr.ip())
                                        } else {
                                            "".to_owned()
                                        };
                                        mac = Some(m.clone());
                                        m
                                    }
                                }
                            };

                            if local_mac.is_empty() && p.mac.is_empty() || local_mac != p.mac {
                                allow_err!(tx.send(config::DiscoveryPeer {
                                    id: p.id.clone(),
                                    ip_mac: HashMap::from([
                                        (addr.ip().to_string(), p.mac.clone(),)
                                    ]),
                                    username: p.username.clone(),
                                    hostname: p.hostname.clone(),
                                    platform: p.platform.clone(),
                                    online: true,
                                }));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if last_recv_time.elapsed().as_millis() > 3_000 {
            break;
        }
    }
    Ok(())
}

#[cfg(not(feature = "controlled-only"))]
fn spawn_wait_responses(sockets: Vec<UdpSocket>) -> UnboundedReceiver<config::DiscoveryPeer> {
    let (tx, rx) = unbounded_channel::<_>();
    for socket in sockets {
        let tx_clone = tx.clone();
        std::thread::spawn(move || {
            allow_err!(wait_response(
                socket,
                Some(std::time::Duration::from_millis(10)),
                tx_clone
            ));
        });
    }
    rx
}

#[cfg(not(feature = "controlled-only"))]
async fn handle_received_peers(mut rx: UnboundedReceiver<config::DiscoveryPeer>) -> ResultType<()> {
    let mut peers = config::LanPeers::load().peers;
    peers.iter_mut().for_each(|peer| {
        peer.online = false;
    });

    let mut response_set = HashSet::new();
    let mut last_write_time: Option<Instant> = None;
    loop {
        tokio::select! {
            data = rx.recv() => match data {
                Some(mut peer) => {
                    let in_response_set = !response_set.insert(peer.id.clone());
                    if let Some(pos) = peers.iter().position(|x| x.is_same_peer(&peer) ) {
                        let peer1 = peers.remove(pos);
                        if in_response_set {
                            peer.ip_mac.extend(peer1.ip_mac);
                            peer.online = true;
                        }
                    }
                    peers.insert(0, peer);
                    if last_write_time.map(|t| t.elapsed().as_millis() > 300).unwrap_or(true)  {
                        config::LanPeers::store(&peers);
                        #[cfg(feature = "flutter")]
                        crate::flutter_ffi::main_load_lan_peers();
                        last_write_time = Some(Instant::now());
                    }
                }
                None => {
                    break
                }
            }
        }
    }

    config::LanPeers::store(&peers);
    #[cfg(feature = "flutter")]
    crate::flutter_ffi::main_load_lan_peers();
    Ok(())
}

// vhd-machine-auth-bridge §17.5 / Requirement 20.5b 回归测试。
// 校验 `start_listening` 应答路径构造的 `PeerDiscovery` SHALL 仅承载
// 既有字段（cmd / id / mac / hostname / username / platform），SHALL NOT
// 在编码后的字节流中夹带任何 `vhd-bridge` / `secret_version` /
// `controlledMachineId` / `Bridge_State` / `Maintenance_Overlay` /
// `VHDMount` / `VHDRustDeskBridge` 等桥接元数据。本测试在所有部署形态
// 下都同等运行，使 `controlled-only` + `vhd-bridge` 形态产物的回归不会
// 静默漏出。
#[cfg(test)]
#[cfg(not(target_os = "ios"))]
mod tests {
    use super::*;

    // 探针：任何一旦命中说明 pong 报文已经泄漏桥接元数据。
    const FORBIDDEN_TOKENS: &[&str] = &[
        "vhd-bridge",
        "vhd_bridge",
        "VHDMount",
        "VHDRustDeskBridge",
        "secretVersion",
        "secret_version",
        "Bridge_State",
        "BridgeState",
        "controlledMachineId",
        "controlled_machine_id",
        "Maintenance_Overlay",
        "controlled-only",
    ];

    #[test]
    fn pong_payload_only_exposes_existing_peer_discovery_fields() {
        let id = "123456789".to_owned();
        let mac = "aa:bb:cc:dd:ee:ff".to_owned();
        let hostname = "host-a".to_owned();
        let username = "user-a".to_owned();
        let platform = "Windows".to_owned();

        let peer = build_pong_peer_discovery(
            id.clone(),
            mac.clone(),
            hostname.clone(),
            username.clone(),
            platform.clone(),
        );

        assert_eq!(peer.cmd, "pong");
        assert_eq!(peer.id, id);
        assert_eq!(peer.mac, mac);
        assert_eq!(peer.hostname, hostname);
        assert_eq!(peer.username, username);
        assert_eq!(peer.platform, platform);
        // `misc` 是 PeerDiscovery 中唯一未被 start_listening 显式赋值的字段；
        // 任何后续改动若把桥接元数据塞进 `misc`，本断言 SHALL 立即失败。
        assert!(
            peer.misc.is_empty(),
            "pong peer_discovery.misc must remain empty; got {:?}",
            peer.misc
        );

        let mut msg_out = Message::new();
        msg_out.set_peer_discovery(peer);
        let bytes = msg_out
            .write_to_bytes()
            .expect("encoding pong PeerDiscovery must succeed");

        for token in FORBIDDEN_TOKENS {
            assert!(
                !bytes.windows(token.len()).any(|w| w == token.as_bytes()),
                "pong payload leaked forbidden bridge token {:?}",
                token
            );
        }
    }
}
