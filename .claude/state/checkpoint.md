当前状态：Phase 2 - Iter 04 - Step 7（docs completed, ready to git push）
上一步完成：文档更新（progress.md、lesson_learned.md、structure.md、tutorial.md、README.md）
下一步待做：git commit + git push
最后更新：2026-03-27 21:20

## 本次迭代完成内容

### 代码修改
1. **timeline.rs** — Braille 2-per-char 柱状图，y_max×1.15，30s 泄漏检测，峰值指示
2. **hotspot.rs** — format_file_line 分级回退，detect_lang，SKIP_CONTAINS/PREFIX 分离
3. **sampler.rs** — 帧质量保留（only overwrite if new frames have file info），alloc_events 计数
4. **symbols.rs** — PIE load_base 修正，binary_name_for_ip fallback，ALLOCMAP_DEBUG_SYMBOLS

### 测试程序
5. **spike_alloc** — 重写为 4 函数 50MB-1GB 随机分配
6. **alloc_c** — C 测试程序（gcc -g -O0 预编译）
7. **alloc_cpp** — C++ 测试程序（g++ -g -O0 预编译）
8. **alloc_go** — Go 测试程序源码（需 go build）

### 验证结果
- cargo build --release: PASSED
- cargo clippy -- -D warnings: 0 warnings
- cargo test: 68/68 PASSED
- ALLOCMAP_DEBUG_SYMBOLS=1: 正确显示 spike_alloc::function_large_alloc at src/main.rs:53
- JSON report: Site 0-3 正确显示 spike_alloc 用户代码帧（不再是 <system>）
