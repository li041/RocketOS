#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IoVec {
    pub base: usize,
    pub len: usize,
}

bitflags::bitflags! {
    // 定义于 <bits/poll.h>。
    #[derive(Debug, Clone, Copy)]
    pub struct PollEvents: i16 {
        // 可以被监听的事件类型。这些位可以在 `events` 中设置，表示感兴趣的事件类型；
        // 它们会出现在 `revents` 中，表示文件描述符的实际状态。
        /// 有可读的数据
        const IN = 0x001;
        /// 有紧急数据可读
        const PRI = 0x002;
        /// 当前可写，写操作不会阻塞
        const OUT = 0x004;

        // 总是会隐式监听的事件类型。这些位不需要在 `events` 中设置，
        // 但如果发生了，它们会出现在 `revents` 中，表示文件描述符的状态。
        /// Err Condition
        const ERR = 0x008;
        /// Hang up (例如对端关闭了连接)
        const HUP = 0x010;
        /// invalid  poll request (例如文件描述符无效)
        const INVAL = 0x020;
    }
}
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct PollFd {
    pub fd: i32,
    pub events: PollEvents,
    pub revents: PollEvents,
}
