use alloc::{
    collections::vec_deque::VecDeque,
    format,
    string::{String, ToString},
    sync::{Arc, Weak},
};
use hashbrown::HashMap;
use lazy_static::lazy_static;

use crate::{ext4::dentry::Ext4DirEntry, mutex::SpinNoIrqLock};

use super::inode::InodeOp;

// VFS层的统一目录项结构
#[repr(C)]
pub struct Dentry {
    pub absolute_path: String,
    pub inode_num: usize,
    pub inner: SpinNoIrqLock<DentryInner>,
}

pub struct DentryInner {
    // None 表示该 dentry 未关联 inode
    pub inode: Option<Arc<dyn InodeOp>>,
    // pub inode: Option<Arc<SpinNoIrqLock<OSInode>>>,
    pub parent: Option<Weak<Dentry>>,
    // chrildren 是一个哈希表, 用于存储子目录/文件, name不是绝对路径
    pub children: HashMap<String, Arc<Dentry>>,
}

impl DentryInner {
    pub fn new(parent: Option<Arc<Dentry>>, inode: Arc<dyn InodeOp>) -> Self {
        Self {
            inode: Some(inode),
            parent: parent.map(|p| Arc::downgrade(&p)),
            children: HashMap::new(),
        }
    }
    // 负目录项
    pub fn negative(parent: Option<Arc<Dentry>>) -> Self {
        Self {
            inode: None,
            parent: parent.map(|p| Arc::downgrade(&p)),
            children: HashMap::new(),
        }
    }
}

impl Dentry {
    pub fn zero_init() -> Self {
        Self {
            absolute_path: String::new(),
            inode_num: 0,
            inner: SpinNoIrqLock::new(DentryInner::negative(None)),
        }
    }
    pub fn new(
        absolute_path: String,
        inode_num: usize,
        parent: Option<Arc<Dentry>>,
        inode: Arc<dyn InodeOp>,
    ) -> Arc<Self> {
        Arc::new(Self {
            absolute_path,
            inode_num,
            inner: SpinNoIrqLock::new(DentryInner::new(parent, inode)),
        })
    }
    pub fn negative(absolute_path: String, parent: Option<Arc<Dentry>>) -> Arc<Self> {
        Arc::new(Self {
            absolute_path,
            inode_num: 0,
            inner: SpinNoIrqLock::new(DentryInner::negative(parent)),
        })
    }
    // // 上层调用者保证由负目录项调用
    // pub fn associate(&mut self, inode_num: usize, inode: Arc<dyn InodeOp>) {
    //     self.inner.lock().inode = Some(inode);
    //     self.inode_num = inode_num;
    // }
    pub fn is_negative(&self) -> bool {
        self.inner.lock().inode.is_none()
    }
    pub fn get_last_name(&self) -> &str {
        self.absolute_path
            .split('/')
            .last()
            .unwrap_or(&self.absolute_path)
    }
    // 上层调用者保证: 负目录项不能调用该函数
    pub fn get_inode(&self) -> Arc<dyn InodeOp> {
        self.inner.lock().inode.clone().unwrap()
    }
    pub fn get_parent(&self) -> Arc<Dentry> {
        self.inner
            .lock()
            .parent
            .clone()
            .map(|p| p.upgrade().unwrap())
            .unwrap()
    }
}

lazy_static! {
    pub static ref DENTRY_CACHE: SpinNoIrqLock<DentryCache> =
        SpinNoIrqLock::new(DentryCache::new(1024));
}

pub fn lookup_dcache_with_absolute_path(absolute_path: &str) -> Option<Arc<Dentry>> {
    DENTRY_CACHE.lock().get(absolute_path)
}

pub fn lookup_dcache(parent: &Arc<Dentry>, name: &str) -> Option<Arc<Dentry>> {
    let absolute_path = format!("{}/{}", parent.absolute_path, name);
    DENTRY_CACHE.lock().get(&absolute_path)
}

pub fn insert_dentry(dentry: Arc<Dentry>) {
    DENTRY_CACHE
        .lock()
        .insert(dentry.absolute_path.clone(), dentry);
}

// 哈希键是由父目录的地址和当前文件名生成的, 确保全局唯一性
pub struct DentryCache {
    cache: SpinNoIrqLock<HashMap<String, Arc<Dentry>>>,
    // 用于LRU策略的列表
    lru_list: SpinNoIrqLock<VecDeque<String>>,
    capacity: usize,
}

impl DentryCache {
    fn new(capacity: usize) -> Self {
        DentryCache {
            cache: SpinNoIrqLock::new(HashMap::new()),
            lru_list: SpinNoIrqLock::new(VecDeque::new()),
            capacity,
        }
    }

    fn get(&self, absolute_path: &str) -> Option<Arc<Dentry>> {
        let mut lru_list = self.lru_list.lock();
        if let Some(dentry) = self.cache.lock().get(absolute_path) {
            // 更新 LRU 列表
            if let Some(pos) = lru_list.iter().position(|x| x == absolute_path) {
                lru_list.remove(pos);
            }
            lru_list.push_back(absolute_path.to_string());
            return Some(Arc::clone(dentry));
        }
        None
    }

    fn insert(&self, absolute_path: String, dentry: Arc<Dentry>) {
        let mut cache = self.cache.lock();
        let mut lru_list = self.lru_list.lock();

        // 如果已经存在，则更新
        if cache.contains_key(&absolute_path) {
            if let Some(pos) = lru_list.iter().position(|x| x == &absolute_path) {
                lru_list.remove(pos);
            }
        } else if cache.len() == self.capacity {
            // 缓存已满，移除最旧的
            if let Some(oldest) = lru_list.pop_front() {
                cache.remove(&oldest);
            }
        }

        cache.insert(absolute_path.clone(), Arc::clone(&dentry));
        lru_list.push_back(absolute_path);
    }
}
