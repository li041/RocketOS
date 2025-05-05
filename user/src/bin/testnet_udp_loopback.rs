#![no_std]
#![no_main]
use core::{mem::{size_of, zeroed}, str};

use user_lib::{
    bind, close, connect, fork, println, recvfrom, sendto, shutdown, socket, waitpid
};

/// POSIX 常量
const AF_INET: usize = 2;
const AF_INET6:usize=10;
const SOCK_DGRAM: usize = 2; // UDP 使用 SOCK_DGRAM

/// IPv4 地址结构
#[repr(C)]
struct InAddr { s_addr: u32 }

/// sockaddr_in
#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port:   u16,
    sin_addr:   InAddr,
    sin_zero:   [u8; 8],
}

#[no_mangle]
pub fn main() -> i32 {
    let pid = unsafe { fork() };
    if pid < 0 {
        return -2; // fork 失败
    }

    // 共用地址结构（服务端绑定端口 5555）
    let server_addr_for_server = SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port:   22u16.to_be(), // 服务端端口
        sin_addr:   InAddr {
            s_addr: u32::from_be_bytes([1, 0, 0, 127]), // 127.0.0.1
        },
        sin_zero: [0; 8],
    };
    let client_addr_for_server = SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port:   22u16.to_be(), // 服务端端口
        sin_addr:   InAddr {
            s_addr: u32::from_be_bytes([1, 0, 0, 127]), // 127.0.0.1
        },
        sin_zero: [0; 8],
    };
    if pid > 0 {
        // === 父进程：UDP 服务端 ===
        let sock_fd = unsafe { socket(AF_INET, SOCK_DGRAM, 0) };
        if sock_fd < 0 { return -3; }

        // 绑定到指定地址
        if unsafe { bind(sock_fd as usize, &server_addr_for_server as *const _ as usize, size_of::<SockAddrIn>()) } < 0 {
            return -4; // 绑定失败
        }

        // println!("Server listening on 127.0.0.1:5555...");

        // 接收数据
        let mut buf = [0u8; 512];
        let mut client_addr: SockAddrIn = unsafe { zeroed() };
        let mut addr_len = size_of::<SockAddrIn>() as u32;
        //这里选择g使用connect,后续不用recv,send指定了
        connect(sock_fd as usize, &client_addr_for_server as *const _ as usize, size_of::<SockAddrIn>());

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

        //     // 可选：发送响应
        //     let resp = b"ACK from server";
        //     let _ = unsafe {
        //         sendto(
        //             sock_fd as usize,
        //             resp,
        //             resp.len(),
        //             0,
        //             &client_addr as *const _ as usize,
        //             size_of::<SockAddrIn>(),
        //         )
        //     };
        }

        // 关闭套接字
        unsafe { shutdown(sock_fd as usize) };
        
        // 等待子进程
        let mut exit_code = 0;
        unsafe { waitpid(pid, &mut exit_code) };
        if exit_code != 0 {
            return -5;
        }

    } else {
        let server_addr_for_client = SockAddrIn {
            sin_family: AF_INET as u16,
            sin_port:   22u16.to_be(), // 服务端端口
            sin_addr:   InAddr {
                s_addr: u32::from_be_bytes([1, 0, 0, 127]), // 127.0.0.1
            },
            sin_zero: [0; 8],
        };
        let client_addr_for_client = SockAddrIn {
            sin_family: AF_INET as u16,
            sin_port:   22u16.to_be(), // 服务端端口
            sin_addr:   InAddr {
                s_addr: u32::from_be_bytes([1, 0, 0, 127]), // 127.0.0.1
            },
            sin_zero: [0; 8],
        };
        // === 子进程：UDP 客户端 ===
        let sock_fd = unsafe { socket(AF_INET, SOCK_DGRAM, 0) };
        if sock_fd < 0 { return -6; }
        println!("{}",&server_addr_for_client as *const _ as usize);
        // connect(sock_fd as usize, & as *const _ as usize, size_of::<SockAddrIn>());
        bind(sock_fd as usize, &client_addr_for_client as *const _ as usize, core::mem::size_of::<SockAddrIn>() as usize);
        // 发送数据到服务端
        let msg = b"Hello from UDP client!";
        let ret = unsafe {
            sendto(
                sock_fd as usize,
                msg,
                msg.len(),
                &server_addr_for_client as *const _ as usize,
                size_of::<SockAddrIn>(),
                0,
            )
        };

        if ret < 0 {
            return -7; // 发送失败
        }
        println!("Client sent message");
        // loop {
            
        // }
        // // 可选：接收响应
        // let mut buf = [0u8; 512];
        // let mut addr_len = size_of::<SockAddrIn>() as u32;
        // let n = unsafe {
        //     recvfrom(
        //         sock_fd as usize,
        //         &mut buf,
        //         512,
        //         0,
        //         0 as usize, // 不关心来源地址
        //         &mut addr_len as *mut _ as usize,
        //     )
        // };
        // if n > 0 {
        //     println!("Client received response");
        // }

        // unsafe { shutdown(sock_fd as usize) };
    }

    0
}