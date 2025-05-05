#![no_std]
#![no_main]
use core::{mem::{size_of, zeroed}, str};

use user_lib::{
    accept, bind, close, connect, fork, listen, open, println, read, recvfrom, sendto, shutdown, socket, waitpid, write, OpenFlags
};

/// POSIX 常量
const AF_INET:    usize = 2;
const SOCK_STREAM: usize = 1;
const DGRAM:usize=2;

/// IPv4 地址结构
#[repr(C)]
struct InAddr { s_addr: u32 }

/// sockaddr_in
#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port:   u16,
    sin_addr:   InAddr,
    sin_zero:   [u8;8],
}

#[no_mangle]
pub fn main() -> i32 {
    // let pid = unsafe { fork() };
    // if pid < 0 {
    //     return -2; // fork 失败
    // }
    // if pid>0 {
         // 公用的 sockaddr_in，用于绑定和连接
    let peer_addr = SockAddrIn {
        sin_family: AF_INET as u16,
        sin_port:   5555u16.to_be(), // 端口 22（网络字节序）
        sin_addr:   InAddr {
            s_addr: u32::from_be_bytes([15, 2, 0, 10]), // 127.0.0.1
        },
        sin_zero: [0;8],
    };
    // let fd=open("testfile",OpenFlags::RDWR);
    // if pid > 0 {
        // === 父进程：服务端 ===
        let listen_fd = unsafe { socket(AF_INET, SOCK_STREAM, 0) };
        if listen_fd < 0 { return -3; }
        
        // 绑定到指定地址
        if unsafe { bind(listen_fd as usize, &peer_addr as *const _ as usize, size_of::<SockAddrIn>() as usize) } < 0 {
            return -4; // 绑定失败
        }
        
        // 开始监听
        if unsafe { listen(listen_fd as usize, 1) } < 0 {
            return -5; // 监听失败
        }
        println!("Server listening on 10.0.2.15:5556...");

        // 通知子进程开始连接
        // if unsafe { write(fd as usize, &[1u8]) } < 0 {
        //     return -6; // 写入管道失败
        // }

        // 接受连接
        let mut client_addr: SockAddrIn = unsafe { zeroed() };
        let mut addr_len = size_of::<SockAddrIn>() as u32;
        // let buf=b"1";
        // write(fd as usize, buf);
        let client_fd = unsafe {
            accept(
                listen_fd as usize,
                &mut client_addr as *mut _ as usize,
                &mut addr_len as *mut _ as usize,
            )
        };

        if client_fd > 0 {
            loop {
                let mut buf = [0u8; 512];
                let n = unsafe { recvfrom(client_fd as usize, &mut buf, 512, 0,0,0) }; // 使用recv接收数据
                if n > 0 {
                    let s = str::from_utf8(&buf[..n as usize]).unwrap_or("[非UTF-8]");
                    println!("Server received: {}", s);
                    break;
                }
            }

        }
        // loop {
            
        // }
        // 等待客户端进程结束
        shutdown(listen_fd as usize);
        shutdown(client_fd as usize);
        // close(listen_fd as usize);
        // close(client_fd as usize);
        let mut exit_code = 0;
        // unsafe { waitpid(pid as usize, &mut exit_code) };
        if exit_code != 0 {
            return -7; // 子进程异常退出
        }
    // }
    // else {
    //     let peer_addr_client = SockAddrIn {
    //         sin_family: AF_INET as u16,
    //         sin_port:   5555u16.to_be(), // 端口 22（网络字节序）
    //         sin_addr:   InAddr {
    //             s_addr: u32::from_be_bytes([15, 2, 0, 10]), // 127.0.0.1
    //         },
    //         sin_zero: [0;8],
    //     };
    //     // === 子进程：客户端 ===
    
    //     // 创建套接字并连接到服务端
    //     let client_fd = unsafe { socket(AF_INET, SOCK_STREAM, 0) };
    //     if client_fd < 0 { return -9; }
    //     loop {
    //         unsafe {
    //                 let a=connect(
    //                         client_fd as usize,
    //                         &peer_addr_client as *const _ as usize,
    //                         size_of::<SockAddrIn>() as usize,
    //                 );
    //                 sendto(client_fd as usize, "Hello from client!".as_bytes(), "Hello from client!".len(), &peer_addr_client as *const _ as usize, size_of::<SockAddrIn>() as usize,0);
    //                 sendto(client_fd as usize, "this is text2".as_bytes(), "this is text2".len(),&peer_addr_client as *const _ as usize, size_of::<SockAddrIn>() as usize,0);
    //         };
    
    //     }
    //     println!("Message sent successfully");
    // }
   
    0
}
