#import "../components/prelude.typ": *


参考starryOS和往届一等奖作品PhoenixOS的网络实现
使用的网络协议栈是smoltcp,能够处理IPv4和IPv6两类IP地址。
支持tcp和udp两种传输协议.
支持unix域套接字.
支持af_alg套接字.
支持socketpair双工传输

特点:

3层封装socket,使用socket_set统一管理所有socket,并将socket同sockhandle绑定,使用listentable管理所有监听地址和连接socket.

socket支持file trait操作

socket参考PhoenixOS实现了使用AtomicU8管理套接字状态

socket使用poll轮询查看可读写事件

socket支持非阻塞和阻塞两种模式

socket 网络序和主机序转换

socket支持以10.0.2.2为默认IP地址的虚拟网卡,通过qemu映射为主机的10.0.2.15

socket虚拟网卡设置

socket本地回环loopback设置,使用统一的Netdevice接口管理

recycle回收alloc分配,使用统一的Allocator接口管理

支持loogarch 在device中支持pci总线的网络设备和网卡设备

网络系统

网络系统综述

网络系统device定义,loopback netdevice(socket的回收和分配,socket的buffer池管理)
网络系统interface网卡定义
网络socket封装(如何封装, socket方法实现File_op,socket的状态管理,socket的poll管理,socketset,listentable管理,如何使用poll轮询管理socket的可读写事件)
网络支持unix,af_alg套接字,支持socketpair双工传输(如何实现的双工)
网络支持loogarch的pci总线设备

= 网络系统
RocketOS的网络系统是一个基于_smoltcp_协议栈的网络实现,旨在提供高效灵活的网络通信能力.它支持多种网络协议和套接字类型,并通过统一的接口管理所有网络设备和套接字.

RocketOS网络系统支持AF_INET,AF_INET6,AF_UNIX和AF_ALG等多种地址族的套接字,能够处理IPv4和IPv6两类IP地址.它还支持TCP和UDP两种传输协议.此外,RocketOS还实现了基于pipe的socketpair双工传输功能并通过了iperf,netperf,ltp相关测试.
== 网络系统概述
RocketOS的网络包括以下几个主要组件:
- `NetDevice`: 网络设备接口,定义了网络设备的基本操作和特性.根据抽象`NetDevice`接口可以实现不同网络设备,包括虚拟本地设备`VirtioNetDevice`和虚拟本地回环设备`LoopbackDev`.
```rs
// 网络设备管理,实现sync和send特性,以便在多线程环境中安全使用
pub trait NetDevice:Sync + Send {
    //获取设备容量
    fn capabilities(&self)->smoltcp::phy::DeviceCapabilities;
    //获取设备mac地址
    fn mac_address(&self)->EthernetAddress;
    //是否可以发送数据
    fn isok_send(&self)->bool;
    //是否可以接收数据
    fn isok_recv(&self)->bool;
    //一次最多可以发送报文数量
    fn max_send_buf_num(&self)->usize;
    //一次最多可以发送报文数量
    fn max_recv_buf_num(&self)->usize;
    //回收接收buffer
    fn recycle_recv_buffer(&mut self,recv_buf:NetBufPtr);
    //回收发送buffer
    fn recycle_send_buffer(&mut self)->Result<(),()>;
    //发送数据
    fn send(&mut self,ptr:NetBufPtr);
    //接收数据
    fn recv(&mut self)->Option<NetBufPtr>;
    //分配一个发送的数据包
    fn alloc_send_buffer(&mut self,size:usize)->NetBufPtr;
}
```
同时为了使`NetDevice`定义符合`smoltcp`的`Device`接口需求,定义了`NetDeviceWrapper`结构体，通过`RefCell`包装 `Box<dyn NetDevice>` ,允许内部的可变访问,以便在实现smoltcp中的`Device` trait时提供对底层设备的操作.
```rs
pub struct NetDeviceWrapper {
    inner: RefCell<Box<dyn NetDevice>>,
}
```

- `InterfaceWrapper`: RocketOS的网卡抽象,系统使用`InterfaceWrapper`来封装_smoltcp_的`Interface`接口和`NetDeviceWrapper`,提供对网卡设备的统一管理和操作.可以支持创建多个硬件网卡在os中的映射.在RocketOS中,同linux类似分别管理着`lo`回环设备和`eth33`虚拟网卡设备.其中`eth33`虚拟网卡设备通过`qemu`的`10.0.2.15`映射为主机的`10.0.2.2`接口
```rs
pub struct InterfaceWrapper {
    //smoltcp网卡
    iface: Mutex<Interface>,
    //网卡ethenet地址
    address: EthernetAddress,
    //名字eth0
    name: &'static str,
    //网卡设备抽象
    dev: Mutex<NetDeviceWrapper>,
}
```