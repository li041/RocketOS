#![no_std]
#![no_main]
use core::{mem::{size_of, zeroed}, ops::Sub, str};

use user_lib::{
    bind, close, connect, fork, println, recvfrom, sendto, shutdown, socket, waitpid
};

/// POSIX 常量
const AF_INET6: usize = 10;
const SOCK_DGRAM: usize = 2; // UDP 使用 SOCK_DGRAM

/// IPv6 地址结构
#[repr(C)]
struct In6Addr {
    s6_addr: [u8; 16], // IPv6 地址存储为 16 字节数组
}

/// sockaddr_in6
#[repr(C)]
struct SockAddrIn6 {
    sin6_family: u16,
    sin6_port:   u16,
    sin6_flowinfo: u32,
    sin6_addr:   In6Addr,
    sin6_scope_id: u32,
}

#[no_mangle]
pub fn main() -> i32 {
    let pid = unsafe { fork() };
    if pid < 0 {
        return -2; // fork 失败
    }

    // IPv6 回环地址 ::1 的字节表示
    const IPV6_LOOPBACK: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];

    // 共用地址结构（服务端绑定端口 5555）
    let server_addr = SockAddrIn6 {
        sin6_family: AF_INET6 as u16,
        sin6_port: 22u16.to_be(), // 端口号需要转换网络字节序
        sin6_flowinfo: 0,
        sin6_addr: In6Addr { s6_addr: IPV6_LOOPBACK },
        sin6_scope_id: 0,
    };
    let client_addr_for_server = SockAddrIn6 {
        sin6_family: AF_INET6 as u16,
        sin6_port: 22u16.to_be(), // 端口号需要转换网络字节序
        sin6_flowinfo: 0,
        sin6_addr: In6Addr { s6_addr: IPV6_LOOPBACK },
        sin6_scope_id: 0,
    };

    if pid > 0 {
        // === 父进程：UDP 服务端 ===
        let sock_fd = unsafe { socket(AF_INET6, SOCK_DGRAM, 0) };
        if sock_fd < 0 { return -3; }

        // 绑定到指定地址
        if unsafe { bind(sock_fd as usize, &server_addr as *const _ as usize, size_of::<SockAddrIn6>()) } < 0 {
            return -4; // 绑定失败
        }

        // println!("Server listening on [::1]:5555...");

        // 接收数据
        let mut buf = [0u8; 512];
        let mut client_addr: SockAddrIn6 = unsafe { zeroed() };
        let mut addr_len = size_of::<SockAddrIn6>() as u32;
        connect(sock_fd as usize,&client_addr_for_server as *const _ as usize, size_of::<SockAddrIn6>());

        let n = unsafe {
            recvfrom(
                sock_fd as usize,
                &mut buf,
                512,
                0,
                &mut client_addr as *mut _ as usize,
                &mut addr_len as *mut _ as usize,
            )
        };

        if n > 0 {
            let s = str::from_utf8(&buf[..n as usize]).unwrap_or("[非UTF-8]");
            println!("Server received: {}", s);
        }

        unsafe { shutdown(sock_fd as usize) };
        
        // 等待子进程
        let mut exit_code = 0;
        unsafe { waitpid(pid, &mut exit_code) };
        if exit_code != 0 {
            return -5;
        }

    } else {
        let client_addr = SockAddrIn6 {
            sin6_family: AF_INET6 as u16,
            sin6_port: 22u16.to_be(), // 端口号需要转换网络字节序
            sin6_flowinfo: 0,
            sin6_addr: In6Addr { s6_addr: IPV6_LOOPBACK },
            sin6_scope_id: 0,
        };
        let server_addr_for_client = SockAddrIn6 {
            sin6_family: AF_INET6 as u16,
            sin6_port: 22u16.to_be(), // 端口号需要转换网络字节序
            sin6_flowinfo: 0,
            sin6_addr: In6Addr { s6_addr: IPV6_LOOPBACK },
            sin6_scope_id: 0,
        };
        // === 子进程：UDP 客户端 ===
        let sock_fd = unsafe { socket(AF_INET6, SOCK_DGRAM, 0) };
        if sock_fd < 0 { return -6; }
        bind(sock_fd as usize, &client_addr as *const _ as usize, size_of::<SockAddrIn6>());
        // 发送数据到服务端
        let msg = b"Hello from IPv6 UDP client!";
        let ret = unsafe {
            sendto(
                sock_fd as usize,
                msg,
                msg.len(),
                &server_addr_for_client as *const _ as usize,
                size_of::<SockAddrIn6>(),
                0,
            )
        };

        if ret < 0 {
            return -7; // 发送失败
        }
        println!("Client sent message");
    }

    0
}