/// allocmap-preload：LD_PRELOAD 注入库
///
/// # 安全警告
/// 本 crate 编译为 .so，通过 LD_PRELOAD 注入目标进程。
/// 在 malloc/free hook 内部，绝对不能调用标准 allocator，
/// 否则会产生无限递归导致栈溢出。
/// 所有内部内存分配必须通过 bump_alloc 模块。

pub mod hooks;
pub mod ipc;
pub mod bump_alloc;

/// .so 初始化入口，在库被加载时自动调用
#[no_mangle]
pub extern "C" fn allocmap_init() {
    // TODO: 初始化 IPC channel，连接到 allocmap-cli 进程
    // 通过环境变量 ALLOCMAP_SOCKET_PATH 获取 socket 路径
}
