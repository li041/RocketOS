/*
 * @Author: Peter/peterluck2021@163.com
 * @Date: 2025-04-17 23:24:30
 * @LastEditors: Peter/peterluck2021@163.com
 * @LastEditTime: 2025-04-18 15:43:53
 * @FilePath: /RocketOS/user/src/bin/testnet_tcp_ip6_client.rs
 * @Description: 
 * 
 * Copyright (c) 2025 by peterluck2021@163.com, All Rights Reserved. 
 */
#![no_std]
#![no_main]

use core::{mem::{size_of, zeroed}, str};
use user_lib::{
    connect, println, sendto, shutdown, socket, yield_
};

/// POSIX 常量
const AF_INET6:    usize = 10; // IPv6 地址族
const SOCK_STREAM: usize = 1;  // TCP

/// IPv6 地址结构
#[repr(C)]
struct In6Addr { s6_addr: [u8; 16] }

/// sockaddr_in6
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
    // 连接目标为宿主机虚拟 IPv6 地址 fec0::2，端口 5556
    let server_addr = SockAddrIn6 {
        sin6_family:   AF_INET6 as u16,
        sin6_port:     5556u16.to_be(),    // 端口 5556（网络字节序）
        sin6_flowinfo: 0,
        sin6_addr: In6Addr {
            s6_addr: [
                0xfe, 0xc0, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x02, // fec0::2
            ],
        },
        sin6_scope_id: 0,
    };

    // 创建 socket
    let sock_fd = unsafe { socket(AF_INET6, SOCK_STREAM, 0) };
    if sock_fd < 0 {
        println!("socket 创建失败");
        return -1;
    }

    // 连接宿主机地址
    if unsafe {
        connect(
            sock_fd as usize,
            &server_addr as *const _ as usize,
            size_of::<SockAddrIn6>(),
        )
    } < 0 {
        println!("connect 失败");
        return -2;
    }

    println!("Connected to host [fec0::2]:5556!");

    // 发送一段消息
    let msg = b"Hello from Guest IPv6 client!";
    let sent = unsafe {
        sendto(
            sock_fd as usize,
            msg,
            msg.len(),
            0,
            0,
            0,
        )
    };
    println!("Sent {} bytes to host.", sent);

    // 关闭连接
    unsafe {
        shutdown(sock_fd as usize);
    }

    println!("Client shutdown.");
    0
}
