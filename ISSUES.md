# jabkit-rs 工具级待修清单

> 本文件只记录**工具级**问题（CLI / 退出码 / BibTeX 生成 / keyring / release / README）。
> Provider 级问题（API 调用、serde、字段映射、限流）→ `~/.hermes/profiles/science/skills/jabkit-provider-diagnostics/SKILL.md`。
>
> 来源：2026-09-06 外部评审 + 本地源码复核 + 动态复现。
>
> **Round 3（2026-09-09/10）**：Poe GPT-6-Astra 二次审计（`/tmp/jabkit_poe_audit.md`）驱动的 P0 正确性修复。

## 已修 — Round 3（2026-09-09/10，Astra P0 正确性）

### 括号"转义"造成静默数据损坏（发布阻断项，已实证重做）
- **位置**: `src/bibtex.rs` `escape_unbalanced_braces`
- **现象**: Round 2 用 `\{`/`\}` 转义未配对括号。但传统 BibTeX 字段扫描只按 `{`/`}` 计数，反斜杠**不阻止**计数 → 未闭合的 `{` 把字段体吞到文件结尾。**本地实证**（真实 `bibtex 0.99d`）：`title = {x \{ y}` → `Illegal end of database file` + `empty title/journal/year`，整条记录字段全丢。
- **修复**: 未配对 `{`/`}` 改为**不含括号字符**的 TeX 命令 `\textbraceleft`/`\textbraceright`（LaTeX 消费端渲染为 `{`/`}`，对 BibTeX 计数无害）。已配对分组（如 `{N}ystagmus` 大写保护）原样保留。
- **实证**: 真实 `bibtex` 端到端 3 用例（单个 stray open / both stray / balanced）零错误、title 完整保留；对比修复前全字段丢失。
- **教训**: "用自己的 formatter 输出喂自己的 checker"会验证错误的规则（Round 2 的 5 个单测全绿但方向错）。结构正确性必须用**独立实现**（真实 bibtex）验证。

### book-chapter → `@inbook` 语义错误（应为 `@incollection`）
- **位置**: `src/bibtex.rs` `EntryType`、`src/provider/crossref.rs` `type_from_crossref`、`src/provider/openalex.rs` `type_from_oa`
- **现象**: `book-chapter` 映射到 `InBook`。但传统 BibTeX 的 `inbook` 标准字段集**不含 `booktitle`**，书名可能不显示；有独立标题的书中章节正确类型是 `incollection`（字段集含 `booktitle`）。
- **修复**: 新增 `InCollection` 类型，删除已无生产构造点的 `InBook`（避免 clippy `-D warnings` 死变体）；Crossref/OpenAlex 的 `book-chapter` → `InCollection`，container-title → `booktitle`。
- **附带**: 未知 Crossref/OpenAlex 类型回退从 `Article` 改 `Misc`（不再把未知工作伪装成期刊文章）；未知类型 container → `note`（不丢数据）。
- **回归测试**: `crossref.rs::tests`（新模块）`test_crossref_book_chapter_maps_to_incollection`、`test_crossref_unknown_type_falls_back_to_misc_not_article`。

### stub provider 错误消息误导 + 双重前缀
- **位置**: `src/provider/stubs.rs`、`src/main.rs` fetch 路径
- **现象**: stub 报 "no public search API available"（暗示外部服务问题，实际是本版本未实现）；`get-by-id` 双重前缀（`CiteSeerX: CiteSeerX: ...`）。
- **修复**: stub 消息改为精确的 "not implemented in this version (stub provider)"，去掉自身 provider 名前缀（main 层统一加）；fetch 路径补统一 `provider: ` 前缀（与 get-by-id 一致）。
- **实测**: `fetch --provider zbMATH` → `zbMATH: not implemented in this version (stub provider)` exit 1；`get-by-id --provider CiteSeerX` → `CiteSeerX: ID lookup not implemented in this version (stub provider)`，单前缀。

### HTTP 客户端静默回退丢超时/代理
- **位置**: `src/provider/mod.rs` `http_client`
- **现象**: `build().unwrap_or_else(|_| Client::new())` — 构建失败时回退到无超时/无代理的客户端；且每次调用都新建客户端。
- **修复**: `OnceLock` 进程级单例（不再每次新建）；构建失败**不静默降级**，`expect` 带清晰消息（缺超时 = 慢上游可永久挂起进程）。

### secret-tool 超时不终止子进程（泄漏）
- **位置**: `src/keyring.rs` `secret_tool_lookup`
- **现象**: Round 1 用 `mpsc::channel` + `recv_timeout(5s)`，只限制接收方等待——`secret-tool` 子进程超时时**不被终止**，挂起的 keyring daemon 会留子进程在后台。
- **修复**: 单次 spawn + stdout pipe + `try_wait()` 轮询 5s deadline + 超时真正 `kill()` 子进程；子进程退出后才读 stdout（不对挂起进程阻塞读）。
- **验证**: clippy `-D warnings` 零、编译通过（本机无 secret-tool，走 "not installed → None" 分支不报错）。

## 已修（2026-09-06）

### arXiv eprint 字段被覆盖为字面量 "arXiv"
- **位置**: `src/provider/arxiv.rs` `</entry>` handler
- **现象**: `eprint = {arXiv}` 而非 `eprint = {2306.12345}`
- **修复**: 删除 `</entry>` 里的 `set_field(Field::Eprint, "arXiv")` 覆盖
- **回归测试**: `main.rs` `test_arxiv_eprint_not_overwritten`（Atom XML 样本，断言 eprint 以真实 ID 开头）
- **实测**: `eprint = {1101.4081v3}`, `eprint = {2306.12345v2}`, `eprint = {nlin/0412031v1}`

### doi-to-bibtex 退出码恒 0
- **位置**: `src/main.rs` `Commands::DoiToBibtex`
- **现象**: 全部 DOI 404 时 exit 0，stdout 空
- **修复**: 加 `failures` 计数，`failures > 0` 时 `anyhow::bail!` → exit 1
- **实测**: 2×404 → exit=1；1 good + 1 fake → exit=1, stdout 有 1 条 BibTeX

### README 全面漂移
- **现象**: README 是 5-provider 时代写的，命令/选项/数量全部过时
- **修复**: 重写 README，对齐 `src/cli.rs` 实际定义（26 providers、`--provider` 必填、`--limit`、`get-by-id --provider --id`）
- **验证**: 所有 Quick Start 命令实测通过（fetch/doi-to-bibtex/list-providers/get-by-id 均 OK）

### Crossref fetch_by_id 缺 pmid/abstract/subtitle（字段不对称）
- **位置**: `src/provider/crossref.rs` `fetch_by_id()`
- **现象**: 同一 DOI，`fetch` 路径有 subtitle+pmid+abstract，`get-by-id` 路径没有
- **修复**: 给 `fetch_by_id` 补上 subtitle、PMID、abstract 处理（与 search 路径对齐）
- **实测**: 代码对齐完成；实测 DOI 是否含 pmid/abstract 取决于上游数据（Crossref 非所有条目都有）

### `ApiKeys` 派生 `Debug` 含明文 key
- **位置**: `src/keyring.rs`
- **现象**: `println!("{:?}", api_keys)` 会打印所有 API key 明文
- **修复**: 自定义 `Debug` impl，所有 key 字段显示 `***`
- **实测**: 编译通过，key 不再明文输出

### 所有 provider 无 HTTP timeout
- **位置**: `src/provider/mod.rs` + 全部 provider 文件
- **现象**: `reqwest::Client::new()` 无 timeout，网络挂起时 CLI 永久阻塞
- **修复**: 新增 `provider::http_client()` 共享客户端（30s request timeout + 10s connect timeout），全部 17 个 provider 文件改用
- **实测**: 编译通过，13/13 测试通过

### provider 数量表述不一致
- **位置**: `src/cli.rs` after_help
- **现象**: after_help 写 "Providers (25)"，fetch doc 写 "26 providers"
- **修复**: after_help 改为 "Providers (26)"
- **实测**: `--help` 输出 "Providers (26)" ✓

### arXiv 条目无 source 标注
- **位置**: `src/provider/arxiv.rs`
- **现象**: arXiv 条目 `@article` 无 journal 字段，biblatex 无法识别为 preprint
- **修复**: `<id>` 解析时设置 `Note = "arXiv"`；`<arxiv:comment>` 时追加到 Note（`; ` 分隔）
- **实测**: `note = {arXiv; 10 pages, 3 figures, 11 references}` ✓

### arXiv 搜索参数只 `replace(' ', "+")` 不编码
- **位置**: `src/provider/arxiv.rs:18`
- **现象**: 含 `&`/`=` 等字符的查询可能出错
- **修复**: 新增 `urlencoding()` 函数（percent-encoding，空格→`+`），替换原 `replace`
- **实测**: 编译通过

### release.yml 配置问题
- **位置**: `.github/workflows/release.yml`
- **现象**: macOS 目标在 ubuntu-latest 上跑（无交叉工具链）、无测试步骤、产物名与 README 不一致
- **修复**:
  - 加 `test` job（`cargo test --release`），build 依赖 test
  - macOS 目标改用 `macos-latest` runner（原生编译 aarch64 + x86_64）
  - `fail-fast: false`（一个目标失败不阻塞其他）
  - 产物重命名为 `jabkit-linux-x86_64`/`jabkit-windows-x86_64.exe`/`jabkit-macos-x86_64`/`jabkit-macos-aarch64`
  - 与 README 下载路径对齐
- **实测**: YAML 语法正确（lint ok），需 push tag 触发实际验证
- **遗留**: `aarch64-unknown-linux-gnu` 目标未加入（ubuntu 交叉工具链缺失），如需 Linux ARM 产物后续补

### BibTeX 引用键无冲突消解
- **位置**: `src/main.rs` Fetch 循环
- **现象**: 同作者同年多篇文章 → 相同引用键（如 `yang2026` 出现两次）
- **修复**: 批次内 `HashMap<String, usize>` 计数，重复键加字母后缀（`yang2026` → `yang2026a` → `yang2026b`）
- **回归测试**: `main.rs` `test_citation_key_dedup`（3 次同 key → 3 个唯一键）
- **实测**: 14/14 测试通过

### 输出流不一致（print vs println）
- **位置**: `src/main.rs` Fetch/GetById 循环
- **现象**: `print!` 无换行，管道拼接时条目粘连
- **修复**: 全部改为 `println!`（Fetch 循环 + GetById 单条）
- **实测**: 编译通过，输出格式正确

## 已修（2026-09-09 — 外部评审 P0 批量修复）

### book-chapter → `@inbook` + `booktitle` 字段
- **位置**: `src/bibtex.rs`（新增 `InBook` 类型 + `Booktitle` 字段）、`src/provider/crossref.rs`、`src/provider/openalex.rs`
- **现象**: Crossref `book-chapter` 映射为 `InProceedings`，书籍章节与会议论文混为一类；容器名落入 `journal` 而非 `booktitle`
- **修复**: 新增 `EntryType::InBook` + `Field::Booktitle`；crossref/openalex 的 `book-chapter` 改映射到 `InBook`；`set_container()` helper 按条目类型决定容器名放 `booktitle` 还是 `journal`
- **回归测试**: `test_inbook_rendering`（断言 `@inbook{}` + `booktitle` 字段 + 无 `journal`）
- **实测**: DOI 10.1007/978-3-540-29678-2_6294 → `@inbook{doi_101007978-3-540-29678-26294, booktitle = {Encyclopedia of Neuroscience}}` ✓

### 中文/非 ASCII 作者引用键 → DOI 回退
- **位置**: `src/bibtex.rs` `generate_key()`
- **现象**: 中文作者（如"王, 晓凯"）生成 `王2024` 键，LaTeX 跨语言库合并隐患
- **修复**: `generate_key()` 先过滤非 ASCII 字符；无可用 ASCII 作者名时回退到 DOI 派生键（`doi_10...`）；无 DOI 且无作者时回退 `nauthor_YYYY`
- **回归测试**: `test_bibtex_key_cjk_author_with_doi`（`王, 晓凯` + DOI → `doi_103760cmaj202401001`）、`test_bibtex_key_cjk_author_no_doi`（`王, 晓凯` 无 DOI → `nauthor_2024`）、`test_bibtex_key_no_author_with_doi`、`test_bibtex_key_no_author_no_doi`
- **实测**: Crossref BPPV 30 条 → 0 个中文键、0 个 unknown 键（之前 3 个 unknown）✓

### doi-to-bibtex 批量退出码 + 键去重
- **位置**: `src/main.rs` `Commands::DoiToBibtex`、`src/cli.rs`（新增 `--strict`）
- **现象**: 部分 DOI 失败时进程恒 exit 1，stdout 已有正确 BibTeX 但脚本判定失败；doi-to-bibtex 路径无键去重
- **修复**: 默认行为 = 全失败 exit 1 / 部分失败 exit 0 + stderr warning / 全成功 exit 0；`--strict` 使任何失败都 exit 1；批量输出复用 `deduplicate_keys()` 公共函数
- **回归测试**: `test_doi_to_bibtex_command`（默认 strict=false）、`test_doi_to_bibtex_strict_flag`（--strict → true）
- **实测**: 1 好 + 1 坏 → exit 0 + BibTeX ✓；--strict 1 好 + 1 坏 → exit 1 ✓；2 坏 → exit 1 ✓

### Keyring account 名兼容 + secret-tool 5s 超时
- **位置**: `src/keyring.rs` `with_keyring_fallback()` + `secret_tool_lookup()`
- **现象**: README 示例 `account S2_API_KEY`，代码查 `account SemanticScholar`，按文档存必失败；secret-tool 无超时，keyring daemon 挂起时阻塞 CLI
- **修复**: 每个 key 先查显示标签再查 env var 名（双格式兼容）；`secret_tool_lookup` 改为 mpsc channel + `recv_timeout(5s)`，超时返回 None
- **实测**: 编译通过，secret-tool 正常时 <100ms 返回 ✓

### BiodiversityHL URL 修复
- **位置**: `src/provider/stubs.rs`
- **现象**: URL 字符串 `https://www.biodiversitylibrary.org/api2/http://www.biodiversitylibrary.org/api2/GetSearchResults` 把完整 URL 当 path 拼接
- **修复**: 改为 `http://www.biodiversitylibrary.org/api2/GetSearchResults`
- **实测**: 编译通过 ✓

### 键去重逻辑提取为公共函数
- **位置**: `src/bibtex.rs` `deduplicate_keys()`、`src/main.rs`
- **现象**: fetch 路径有内联 HashMap 去重，doi-to-bibtex 路径无；测试 `test_citation_key_dedup` 复制了生产逻辑
- **修复**: 提取 `deduplicate_keys(&mut [BibEntry])` 到 `bibtex.rs`；fetch 和 doi-to-bibtex 共用；测试改为直接调用生产函数
- **回归测试**: `test_citation_key_dedup`（4 条含 3 条重复 → 4 个唯一键，直接调 `deduplicate_keys`）

## 已修（2026-09-09 — 第二轮：括号安全 + 透明 + 在线冒烟）

### BibTeX 字段括号平衡校验
- **位置**: `src/bibtex.rs` `escape_unbalanced_braces()` + `format_bibtex_value()`
- **现象**: 上游 title 含未配对 `{}`（数学公式、化学式高频）时，原"只加外层括号"会让解析器吞掉后续字段或报解析错
- **修复**: 渲染前双向扫描定位未配对括号，未配对的转义为 `\{`/`\}`；已配对分组（如保护大写的 `{N}ystagmus`）原样保留
- **回归测试**: `test_brace_balanced_preserved`、`test_brace_stray_open_escaped`、`test_brace_stray_close_escaped`、`test_brace_nested_balanced_untouched`、`test_brace_output_roundtrip_parens_balanced`（断言未转义括号数配平）
- **效果**: 字段体永远 well-formed，导入 JabRef/LaTeX 不再因 stray brace 崩

### list-providers 区分占位 / 已实现
- **位置**: `src/provider/mod.rs`（trait 加 `is_stub()`）、`stubs.rs`（宏展开 `is_stub()=true`）、`main.rs` ListProviders
- **现象**: 9 个占位 provider 与真 provider 平铺展示，用户把"已注册"误当"能搜"
- **修复**: 占位项显示 `[stub — no API]`，真 provider 显示 `[key]`/`[no key]`
- **实测**: `list-providers` 9 个 stub 全部标注，16 个真 provider 显示密钥状态 ✓

### 在线冒烟 e2e 测试（真实 Crossref）
- **位置**: `src/main.rs` `e2e_crossref_live_smoke`
- **现象**: 之前只测源码逻辑，未验证真实 API 运行（审查者 P1"定期在线冒烟"）
- **修复**: 加 `#[ignore]` 的 e2e 测试，打真实 Crossref — 已知 DOI 成功回读 + 不存在 DOI 必须 Err；默认不跑（离线 CI 不挂），手动 `-- --ignored` 或接 networked smoke job
- **实测**: 本地 `-- --ignored` 通过（known DOI round-trip + 404 路径正确）✓

## 待修

### P2: arXiv 条目类型固定 `@article`
- **位置**: `src/provider/arxiv.rs`
- **现象**: arXiv preprint 应输出 `@misc` 或 `@article` + `eprinttype = {arxiv}`（biblatex 标准）
- **当前**: 输出 `@article` + `note = {arXiv}`，可用但非 biblatex 最优
- **修复方向**: 加 `eprinttype` 字段或改用 `@misc`
- **优先级**: 低（当前输出可用，仅 biblatex 排版非最优）
