use core::ptr;

use alloc::{collections::{btree_map::BTreeMap, vec_deque::VecDeque}, string::String, sync::{Arc, Weak}};
use alloc::vec::Vec;
use alloc::vec;
use xmas_elf::header;

use crate::{
    config::{PAGE_SIZE, PAGE_SIZE_BITS}, drivers::block::{
        block_cache::get_block_cache,
        block_dev::BlockDevice,
    }, ext4::{block_op::{Ext4DirContentRO, Ext4DirContentWE}, extent_tree::Ext4ExtentIdx}, fat32::inode, fs::{
        dentry::Dentry, inode::InodeOp, inode_trait::InodeState, page_cache::AddressSpace, FSMutex
    }, mm::page::Page, mutex::SpinNoIrqLock
};

use super::{
    block_group::GroupDesc, dentry::{self, Ext4DirEntry}, extent_tree::{Ext4Extent, Ext4ExtentHeader}, fs::Ext4FileSystem, super_block::{self, Ext4SuperBlock}
};

const EXT4_N_BLOCKS: usize = 15;

// File mode
const S_IXOTH: u16 = 0x1; // Others have execute permission
const S_IWOTH: u16 = 0x2; // Others have write permission
const S_IROTH: u16 = 0x4; // Others have read permission
const S_IXGRP: u16 = 0x8; // Group has execute permission
const S_IWGRP: u16 = 0x10; // Group has write permission
const S_IRGRP: u16 = 0x20; // Group has read permission
const S_IXUSR: u16 = 0x40; // Owner has execute permission
const S_IWUSR: u16 = 0x80; // Owner has write permission
const S_IRUSR: u16 = 0x100; // Owner has read permission
const S_ISVTX: u16 = 0x200; // Sticky bit
const S_ISGID: u16 = 0x400; // Set GID
const S_ISUID: u16 = 0x800; // Set UID

const S_IFDIR: u16 = 0x4000; // Directory

// inode flags
// const EXT4_SECRM_FL: u32 = 0x00000001; // Secure deletion
// const EXT4_UNRM_FL: u32 = 0x00000002; // Undelete
// const EXT4_COMPR_FL: u32 = 0x00000004; // Compress file
// const EXT4_SYNC_FL: u32 = 0x00000008; // Synchronous updates
// const EXT4_IMMUTABLE_FL: u32 = 0x00000010; // Immutable file
// const EXT4_APPEND_FL: u32 = 0x00000020; // writes to file may only append
// const EXT4_NODUMP_FL: u32 = 0x00000040; // do not dump file
// const EXT4_NOATIME_FL: u32 = 0x00000080; // do not update atime
// const EXT4_DIRTY_FL: u32 = 0x00000100;
// const EXT4_COMPRBLK_FL: u32 = 0x00000200; // One or more compressed clusters
// const EXT4_NOCOMPR_FL: u32 = 0x00000400; // Don't compress
// const EXT4_ECOMPR_FL: u32 = 0x00000800; // Compression error
pub const EXT4_INDEX_FL: u32 = 0x00001000; // hash indexed directory
pub const EXT4_EXTENTS_FL: u32 = 0x000080000; // Inode uses extents
pub const EXT4_INLINE_DATA_FL: u32 = 0x10000000; // Inode has inline data

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
// 注意Ext4Inode字段一共160字节, 但是sb.inode_size是256字节, 在计算偏移量时要注意使用sb的
pub struct Ext4InodeDisk {
    mode: u16,              // 文件类型和访问权限
    uid: u16,               // 文件所有者的用户ID(低16位)
    size_lo: u32,           // 文件大小(字节, 低32位)
    atime: u32,             // 最后访问时间
    change_inode_time: u32, // 最近Inode改变时间
    modify_file_time: u32,  // 最近文件内容修改时间
    dtime: u32,             // 删除时间
    gid: u16,               // 所属组ID(低16位)
    links_count: u16,       // 硬链接数
    blocks_lo: u32,         // 文件大小(块数, 以512字节为逻辑块, 如果设置EXT4_HUGE_FILE_FL, 才是fs_block size)
    flags: u32,             // 扩展属性标志
    osd1: u32,              // 操作系统相关
    // 符号链接: 目标字符串长度小于60字节, 直接存储在blocks中
    // ext2/3文件: 存储文件数据块指针, 0-11直接, 12间接, 13二级间接, 14三级间接
    block: [u32; EXT4_N_BLOCKS], // 文件数据块指针
    generation: u32,             // 文件版本(用于NFS)
    file_acl_lo: u32,            // 文件访问控制列表
    size_hi: u32,                // 文件大小(字节, 高32位)
    obso_faddr: u32,             // 已废弃碎片地址
    // 具体可能不是3个u32, 但是这里只是为了占位(大小是12字节)
    osd2: [u32; 3],               // 操作系统相关
    extra_isize: u16,             // Inode扩展大小
    checksum_hi: u16,             // CRC32校验和高16位
    change_inode_time_extra: u32, // 额外的Inode修改时间(nsec << 2 | epoch)
    modify_file_time_extra: u32,  // 额外的内容修改时间(nsec << 2 | epoch)
    atime_extra: u32,             // 额外的访问时间(nsec << 2 | epoch)
    create_time: u32,             // 文件创建时间
    create_time_extra: u32,       // 额外的创建时间(nsec << 2 | epoch)
    version_hi: u32,              // 文件版本(高32位)
    project_id: u32,              // 项目ID
}

// ext4中没有inode0, 根目录的inode number是2
// 定位inode的位置: 块组 + 组内偏移量
// 块组号 = (inode_number - 1) / inodes_per_group
// 组内偏移量 = (inode_number - 1) % inodes_per_group
impl Ext4InodeDisk {
    // group_desc: 对于root_inode来说, 是块组描述符表的第一个
    fn new_root(
        block_device: Arc<dyn BlockDevice>,
        // ext4_meta: &Ext4Meta,
        super_block: &Arc<Ext4SuperBlock>,
        group_desc: &Arc<GroupDesc>,
    ) -> Self {
        let root_ino = 2;
        let inode_table_block_id = group_desc.inode_table() as usize;
        let ext4_root_inode = get_block_cache(inode_table_block_id, block_device, super_block.block_size as usize)
            .lock()
            .read(
                (root_ino - 1) * super_block.inode_size as usize,
                |inode: &Ext4InodeDisk| inode.clone(),
            );
        ext4_root_inode
    }
}

/// 辅助函数
impl Ext4InodeDisk {
    /// 是否使用extent tree, 还是传统的12个直接块, 1个间接块, 1个二级间接块, 1个三级间接块
    fn use_extent_tree(&self) -> bool {
        self.flags & EXT4_EXTENTS_FL == EXT4_EXTENTS_FL
    }
    /// 是否有inline data
    fn has_inline_data(&self) -> bool {
        self.flags & EXT4_INLINE_DATA_FL == EXT4_INLINE_DATA_FL
    }
    /// 是否是目录
    fn is_dir(&self) -> bool {
        self.mode & S_IFDIR == S_IFDIR
    }
    fn flags(&self) {
        log::info!(
            "\thash indexed directory: {}",
            self.flags & EXT4_INDEX_FL == EXT4_INDEX_FL
        );
        log::info!(
            "\tinode uses extents: {}",
            self.flags & EXT4_EXTENTS_FL == EXT4_EXTENTS_FL
        );
        log::info!(
            "\tinode has inline data: {}",
            self.flags & EXT4_INLINE_DATA_FL == EXT4_INLINE_DATA_FL
        );
    }
    pub fn get_size(&self) -> u64 {
        (self.size_hi as u64) << 32 | self.size_lo as u64
    }
}

// Extent tree
impl Ext4InodeDisk {
    pub fn init_extent_tree(&mut self) {
        assert!(self.use_extent_tree(), "not use extent tree");
        // 初始化extent tree
        let header_ptr = self.block.as_mut_ptr() as *mut Ext4ExtentHeader;
        unsafe {
            header_ptr.write(Ext4ExtentHeader::default());
        }
    }
    fn extent_header(&self) -> Ext4ExtentHeader {
        assert!(self.use_extent_tree(), "not use extent tree");
        assert!(!self.has_inline_data());
        // extent_header是block的前12字节
        unsafe {
            let extent_header_ptr = self.block.as_ptr() as *const Ext4ExtentHeader;
            assert!((*extent_header_ptr).magic == 0xF30A, "magic number error");
            *extent_header_ptr
        }
    }
    fn extent_idxs(&self, extent_header: &Ext4ExtentHeader) -> Vec<Ext4ExtentIdx> {
        assert!(extent_header.depth > 0, "not index node");
        let mut extent_idx = Vec::new();
        // extent_idx是block的后4字节
        unsafe {
            let extent_idx_ptr = self.block.as_ptr().add(3) as *const Ext4ExtentIdx;
            for i in 0..extent_header.entries as usize {
                extent_idx.push(ptr::read(extent_idx_ptr.add(i as usize)));
            }
        }
        extent_idx
    }
    fn extents(&self, extent_header: &Ext4ExtentHeader) -> Vec<Ext4Extent> {
        assert!(extent_header.depth == 0, "not leaf node");
        let mut extents = Vec::new();
        unsafe {
            let extent_ptr = self.block.as_ptr().add(3) as *const Ext4Extent;
            for i in 0..extent_header.entries as usize {
                extents.push(ptr::read(extent_ptr.add(i as usize)));
            }
        }
        extents
    }

    /// 用于文件的读写, 
    /// 由上层调用者保证: 未命中页缓存时才调用
    /// logical_start_block: 逻辑块号(例. 文件的前4096字节对于ext4就是逻辑块号0的内容) 
    fn find_extent(
        &self,
        logical_start_block: u32,
        block_device: Arc<dyn BlockDevice>,
        ext4_block_size: usize,
    ) -> Result<Ext4Extent, &'static str> {
        let current_block = logical_start_block;

        // 获取根节点的extent_header
        let mut extent_header = self.extent_header();

        // 遍历extent B+树，直到找到所有需要的块范围
        while extent_header.depth > 0 {
            // 当前节点是索引节点
            let extent_idxs = self.extent_idxs(&extent_header);

            // 在索引节点中找到包含目标块的子节点
            if let Some(idx) = extent_idxs.iter().find(|idx| idx.block <= current_block) {
                let next_block = idx.physical_leaf_block();
                extent_header = 
                    // 加载子节点的ExtentHeader
                    get_block_cache(next_block, block_device.clone(), ext4_block_size)
                        .lock()
                        .read(0, |header: &Ext4ExtentHeader| header.clone());
            } else {
                // 未找到对应的索引节点
                return Err("extent not found");
            }
        }
        // 当前节点是叶子节点
        let mut extents = self.extents(&extent_header);

        // 遍历叶子节点的所有extent
        for extent in extents.drain(..) {
            let start_block = extent.logical_block;
            let end_block = start_block + extent.len as u32;
            if logical_start_block >= start_block && logical_start_block < end_block {
                log::info!(
                    "[Ext4InodeDisk::find_extent]: hit\nlogical_start_block: {}, start_block: {}, end_block: {}",
                    logical_start_block,
                    start_block,
                    end_block
                );
                return Ok(extent);
            }
        }
        return Err("extent not found");
    }
    // 用于目录的inode的`load_children_from_disk`
    fn read_all(&self, block_device: Arc<dyn BlockDevice>, ext4_block_size: usize) -> Vec<Ext4Extent> {
        assert!(self.is_dir(), "not a directory");
        // 使用队列来遍历extent tree
        let mut queue= VecDeque::new();
        queue.push_back(self.extent_header());
        let mut ret = Vec::new();

        while let Some(extent_header) = queue.pop_front() {
            if extent_header.depth > 0 {
                let extent_idxs = self.extent_idxs(&extent_header);
                for idx in extent_idxs.iter() {
                    let next_block = idx.physical_leaf_block();
                    let next_extent_header = get_block_cache(next_block, block_device.clone(), ext4_block_size)
                        .lock()
                        .read(0, |header: &Ext4ExtentHeader| header.clone());
                    queue.push_back(next_extent_header);
                }
            } else {
                let extents = self.extents(&extent_header);
                for extent in extents {
                    ret.push(extent);
                }
            }
        }
        // 由B+树的性质确保了, Extents是有序的(从小到大)
        ret 
    } 
}


pub struct Ext4Inode {
    pub ext4_fs: Weak<Ext4FileSystem>,
    pub block_device: Arc<dyn BlockDevice>,
    pub address_space: AddressSpace,
    pub inner: FSMutex<Ext4InodeInner>,
}

pub struct Ext4InodeInner {
    pub inode_on_disk: Ext4InodeDisk,
    pub state: InodeState,
}

impl Ext4InodeInner {
    pub fn new(inode_on_disk: Ext4InodeDisk) -> Self {
        Self {
            inode_on_disk,
            state: InodeState::Init,
        }
    }
}

impl Ext4Inode {
    /// used by `InodeOp ->create`
    pub fn new(inode_mode: u16, flags: u32, ext4_fs: Weak<Ext4FileSystem>, block_device: Arc<dyn BlockDevice>) -> Arc<Self> {
        // Todo: 1. init_owner(): 设置mode, uid, gid
        // Todo: 2. 时间戳: atime, mtime, ctime 
        // 3. 设置i_size = 0, i_blocks(逻辑块计数) = 0
        // 4. 设置flags, extent tree初始化
        let mut new_inode_disk = Ext4InodeDisk {
            mode: inode_mode,
            flags,
            ..Default::default()
        };
        assert!(flags & EXT4_EXTENTS_FL == EXT4_EXTENTS_FL, "not use extent tree");
        // 初始化extent tree
        new_inode_disk.init_extent_tree();
        Arc::new(
            Ext4Inode {
                ext4_fs,
                block_device,
                address_space: AddressSpace::new(),
                inner: FSMutex::new(Ext4InodeInner::new(new_inode_disk)),
            }
        )
    } 
    pub fn new_root(block_device: Arc<dyn BlockDevice>, ext4_fs: Arc<Ext4FileSystem>, group_desc: &Arc<GroupDesc>) -> Self {
        let super_block = &ext4_fs.super_block;
        let root_inode_disk = Ext4InodeDisk::new_root(block_device.clone(), super_block, group_desc);
        Self {
            ext4_fs: Arc::downgrade(&ext4_fs),
            block_device,
            address_space: AddressSpace::new(),
            inner: FSMutex::new(Ext4InodeInner::new(root_inode_disk)),
        }
    }
    // 所有的读/写都是基于Ext4Inode::read/write, 通过页缓存和extent tree来读写
    pub fn read(
        &self,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<usize, &'static str> {
        // 需要读取的总长度
        let rbuf_len = buf.len();
        let inode_size = self.inner.lock().inode_on_disk.size_lo as usize;

        // offset超出文件大小, 直接返回0(EOF)
        if offset >= inode_size {
            return Ok(0);
        }
        log::info!(
            "[Ext4Inode::read]: offset: {}, inode_size: {}, rbuf_len: {}",
            offset,
            inode_size,
            rbuf_len
        );

        // 先读取页缓存
        let mut current_read = 0;
        let mut page_offset = offset >> PAGE_SIZE_BITS;
        let mut page_offset_in_page = offset & (PAGE_SIZE - 1);

        let mut current_extent: Option<Ext4Extent> = None;
        let mut page: Arc<SpinNoIrqLock<Page>>;
        let mut fs_block_id: usize;

        while current_read < rbuf_len {
            if let Some(page_cache) = self.address_space.get_page_cache(page_offset) {
                // 页缓存命中
                page = page_cache;
            } else {
                // 页缓存未命中, 看是否在查到的PhysicalBlockRange中
                if let Some(extent) = &current_extent {
                    if (extent.logical_block + extent.len as u32) as usize
                        > page_offset 
                    {
                        // 命中extent读取, 知道对应的物理块号
                        fs_block_id = extent.physical_start_block() + page_offset
                            - extent.logical_block as usize;
                    } else { 
                        // 未命中, 从inode中读取extent
                        let extent = self.inner.lock().inode_on_disk.find_extent(page_offset as u32, self.block_device.clone(), self.ext4_fs.upgrade().unwrap().block_size())?;
                        fs_block_id = extent.physical_start_block() + page_offset
                            - extent.logical_block as usize;
                        current_extent= Some(extent);
                    }
                } else {
                    // 未命中, 从inode中读取extent
                        let extent = self.inner.lock().inode_on_disk.find_extent(page_offset as u32, self.block_device.clone(), self.ext4_fs.upgrade().unwrap().block_size())?;
                    fs_block_id = extent.physical_start_block() + page_offset
                        - extent.logical_block as usize;
                    current_extent = Some(extent);
                }
                page = self.address_space.new_page_cache(
                    page_offset,
                    fs_block_id,
                    self.block_device.clone(),
                );
            }
            // 计算本次能读取的长度, 不能超过文件大小
            let remaining_file_size = inode_size - (current_read + offset);
            let copy_len = (rbuf_len - current_read).min(PAGE_SIZE - page_offset_in_page).min(remaining_file_size);
            // 先读出一整页, 再从页中拷贝需要的部分到buf中
            page.lock().read(0, |data: &[u8; PAGE_SIZE]| {
                buf[current_read..current_read + copy_len]
                    .copy_from_slice(&data[page_offset_in_page..page_offset_in_page + copy_len]);
            });
            // 读取到文件末尾
            if remaining_file_size < PAGE_SIZE {
                return Ok(current_read);
            }
            current_read += copy_len;
            page_offset += 1;
            page_offset_in_page = 0;
        }
        log::info!(
            "[Ext4Inode::read]: current_read: {}",
            current_read
        );
        Ok(current_read)
    }
    /// Todo:
    pub fn write(&self, offset: usize, buf: &[u8]) -> usize {
        // 需要写回的总长度
        let wbuf_len = buf.len();
        // 先读取页缓存
        let mut current_write = 0;
        let mut page_offset = offset >> PAGE_SIZE_BITS;
        let mut page_offset_in_page = offset & (PAGE_SIZE - 1);

        let mut current_extent: Option<Ext4Extent> = None;
        let mut page: Arc<SpinNoIrqLock<Page>>;
        let mut fs_block_id: usize;

        while current_write < wbuf_len {
            if let Some(page_cache) = self.address_space.get_page_cache(page_offset) {
                // 页缓存命中
                page = page_cache;
            } else {
                // 页缓存未命中, 看是否在查到的PhysicalBlockRange中
                if let Some(extent) = &current_extent {
                    if (extent.logical_block + extent.len as u32) as usize
                        > page_offset 
                    {
                        // 命中extent读取, 知道对应的物理块号
                        fs_block_id = extent.physical_start_block() + page_offset
                            - extent.logical_block as usize;
                    } else { 
                        // 未命中, 从inode中读取extent
                        let extent = self.inner.lock().inode_on_disk.find_extent(page_offset as u32, self.block_device.clone(), self.ext4_fs.upgrade().unwrap().block_size()).unwrap();
                        fs_block_id = extent.physical_start_block() + page_offset
                            - extent.logical_block as usize;
                        current_extent= Some(extent);
                    }
                } else {
                    // 未命中, 从inode中读取extent
                        let extent = self.inner.lock().inode_on_disk.find_extent(page_offset as u32, self.block_device.clone(), self.ext4_fs.upgrade().unwrap().block_size()).unwrap();
                    fs_block_id = extent.physical_start_block() + page_offset
                        - extent.logical_block as usize;
                    current_extent = Some(extent);
                }
                page = self.address_space.new_page_cache(
                    page_offset,
                    fs_block_id,
                    self.block_device.clone(),
                );
            }
            let copy_len = (wbuf_len - current_write).min(PAGE_SIZE - page_offset_in_page);
            page.lock().modify(0, |data: &mut [u8; PAGE_SIZE]| {
                data[page_offset_in_page..page_offset_in_page + copy_len]
                    .copy_from_slice(&buf[page_offset_in_page..page_offset_in_page + copy_len]);
            });
            current_write += copy_len;
            page_offset += 1;
            page_offset_in_page = 0;
        }
        current_write

    }
    /// 只读取磁盘上的目录项, 不会加载Inode进入内存
    /// 上层调用者应优先使用DentryCache, 只有未命中时才调用
    pub fn lookup(&self, name: &str) -> Option<Ext4DirEntry> {
        log::info!("[Ext4Inode::lookup] name: {}", name);
        assert!(self.inner.lock().inode_on_disk.is_dir(), "not a directory");
        let dir_size = self.inner.lock().inode_on_disk.get_size();
        assert!(dir_size & (PAGE_SIZE as u64 - 1) == 0, "dir_size is not page aligned");
        let mut buf = vec![0u8; dir_size as usize];
        // buf中是目录的所有内容
        self.read(0, &mut buf).expect("read failed");
        let dir_content = Ext4DirContentRO::new(&buf);
        dir_content.find(name)
    }
    pub fn getdents(&self) -> Vec<Ext4DirEntry> {
        assert!(self.inner.lock().inode_on_disk.is_dir(), "not a directory");
        let dir_size = self.inner.lock().inode_on_disk.get_size();
        assert!(dir_size & (PAGE_SIZE as u64 - 1) == 0, "dir_size is not page aligned");
        let mut buf = vec![0u8; dir_size as usize];
        // buf中是目录的所有内容
        self.read(0, &mut buf).expect("read failed");
        let dir_content = Ext4DirContentRO::new(&buf);
        dir_content.list()
    }
}

impl Ext4Inode {
    // 目录项的插入
    pub fn add_entry(&self, dentry: Arc<Dentry>, inode_num: u32, file_type: u8) {
        assert!(self.inner.lock().inode_on_disk.is_dir(), "not a directory");
        let dir_size = self.inner.lock().inode_on_disk.get_size();
        assert!(dir_size & (PAGE_SIZE as u64 - 1) == 0, "dir_size is not page aligned");
        let mut buf = vec![0u8; dir_size as usize];
        // buf中是目录的所有内容
        self.read(0, &mut buf).expect("read failed");
        let mut dir_content = Ext4DirContentWE::new(&mut buf);
        // 更新目录内容, 以及可能目录会扩容, inode_on_disk的size会更新
        dir_content.add_entry(dentry.get_last_name(), inode_num, file_type).expect("Ext4Inode::add_entry failed");
        // 写回Block_Cache
        self.write(0, &buf);
    }
}

// set系列方法
impl Ext4Inode {
    fn set_mode(&self, mode: u16) {
        self.inner.lock().inode_on_disk.mode = mode | 0o777;
    }
    fn set_flags(&self, flags: u32) {
        self.inner.lock().inode_on_disk.flags = flags;
    }
}

// Todo: 支持inode_cache
// 上层调用者应保证: 1. inode_num是有效的 2. inode_num对应的inode未加载
pub fn load_inode(inode_num: usize, block_device: Arc<dyn BlockDevice>, ext4_fs: Arc<Ext4FileSystem>) -> Arc<Ext4Inode> {
    let inodes_per_group = ext4_fs.super_block.inodes_per_group as usize;
    let bg = (inode_num - 1) / inodes_per_group;
    let index = (inode_num - 1) % inodes_per_group;
    let inode_table_block_id = ext4_fs.block_groups[bg].inode_table() as usize;
    let inode_on_disk = get_block_cache(inode_table_block_id, block_device.clone(), ext4_fs.super_block.block_size as usize)
        .lock()
        .read(
            index * ext4_fs.super_block.inode_size as usize,
            |inode: &Ext4InodeDisk| inode.clone(),
        );
    log::info!("[load_inode] inode_num: {}, size: {}", inode_num, inode_on_disk.get_size());
    Arc::new(
        Ext4Inode {
            ext4_fs: Arc::downgrade(&ext4_fs),
            block_device,
            address_space: AddressSpace::new(),
            inner: FSMutex::new(Ext4InodeInner::new(inode_on_disk)),
        }
    )
}