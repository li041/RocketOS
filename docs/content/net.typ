#import "../components/prelude.typ": *

= 网络系统
 RocketOS的网络系统是一个基于_smoltcp_协议栈的网络实现，旨在提供高效灵活的网络通信能力。它支持AF_INET，AF_INET6，AF_UNIX和AF_ALG等多种地址族的套接字，能够处理IPv4和IPv6两类IP地址，同时支持TCP和UDP两种传输协议并通过了iperf，netperf，ltp相关测试。RocketOS通过统一的接口管理所有网络设备和套接字，并支持riscv64和loongarch64下的glibc和musl共计4种架构。

== 网络系统概述
 RocketOS的网络工作模式如下:
#figure(
  align(center,   image("./img/net.png",   width: 100%)),  
  caption: [net工作模式],  
)
 RocketOS的网络系统包括以下几个主要组件:
- *NetDevice*: 网络设备接口，定义了网络设备的基本操作和特性.根据抽象`NetDevice`接口可以实现不同网络设备，包括虚拟本地设备`VirtioNetDevice`和虚拟本地回环设备`LoopbackDev`。

- *InterfaceWrapper*: RocketOS的网卡抽象，系统使用`InterfaceWrapper`来封装_smoltcp_的`Interface`接口和`NetDeviceWrapper`，提供对网卡设备的统一管理和操作。可以支持创建多个硬件网卡在os中的映射。 在RocketOS中，同linux类似分别管理着_lo_回环设备和_eth33_虚拟网卡设备。其中_eth33_虚拟网卡设备通过`qemu`的`10.0.2.15`映射为主机的`10.0.2.2`接口。

- *ListenTable*: RocketOS的全局监听表，用于管理所有监听的端口和连接的socket。监听表维护一个端口到对应连接套接字的映射，通过访问`ListenTable`中连接套接字的状态判断是否允许连接。

- *Socket*: `Socket`提供了对内核套接字的封装，并实现`FileOp`接口，允许通过文件描述符进行访问和操作。

== 网络Device设备--物理层
=== Loongarch64与riscv64适配
 RocketOS的网络系统支持loongarch64和riscv64两种架构，通过分析设备树来映射到内核空间。在riscv64中，网络设备通过MMIO映射到设备地址空间，而在loongarch64中，网络设备通过PCI总线进行挂载。因此，在RocketOS中，选择根据不同架构*条件编译*分析设备树。
```make
#riscv qemu网络配置
-device virtio-net-device,  netdev=net -netdev user,  id=net,  hostfwd=tcp::5555-:5555,  hostfwd=udp::5555-:5555\
#loongarch qemu网络配置
-device virtio-net-pci,  netdev=net -netdev user,  id=net,  hostfwd=tcp::5556-:5555,  hostfwd=udp::5556-:5555 \
```

- 在riscv64中，通过传入`rust_main`的`dtb_address`确定设备树地址，遍历设备树节点查找`compatible`属性为`virtio-net`的节点，并通过获取其`reg`属性来确定设备的MMIO地址并映射到内核。
    #algorithm-figure(
        pseudocode(
            no-number, 
            [*input:* dtb_addr], 
            no-number, 
            [*output:* initialized net device], 
            [*let* dev_tree $<-$ Fdt::from_ptr(dtb_addr + KERNEL_BASE)],  
            [address_cells $<-$ dev_tree.root().prop("address-cells").value[3]],  
            [size_cells $<-$ dev_tree.root().prop("size-cells").value[3]],  
            [*for* node *in* dev_tree.all_nodes() *do*], 
            ind, 
            [*for* prop *in* node.properties() *do*], 
            ind,  
                [log(prop.name)],  
            ded,  
            ded,  
            [*for* node *in* dev_tree.all_nodes() *do*],  
            ind,  
            [*if* node.name == "soc" *then*],  
            ind,  
                [*for* child *in* node.children() *do*],  
                ind,  
                [*if* child.name == "virtio_mmio@10008000" *then*],  
                ind,  
                    [reg $<-$ parse_reg(child,   address_cells,   size_cells)],  
                    [mmio_base $<-$ reg[0].start],  
                    [mmio_size $<-$ reg[0].length],  
                    [map_area $<-$ MapArea::new(
                        VPNRange(KERNEL_BASE+mmio_base,   KERNEL_BASE+mmio_base+mmio_size),  
                        Linear,   R|W
                    )],  
                    [KERNEL_SPACE.lock().push(map_area)],  
                    [sfence.vma()],  
                    [NET_DEVICE_ADDR.lock().replace(KERNEL_BASE+mmio_base)],  
                    [header $<-$ NonNull((KERNEL_BASE+mmio_base) as mut VirtIOHeader)],  
                    [transport $<-$ MmioTransport::new(header)],  
                    [log("vendor=",   transport.vendor_id(),  
                        "version=",   transport.version(),  
                        "type=",   transport.device_type())],  
                    [dev $<-$ VirtioNetDevice::new(transport)],  
                    [net::init(Some(dev))],  
                    [*return*],  
                ded,  
                ded,  
            ded,  
            ded,  
            [*log*("not find a net device")],  
        ),  
        caption: [riscv 网络设备初始化流程],  
        label-name: "riscv_net_device_init",  
    )

- 而在loongarch64中，通过遍历PCI总线设备，查找`device_type`为`network`的节点，并获取其BAR寄存器来确定设备的地址并映射到内核。
    #algorithm-figure(
    pseudocode(
        no-number,  
        [*input:* pci_root,   allocator],  
        no-number,  
        [*output:* 初始化并启动 VirtIO 设备],  
        [*for* (device_fn,   info) *in* pci_root.enumerate_bus(0) *do*],  
        ind,  
        [status,   command $<-$ pci_root.get_status_command(device_fn)],  
        [log("Found",   info,   "at",   device_fn,   "status",   status,   "command",   command)],  
        [*if* virtio_device_type(&info) *then* virtio_type],  
        ind,  
            [log("  VirtIO",   virtio_type)],  
            [allocate_bars(&mut pci_root,   device_fn,   &mut allocator)],  
            [dump_bar_contents(&mut pci_root,   device_fn,   4)],  
            [transport $<-$ PciTransport::new::<HalImpl>(&mut pci_root,   device_fn).unwrap()],  
            [log(
            "Detected virtio PCI device with type",   transport.device_type(),  
            "features",   transport.read_device_features()
            )],  
            [virtio_device(transport)],  
        ded,  
        ded,  
        [*fn* virtio_device(transport) *do*],  
        ind,  
        [*match* transport.device_type() *with*],  
        ind,  
            [DeviceType::Block => virtio_blk(transport)],  
            [DeviceType::Network =>],  
            ind,  
            [log("[initialize net]")],  
            [virtio_net(transport)],  
            ded,  
            [t => log("Unsupported VirtIO device type",   t)],  
        ded,  
        ded,  
    ),  
        caption: [基于 PCI 的 VirtIO 设备初始化流程],  
        label-name: "pci_virtio_init",  
    )

=== NetDevice封装
 RocketOS的网络设备封装了`smoltcp`的`Device`接口，并通过`NetDeviceWrapper`实现了对底层设备的抽象。这使得RocketOS能够支持多种类型的网络设备，包括虚拟网卡和回环设备。
#code-figure(
    ```rs
    // 网络设备管理,  实现sync和send特性,  以便在多线程环境中安全使用
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
        fn recycle_recv_buffer(&mut self,  recv_buf:NetBufPtr);
        //回收发送buffer
        fn recycle_send_buffer(&mut self)->Result<(),  ()>;
        //发送数据
        fn send(&mut self,  ptr:NetBufPtr);
        //接收数据
        fn recv(&mut self)->Option<NetBufPtr>;
        //分配一个发送的网络缓冲区
        fn alloc_send_buffer(&mut self,  size:usize)->NetBufPtr;
    }
    ```,  
    caption: [NetDevice trait],  
    label-name: "NetDevice trait",  
)

 同时为了使`NetDevice`定义符合`smoltcp`的`Device`接口需求，定义了`NetDeviceWrapper`结构体，通过`RefCell`包装 `Box<dyn NetDevice>`，允许内部的可变访问，以便在实现smoltcp中的`Device` trait时提供对底层设备的操作。

#code-figure(
    ```rs
    pub struct NetDeviceWrapper {
        inner: RefCell<Box<dyn NetDevice>>,  
    }
    ```,  
    caption: [NetDeviceWrapper],  
    label-name: "NetDeviceWrapper",  
)


 RocketOS的网络设备还支持在有限的空间中动态对Device发送接收的报文空间进行分配和回收，系统通过定义`NetBufPool`统一管理Device的报文空间并实现`alloc`和`dealloc`方法。Device在`recycle_recv_buffer`和`recycle_send_buffer`便可通过调用`alloc`和`dealloc`方法来分配和回收报文空间，从而提高网络通信的效率并降低内存碎片化。
#code-figure(
    ```rs
    /// A pool of [`NetBuf`]s to speed up buffer allocation.
    ///
    /// It divides a large memory into several equal parts for each buffer.
    pub struct NetBufPool {
        //可以存储的netbuf个数
        capacity: usize,  
        //每个netbuf的长度
        buf_len: usize,  
        pool: Vec<u8>,  
        //用于存储每个待分配的netbuf的offset
        free_list: Mutex<Vec<usize>>,  
    }
    ```,  
    caption: [NetBufPool],  
    label-name: "NetBufPool",  
)
#algorithm-figure(
  pseudocode(
    no-number,  
    [*fn* alloc(self: Arc<Self>) → NetBuf],  
    no-number,  
    [*output:* 新分配的 NetBuf],  
    [offset $<-$ self.free_list.lock().pop().unwrap()],  
    [buf_ptr $<-$ NonNull(self.pool.as_ptr().add(offset) as *mut u8*)],  
    [*return* NetBuf {
      header_len: 0,  
      packet_len: 0,  
      capacity: self.buf_len,  
      buf_ptr: buf_ptr,  
      pool_offset: offset,  
      pool: Arc::clone(self),  
    }],  
    v(.5em),  
    no-number,  
    [*fn* dealloc(self,   offset: usize) → ()],  
    no-number,  
    [*precondition:* offset % self.buf_len == 0],  
    [assert(offset % self.buf_len == 0)],  
    [self.free_list.lock().push(offset)],  
  ),  
  caption: [NetBuf 缓冲区分配与回收],  
  label-name: "netbuf_alloc_dealloc",  
)


 系统还为具体的网络设备实现了_smoltcp_的`Device` trait，以便在使用_smoltcp_的`poll`轮询机制来嗅探网络事件时，通过使用`Device` trait方法调用`NetDeviceWrapper`的相关方法来处理网络数据包的发送和接收。

 如下代码和图所示，_smoltcp_通过*环形令牌网络实现对网络设备的轮询*，这里实现的`Device trait`便是在轮询中对令牌进行分配和管理，并在`NetDeviceWrapper`获得令牌时，通过`NetDevice`接口来处理网络数据包的发送和接收。
 #figure(
  align(center,   image("./img/TokenRing.png",   width: 60%)),  
  caption: [环形令牌网络],  
)
#algorithm-figure(
  pseudocode(
    no-number,  
    [*impl* Device *for* NetDeviceWrapper],  
    no-number,  
    [*output:* Rx/Tx 令牌或设备能力],  
    [*fn* receive(self,   timestamp: Instant) → Option<(RxToken,   TxToken)>],  
    ind,  
      [dev $<-$ self.inner.borrow_mut()],  
      [*if* let Err(e) = dev.recycle_tx_buffers() *then*],  
      ind,  
        [warn("recycle_tx_buffers failed:",   e)],  
        [*return* None],  
      ded,  
      [*if* ¬dev.can_transmit() *then* *return* None],  
      [*match* dev.receive() *with*],  
      ind,  
        [Ok(buf)    => rx_buf $<-$ buf],  
        [Err(DevError::Again) => *return* None],  
        [Err(err)  =>],  
        ind,  
          [warn("receive failed:",   err)],  
          [*return* None],  
        ded,  
      ded,  
      [*return* Some((NetRxToken(&self.inner,   rx_buf),   NetTxToken(&self.inner)))],  
    ded,  
    v(.5em),  
    [*fn* transmit(self,   timestamp: Instant) → Option<TxToken>],  
    ind,  
      [dev $<-$ self.inner.borrow_mut()],  
      [*if* let Err(e) = dev.recycle_tx_buffers() *then*],  
      ind,  
        [warn("recycle_tx_buffers failed:",   e)],  
        [*return* None],  
      ded,  
      [*if* dev.can_transmit() *then*],  
      ind,  
        [*return* Some(AxNetTxToken(&self.inner))],  
      ded,  
      [*else* *return* None],  
    ded,  
    v(.5em),  
    [*fn* capabilities(self) → DeviceCapabilities],  
    ind,  
      [caps $<-$ DeviceCapabilities::default()],  
      [caps.max_transmission_unit $<-$ 1514],  
      [caps.max_burst_size $<-$ None],  
      [caps.medium $<-$ Medium::Ethernet],  
      [*return* caps],  
    ded,  
  ),  
  caption: [NetDeviceWrapper 驱动接口实现伪代码],  
  label-name: "netdevicewrapper_methods",  
)

== Interface设备--数据链路层
 RocketOS的网络接口设备通过`InterfaceWrapper`封装了_smoltcp_的`Interface`和`NetDeviceWrapper`,  提供对网卡设备的统一管理和操作。

 通过封装_smoltcp_的`Interface`设备，系统可以通过_smoltcp_的poll轮询机制来嗅探网络事件;通过封装`NetDeviceWrapper`设备，系统可以在出现网络事件时，通过`NetDevice`接口来处理网络数据包的发送和接收。
#code-figure(
    ```rs
    pub struct InterfaceWrapper {
        //smoltcp网卡抽象
        iface: Mutex<Interface>,  
        //网卡ethenet地址
        address: EthernetAddress,  
        //名字eth0
        name: &'static str,  
        dev: Mutex<NetDeviceWrapper>,  
    }
    ```,  
    caption: [InterfaceWrapper结构体],  
    label-name: "InterfaceWrapper",  
)

 系统通过`poll_interfaces`方法实现多个网卡设备的轮询，并在轮询过程中借由实现的`Device trait`对存在网络事件的设备进行数据收发。
#code-figure(
    ```rs
    pub fn poll_interfaces(&self) {
        //对本地回环设备轮询
        LOOPBACK.lock().poll(
            Instant::from_micros_const((current_time_nanos() / NANOS_PER_MICROS) as i64),  
            LOOPBACK_DEV.lock().deref_mut(),  
            &mut self.0.lock(),  
        );
        //对ens0设备轮询
        ETH0.poll(&self.0);
    }
    ```,  
    caption: [poll_interfaces],  
    label-name: "poll_interfaces函数",  
)
== ListenTable监听表--网络层

 RocketOS实现通过一个全局的`LISTENTABLE`管理所有正在监听的端口和连接的socket。监听表维护一个端口到对应连接套接字的映射，通过访问`ListenTable`中连接套接字的状态判断是否允许连接。
#code-figure(
    ```rs
    static LISTEN_TABLE: LazyInit<ListenTable> = LazyInit::new();
    pub struct ListenTable{
        //是由listenentry构建的监听表
        //监听表的表项个数与端口个数有关, 每个端口只允许一个地址使用
        table:Box<[Mutex<Option<Box<ListenTableEntry>>>]>,  
    }
    #[derive(Clone)]
    struct ListenTableEntry{
        //表示监听的server地址addr
        listen_endpoint:IpListenEndpoint,  
        task_id:usize,  
        //这里由于sockethandle与socket存在RAII特性, 因而可以保存sockethandle
        syn_queue:VecDeque<SocketHandle>
    }
    ```,  
    caption: [ListenTablei结构体],  
    label-name: "ListenTable",  
)
== Socket封装--传输层

 RocketOS对于socket实现了3层封装，实现了对AF_UNIX，AF_INET，AF_ALG，AF_INET6套接字的管理，支持tcp，udp协议。实现`FileOp`接口，允许通过文件描述符进行访问和操作。
 内核socket定义如下，`Socket`结构体封装了协议类`socketinner`，套接字类型，以及具体的套接字实现。它还包含了一些状态信息，发送和接收缓冲区大小等。其中所有内容均通过原子操作或者Mutex进行保护，以确保在多线程环境下的安全性和一致性。而`socketinner`进一步封装了套接字的具体实现，包括tcp，  udp，unix和alg等类型的套接字。
#code-figure(
    ```rs
    pub struct Socket {
        pub domain: Domain,  
        pub socket_type: SocketType,  
        inner: SocketInner,  
        close_exec: AtomicBool,  
        send_buf_size: AtomicU64,  
        recv_buf_size: AtomicU64,  
        congestion: Mutex<String>,  
        recvtimeout: Mutex<Option<TimeSpec>>,  
        dont_route: bool,  
        ...
    }
    pub enum SocketInner {
        Tcp(TcpSocket),  
        Udp(UdpSocket),  
        Unix(UnixSocket),  
        Alg(AlgSocket),  
    }
    ```,  
    caption: [InterfaceWrapper],  
    label-name: "InterfaceWrapper",  
)

 套接字通常作为文件描述符使用，因此RocketOS的套接字还实现了`FileOp`接口，允许通过文件描述符进行访问和操作。这使得套接字可以像文件一样进行读写操作，并支持文件描述符的相关系统调用。

 RocketOS对于`Socket`的管理遵循*RAII*思想，为`Socket`实现drop trait，当socket shutdown时会通过drop释放对应的资源并从全局的`SocketSetWrapper`中移除对应句柄。
#code-figure(
    ```rs
    pub fn remove(&self,   handle: SocketHandle) {
        let socket=self.0.lock().remove(handle);
        drop(socket);
    }
    ```,  
    caption: [Socket remove],  
    label-name: "Socket-remove",  
)

 通过上述设计，RocketOS可以做到统一封装多种物理与虚拟网卡（如 Virtio、回环），支持 IPv4/IPv6、TCP/UDP、AF\_UNIX、AF\_ALG 等多种地址族与协议，在 RISC-V64 与 LoongArch64 上高效灵活地管理网卡轮询、端口监听与 Socket 文件操作。
