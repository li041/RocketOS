/*
 * @Author: Peter/peterluck2021@163.com
 * @Date: 2025-04-17 22:35:04
 * @LastEditors: Peter/peterluck2021@163.com
 * @LastEditTime: 2025-04-22 23:51:16
 * @FilePath: /RocketOS/user/src/bin/testnet_tcp_ip6_loopback.rs
 * @Description: 
 * 
 * Copyright (c) 2025 by peterluck2021@163.com, All Rights Reserved. 
 */
#![no_std]
#![no_main]

use core::{mem::zeroed, str};
use user_lib::{
    accept, bind, connect, fork, listen, println, recvfrom, sendto, shutdown, socket, waitpid, yield_
};

const AF_INET6: usize    = 10; // IPv6 地址族&#8203;:contentReference[oaicite:4]{index=4}
const SOCK_STREAM: usize = 1;  // TCP
const PORT_BE: u16       = 22u16.to_be(); // 端口 22（网络字节序）

#[repr(C)]
struct In6Addr { s6_addr: [u8;16] }

#[repr(C)]
struct SockAddrIn6 {
    sin6_family:   u16,
    sin6_port:     u16,
    sin6_flowinfo: u32,
    sin6_addr:     In6Addr,
    sin6_scope_id: u32,
}

#[no_mangle]
pub fn main() -> i32 {
    let pid = unsafe { fork() };
    if pid < 0 { return -1; }

    // 通用的 IPv6 回环 sock address (::1:22)
    let loop6 = SockAddrIn6 {
        sin6_family:   AF_INET6 as u16,
        sin6_port:     PORT_BE,
        sin6_flowinfo: 0,
        sin6_addr:     In6Addr { s6_addr: [0,0,0,0,0,0,0,0, 0,0,0,0, 0,0,0,1] }, // ::1&#8203;:contentReference[oaicite:5]{index=5}
        sin6_scope_id: 0,
    };

    if pid > 0 {
        // === 父：IPv6 服务端 ===
        let ls = unsafe { socket(AF_INET6, SOCK_STREAM, 0) };
        if ls < 0 { return -2; }

        if unsafe { bind(ls as usize,
                         &loop6 as *const _ as usize,
                         core::mem::size_of::<SockAddrIn6>()) } < 0 {
            return -3;
        }
        if unsafe { listen(ls as usize, 1) } < 0 {
            return -4;
        }
        println!("IPv6 server listening on [::1]:22...");

        let mut client: SockAddrIn6 = unsafe { zeroed() };
        let mut addrlen = core::mem::size_of::<SockAddrIn6>() as u32;
        let fd = unsafe {
            accept(ls as usize,
                   &mut client as *mut _ as usize,
                   &mut addrlen as *mut _ as usize)
        };
        if fd < 0 { return -5; }

        let mut buf = [0u8; 512];
        loop{
            let n = unsafe { recvfrom(fd as usize, &mut buf,512, 0, 0, 0) };
            if n > 0 {
                let msg = str::from_utf8(&buf[..n as usize]).unwrap_or("[非UTF-8]");
                println!("Server got: {}", msg);
            }
        }
        unsafe { shutdown(ls as usize) };
        let mut exit_code = 0;
        unsafe { waitpid(pid , &mut exit_code) };
    } else {
        let loop6_client = SockAddrIn6 {
            sin6_family:   AF_INET6 as u16,
            sin6_port:     PORT_BE,
            sin6_flowinfo: 0,
            sin6_addr:     In6Addr { s6_addr: [0,0,0,0,0,0,0,0, 0,0,0,0, 0,0,0,1] }, // ::1&#8203;:contentReference[oaicite:5]{index=5}
            sin6_scope_id: 0,
        };
        // === 子：IPv6 客户端 ===
        // yield_();
        let cl = unsafe { socket(AF_INET6, SOCK_STREAM, 0) };
        if cl < 0 { return -6; }
        loop {
            connect(cl as usize,
                    &loop6_client as *const _ as usize,
                    core::mem::size_of::<SockAddrIn6>());
            sendto(cl as usize,
                   b"Hello over IPv6 loopback!",
                   25,
                   &loop6_client as *const _ as usize,
                   core::mem::size_of::<SockAddrIn6>(),0);
        }
        println!("Client sent message");
        loop {
            
        }
    }
    0
}
